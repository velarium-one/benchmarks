//! Client of the closed standalone runtime's binary API.

mod error;
mod program;
mod session;
mod invocation;
pub use error::{Error, Result};
pub use program::Program;
pub use session::Session;
pub use invocation::{ClientBindings, PreparedInvocation, InvocationResults, ResultView, Counter, Outcome};
/// Public declarations consumed by this client, including its exact callback wire types.
pub use config;

use config::ffi::{ABI_REVISION, BOOTSTRAP_REVISION_MISMATCH, BOOTSTRAP_SUCCESS, GetApi, RuntimeApi};
use libloading::Library;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("engine path must be absolute: {0}")]
    RelativePath(PathBuf),
    #[error("cannot load engine {path}: {source}")]
    Library { path: PathBuf, source: libloading::Error },
    #[error("engine is missing vlrts_get_api: {0}")]
    MissingBootstrap(libloading::Error),
    #[error("engine rejected ABI revision {ABI_REVISION}")]
    RevisionMismatch,
    #[error("engine bootstrap failed with status {0}")]
    BootstrapStatus(u64),
    #[error("engine returned success without an API table")]
    MissingTable,
    #[error("engine returned a misaligned API table")]
    MisalignedTable,
    #[error("engine returned table revision {0}, expected {ABI_REVISION}")]
    TableRevision(u64),
}

/// [nb:core] Loaded engine and its admitted API table, retained as one lifetime owner.
/// No table reference or function pointer escapes independently of the library.
#[derive(Clone)]
pub struct Engine(Rc<EngineInner>);

struct EngineInner {
    api: NonNull<RuntimeApi>,
    _library: Library,
    // The standalone execution contract is single-threaded, even with coexisting engines.
    _calling_thread: PhantomData<Rc<()>>,
}

impl Engine {
    /// Exact build-owned engine path, also useful for provenance and direct launch.
    pub fn acquired_path() -> &'static Path {
        Path::new(env!("VLR_ENGINE_PATH"))
    }
    /// Loads the trusted engine selected by this client's build; no ambient path search is used.
    pub fn acquired() -> std::result::Result<Self, LoadError> {
        unsafe { Self::load(Self::acquired_path()) }
    }

    /// [nb:entry] Loads an exact trusted engine path and admits its public ABI revision.
    ///
    /// # Safety
    /// Loading executes native initializers. The library must be trusted and obey the bootstrap
    /// contract, including the validity of a successful table. Revision checks are not a sandbox.
    pub unsafe fn load(path: &Path) -> std::result::Result<Self, LoadError> {
        // Establish exact artifact ownership before resolving the stable bootstrap.
        if !path.is_absolute() {
            return Err(LoadError::RelativePath(path.to_owned()));
        }

        // Linux policy: Vehicles bind runtime support from this retained engine's global scope.
        // Program admission rejects an earlier engine/executable that owns those symbols.
        let loading_policy = libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL;
        let library = unsafe { libloading::os::unix::Library::open(Some(path), loading_policy) }
            .map(Library::from)
            .map_err(|source| LoadError::Library { path: path.to_owned(), source })?;

        let get_api = unsafe { library.get::<GetApi>(b"vlrts_get_api\0") }
            .map_err(LoadError::MissingBootstrap)?;

        // Check the revision-independent status before interpreting any table memory.
        let mut output: *const c_void = std::ptr::null();
        let status = unsafe { get_api(ABI_REVISION, &mut output) };
        match status {
            BOOTSTRAP_SUCCESS => {}
            BOOTSTRAP_REVISION_MISMATCH => return Err(LoadError::RevisionMismatch),
            other => return Err(LoadError::BootstrapStatus(other)),
        }

        // Admit the table, then retain it with the library that supplies its code and storage.
        let api = NonNull::new(output.cast::<RuntimeApi>().cast_mut())
            .ok_or(LoadError::MissingTable)?;
        if !api.as_ptr().is_aligned() {
            return Err(LoadError::MisalignedTable);
        }

        // invariant: a successful trusted bootstrap supplied a valid matching-layout table.
        let table_revision = unsafe { api.as_ref().abi_revision };
        if table_revision != ABI_REVISION {
            return Err(LoadError::TableRevision(table_revision));
        }

        let engine = EngineInner {
            api,
            _library: library,
            _calling_thread: PhantomData,
        };
        Ok(Self(Rc::new(engine)))
    }

    pub fn abi_revision(&self) -> u64 {
        // invariant: self owns the library throughout this table access.
        self.api().abi_revision
    }

    fn api(&self) -> &RuntimeApi {
        unsafe { self.0.api.as_ref() }
    }
}
