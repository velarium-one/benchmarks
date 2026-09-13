use config::ffi::{ErrorHandle, ErrorView, Bytes};
use crate::Engine;

/// Client-owned failure; its text survives engine release and subsequent invocation preparation.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{stage}: {message} (code {code})")]
pub struct Error {
    pub code: u64,
    pub stage: String,
    pub message: String,
    pub source_pc: Option<u64>,
    /// Whether native entry started, when the failing operation knows it.
    pub execution_entry: u64,
}
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn contract(message: impl Into<String>) -> Self {
        Self {
            code: 4,
            stage: "client".into(),
            message: message.into(),
            source_pc: None,
            execution_entry: config::ffi::ENTRY_NOT_APPLICABLE,
        }
    }
}

// A trusted admitted engine establishes provenance, initialization and lifetime. Validate only
// representable descriptor errors here; zero bytes never forms a null slice.
pub(crate) unsafe fn bytes<'a>(view: Bytes) -> Result<&'a [u8]> {
    let len = usize::try_from(view.len).map_err(|_| Error::contract("byte length exceeds host"))?;
    if len > isize::MAX as usize {
        return Err(Error::contract("byte extent exceeds slice limit"));
    }

    if len == 0 {
        return Ok(&[]);
    }
    if view.ptr.is_null() {
        return Err(Error::contract("nonempty byte view is null"));
    }

    Ok(unsafe { std::slice::from_raw_parts(view.ptr, len) })
}

impl Engine {
    pub(crate) fn finish(&self, status: u64, error: *mut ErrorHandle) -> Result<()> {
        struct OwnedError<'a>(&'a Engine, *mut ErrorHandle);
        impl Drop for OwnedError<'_> {
            fn drop(&mut self) {
                unsafe { (self.0.api().release_error)(self.1); }
            }
        }

        // Retain release responsibility before admitting either success or failure.
        let _record = OwnedError(self, error);
        if status == 0 {
            return if error.is_null() {
                Ok(())
            } else {
                Err(Error::contract("success returned an error record"))
            };
        }
        if error.is_null() {
            return Err(Error::contract(format!("status {status} omitted error record")));
        }

        // Borrow the engine's failure record only after the getter establishes its contents.
        let mut view = std::mem::MaybeUninit::<ErrorView>::uninit();
        let viewed = unsafe { (self.api().error_view)(error, view.as_mut_ptr()) };
        if viewed != 0 {
            return Err(Error::contract("cannot read engine error record"));
        }
        let view = unsafe { view.assume_init() };

        // Copy diagnostic text into client ownership before releasing the engine record.
        let text = |input| -> Result<String> {
            let bytes = unsafe { bytes(input)? };
            String::from_utf8(bytes.to_vec()).map_err(|_| Error::contract("engine error is not UTF-8"))
        };
        let stage = text(view.stage)?;
        let message = text(view.message)?;
        let source_pc = match view.source_pc_present {
            0 => None,
            1 => Some(view.source_pc),
            _ => return Err(Error::contract("invalid source-PC presence")),
        };

        Err(Error {
            code: view.code,
            stage,
            message,
            source_pc,
            execution_entry: view.execution_entry,
        })
    }
}
