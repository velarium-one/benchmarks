//! [nb:core] Client-owned demo dialect over preloaded snapshots and invocation-local handles.
//! The runtime receives only direct callbacks/context, never resource ownership or selectors.

use std::ffi::c_void;
use client::{ClientBindings, config::ffi::*};
use crate::cases::Case;

pub struct Provider<'a> {
    case: &'a Case,
    input: &'a [u8],
    handles: Vec<&'a [u8]>,
}
impl<'a> Provider<'a> {
    pub fn new(case: &'a Case, input: &'a [u8]) -> Self { Self { case, input, handles: Vec::new() } }
    /// The caller must compile/run a trusted guest using demo_dialect's accessible-buffer contract.
    ///
    /// # Safety
    /// Each guest-requested transport buffer must be initialized and accessible for its complete
    /// read/write extent, without overlapping incompatible live references. No arbitrary lookup
    /// or invalid-pointer rejection is promised. The returned binding retains exclusive state.
    pub unsafe fn bindings(&mut self) -> ClientBindings<'_> {
        let input = self.input;
        unsafe { ClientBindings::new(ClientCallbacks { state_read: unsupported_read,
            state_write: unsupported_write, oracle_read: unsupported_oracle,
            host_call_words: words, debug_print: debug, report_error: report }, self, input) }
    }

    /// Dispatch words under the same trusted-buffer contract as bindings.
    /// Raw gmem includes protected holes: only the requested accessible buffer becomes a slice.
    pub unsafe fn dispatch(&mut self, gmem: *mut u8, args: &[u32]) -> u32 {
        match args {
            [1] => u32::try_from(self.input.len()).expect("input admitted to RV32"),
            [2, index] => usize::try_from(*index).ok().and_then(|i| i.checked_mul(4))
                .and_then(|start| self.input.get(start..start.checked_add(4)?))
                .map(|word| u32::from_le_bytes(word.try_into().expect("complete word"))).unwrap_or(0),
            [3] => 0, // declaration terminates through its existing guest-abort trap after callback
            [5, address, length, result] => {
                let key = if *length == 0 { &[] } else {
                    unsafe { std::slice::from_raw_parts(gmem.add(*address as usize), *length as usize) }
                };
                let Ok(path) = std::str::from_utf8(key) else { return 1; };
                if !crate::cases::valid_key(path) { return 1; }
                let Some(bytes) = self.case.resources.get(path) else { return 2; };
                let Ok(size) = u32::try_from(bytes.len()) else { return 4; };
                let Some(handle) = self.handles.len().checked_add(1).and_then(|n| u32::try_from(n).ok()) else { return 10; };
                self.handles.push(bytes);
                let record: Vec<_> = [handle, size].into_iter().flat_map(u32::to_le_bytes).collect();
                unsafe { std::ptr::copy_nonoverlapping(record.as_ptr(), gmem.add(*result as usize), 8); }
                0
            }
            [6, handle, destination, length] => {
                if *handle == 0 || *handle as usize > self.handles.len() { return 5; }
                let bytes = self.handles[*handle as usize - 1];
                if *length as usize != bytes.len() { return 6; }
                if *length != 0 {
                    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(),
                        gmem.add(*destination as usize), bytes.len()); }
                }
                0
            }
            _ => 9,
        }
    }
}

unsafe extern "C" fn words(context: *mut c_void, gmem: *mut u8, _gmem_len: u64,
    args: *const u32, len: u64) -> HostCallResult
{
    // gmem is a callback-only guest-coordinate origin, not permission to access every byte.
    // Trusted guest buffers uphold dispatch's contract; neither pointer nor derived slices escape.
    let words = if len == 0 { &[] } else {
        unsafe { std::slice::from_raw_parts(args, usize::try_from(len).expect("word extent")) }
    };
    let provider = unsafe { &mut *context.cast::<Provider<'_>>() };
    HostCallResult { status: HOST_CALL_CONTINUE, word: unsafe { provider.dispatch(gmem, words) } }
}
unsafe extern "C" fn unsupported_read(_: *mut c_void, _: *const StateKey) -> *const c_void { panic!("state read is not a demo service"); }
unsafe extern "C" fn unsupported_write(_: *mut c_void, _: *const StateKey, _: *const c_void, _: usize) { panic!("state write is not a demo service"); }
unsafe extern "C" fn unsupported_oracle(_: *mut c_void, _: u64) -> *const c_void { panic!("oracle read is not a demo service"); }
unsafe extern "C" fn debug(_: *mut c_void, ptr: *const c_void, len: usize) {
    let bytes = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(ptr.cast(), len) } };
    eprintln!("{}", String::from_utf8_lossy(bytes));
}
unsafe extern "C" fn report(_: *mut c_void, code: u32, pc: u64) { eprintln!("Vehicle error {code} at {pc:#x}"); }
