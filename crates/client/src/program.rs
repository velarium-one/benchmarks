use std::{os::unix::ffi::OsStrExt, path::Path, ptr, rc::Rc};
use config::{compile::CompileConfig, ffi::{Bytes, CompilationRecord, ProgramHandle}};
use crate::{Engine, Error, Result};

/// [nb:core] Loaded Vehicle retained with the exact engine that owns and releases it.
#[derive(Clone)]
pub struct Program(pub(crate) Rc<ProgramInner>);
pub(crate) struct ProgramInner {
    pub raw: *mut ProgramHandle,
    pub engine: Engine,
}

impl Drop for ProgramInner {
    fn drop(&mut self) {
        let mut error = ptr::null_mut();
        let status = unsafe { (self.engine.api().release_program)(&mut self.raw, &mut error) };

        self.engine.finish(status, error).expect("safe Program release invariant");
    }
}

impl Engine {
    /// Delete, compile, then publish one Vehicle; no generated source crosses this boundary.
    pub fn compile_program(&self, elf: &Path, config: &CompileConfig, output: &Path) -> Result<CompilationRecord> {
        let json = config.to_json().map_err(|e| Error::contract(e.to_string()))?;
        let elf_path = Bytes::borrowed(elf.as_os_str().as_bytes());
        let configuration = Bytes::borrowed(&json);
        let output_path = Bytes::borrowed(output.as_os_str().as_bytes());

        let mut record = CompilationRecord::default();
        let mut error = ptr::null_mut();
        let status = unsafe {
            (self.api().compile_program)(elf_path, configuration, output_path, &mut record, &mut error)
        };
        self.finish(status, error)?;

        Ok(record)
    }

    /// Load a locally compiled matching Vehicle without allocating its session.
    ///
    /// # Safety
    /// This is native code loading, including initializers. The artifact must be trusted and
    /// match this engine's current Vehicle calling/memory contract; no persisted hash check exists.
    pub unsafe fn prepare_program(&self, path: &Path) -> Result<Program> {
        unsafe { self.prepare_program_with_pinning(path, false) }
    }

    /// Experimental affinity selection; compilation should precede pinning.
    /// # Safety
    /// Same trusted/matching native-code contract as prepare_program.
    pub unsafe fn prepare_program_with_pinning(&self, path: &Path, pin: bool) -> Result<Program> {
        let vehicle_path = Bytes::borrowed(path.as_os_str().as_bytes());
        let pin_thread = u64::from(pin);

        let mut raw = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe {
            (self.api().prepare_program)(vehicle_path, pin_thread, &mut raw, &mut error)
        };
        self.finish(status, error)?;

        if raw.is_null() {
            return Err(Error::contract("program success omitted handle"));
        }

        let program = ProgramInner { raw, engine: self.clone() };
        Ok(Program(Rc::new(program)))
    }
}
