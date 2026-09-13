//! [nb:core] Client-owned demo dialect over preloaded snapshots and invocation-local handles.
//! The runtime receives only direct callbacks/context, never resource ownership or selectors.

use std::ffi::c_void;
use client::{ClientBindings, config::ffi::*};
use crate::cases::Case;

// Dialect result words, distinct from HostCallResult's continuation/termination status.
// These values match FileStatus in guest/common/kernel/src/lib.rs; that crate is RV32-only.
mod status {
    pub const SUCCESS: u32 = 0;
    pub const INVALID_PATH: u32 = 1;
    pub const NOT_FOUND: u32 = 2;
    pub const ASSET_TOO_LARGE: u32 = 4;
    pub const UNKNOWN_HANDLE: u32 = 5;
    pub const LENGTH_MISMATCH: u32 = 6;
    pub const INVALID_ARGUMENTS: u32 = 9;
    pub const PROTOCOL_ERROR: u32 = 10;
}

pub struct Provider<'a> {
    case: &'a Case,
    input: &'a [u8],
    handles: Vec<&'a [u8]>,
}
impl<'a> Provider<'a> {
    pub fn new(case: &'a Case, input: &'a [u8]) -> Self {
        Self { case, input, handles: Vec::new() }
    }
    /// The caller must compile/run a trusted guest using demo_dialect's accessible-buffer contract.
    ///
    /// # Safety
    /// Each guest-requested transport buffer must be initialized and accessible for its complete
    /// read/write extent, without overlapping incompatible live references. No arbitrary lookup
    /// or invalid-pointer rejection is promised. The returned binding retains exclusive state.
    pub unsafe fn bindings(&mut self) -> ClientBindings<'_> {
        let input = self.input;
        let callbacks = ClientCallbacks {
            state_read: unsupported_read,
            state_write: unsupported_write,
            oracle_read: unsupported_oracle,
            host_call_words: words,
            debug_print: debug,
            report_error: report,
        };

        unsafe { ClientBindings::new(callbacks, self, input) }
    }

    /// Dispatch words under the same trusted-buffer contract as bindings.
    /// Raw gmem includes protected holes: only the requested accessible buffer becomes a slice.
    pub unsafe fn dispatch(&mut self, gmem: *mut u8, args: &[u32]) -> u32 {
        enum Request {
            InputSize,
            InputWord { index: u32 },
            Abort,
            QueryResource { key_address: u32, key_length: u32, result_address: u32 },
            CopyResource { handle: u32, destination: u32, length: u32 },
        }

        // Decode the dialect's exact selector/arity into a request before interpreting operands.
        let request = match args {
            [1] => Request::InputSize,
            [2, index] => Request::InputWord { index: *index },
            [3] => Request::Abort,
            [5, address, length, result] => Request::QueryResource {
                key_address: *address,
                key_length: *length,
                result_address: *result,
            },
            [6, handle, destination, length] => Request::CopyResource {
                handle: *handle,
                destination: *destination,
                length: *length,
            },
            _ => return status::INVALID_ARGUMENTS,
        };

        // Serve the request from this invocation's input snapshot and handle table.
        match request {
            Request::InputSize => u32::try_from(self.input.len()).expect("input admitted to RV32"),
            Request::InputWord { index } => {
                let start = usize::try_from(index).ok().and_then(|index| index.checked_mul(4));
                let Some(start) = start else { return 0; };
                let Some(end) = start.checked_add(4) else { return 0; };

                // Indexed reads return complete little-endian words; an incomplete word reads as zero.
                let Some(word) = self.input.get(start..end) else { return 0; };
                u32::from_le_bytes(word.try_into().expect("complete word"))
            }
            Request::Abort => {
                // The declaration terminates through its guest-abort trap after this callback.
                status::SUCCESS
            }
            Request::QueryResource { key_address, key_length, result_address } => {
                let key_bytes = if key_length == 0 {
                    &[]
                } else {
                    unsafe { std::slice::from_raw_parts(gmem.add(key_address as usize), key_length as usize) }
                };
                let Ok(key) = std::str::from_utf8(key_bytes) else { return status::INVALID_PATH; };
                if !crate::cases::valid_key(key) { return status::INVALID_PATH; }

                let Some(bytes) = self.case.resources.get(key) else { return status::NOT_FOUND; };
                let Ok(size) = u32::try_from(bytes.len()) else { return status::ASSET_TOO_LARGE; };

                let next_handle = self.handles.len().checked_add(1)
                    .and_then(|count| u32::try_from(count).ok());
                let Some(handle) = next_handle else { return status::PROTOCOL_ERROR; };
                self.handles.push(bytes);

                // Publish the one-based handle and resource length as two little-endian words.
                let record_words = [handle, size];
                let record_bytes: Vec<_> = record_words.into_iter().flat_map(u32::to_le_bytes).collect();
                unsafe {
                    std::ptr::copy_nonoverlapping(record_bytes.as_ptr(), gmem.add(result_address as usize), 8);
                }

                status::SUCCESS
            }
            Request::CopyResource { handle, destination, length } => {
                if handle == 0 || handle as usize > self.handles.len() {
                    return status::UNKNOWN_HANDLE;
                }
                let bytes = self.handles[handle as usize - 1];

                if length as usize != bytes.len() {
                    return status::LENGTH_MISMATCH;
                }

                // Zero-length copies do not form a pointer into potentially protected gmem.
                if length != 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), gmem.add(destination as usize), bytes.len());
                    }
                }

                status::SUCCESS
            }
        }
    }
}

unsafe extern "C" fn words(context: *mut c_void, gmem: *mut u8, _gmem_len: u64,
    args: *const u32, len: u64) -> HostCallResult
{
    // gmem is a callback-only guest-coordinate origin, not permission to access every byte.
    // Trusted guest buffers uphold dispatch's contract; neither pointer nor derived slices escape.
    let arguments = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(args, usize::try_from(len).expect("word extent")) }
    };
    let provider = unsafe { &mut *context.cast::<Provider<'_>>() };

    let word = unsafe { provider.dispatch(gmem, arguments) };
    HostCallResult { status: HOST_CALL_CONTINUE, word }
}
unsafe extern "C" fn unsupported_read(_: *mut c_void, _: *const StateKey) -> *const c_void {
    panic!("state read is not a demo service");
}

unsafe extern "C" fn unsupported_write(_: *mut c_void, _: *const StateKey, _: *const c_void, _: usize) {
    panic!("state write is not a demo service");
}

unsafe extern "C" fn unsupported_oracle(_: *mut c_void, _: u64) -> *const c_void {
    panic!("oracle read is not a demo service");
}

unsafe extern "C" fn debug(_: *mut c_void, ptr: *const c_void, len: usize) {
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr.cast(), len) }
    };

    eprintln!("{}", String::from_utf8_lossy(bytes));
}

unsafe extern "C" fn report(_: *mut c_void, code: u32, pc: u64) {
    eprintln!("Vehicle error {code} at {pc:#x}");
}
