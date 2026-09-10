//! [nb:core] Shared binary contract between the client and closed standalone engine.
//! Bootstrap status and signature are stable independently of the revision-dependent table.

use std::ffi::c_void;

pub const ABI_REVISION: u64 = 1;
pub const BOOTSTRAP_SUCCESS: u64 = 0;
pub const BOOTSTRAP_REVISION_MISMATCH: u64 = 1;
pub const BOOTSTRAP_INVALID_OUTPUT: u64 = 2;

/// The output slot must be writable and aligned when non-null. Failure clears it.
/// Success returns a matching immutable table valid until the library is unloaded.
pub type GetApi = unsafe extern "C" fn(u64, *mut *const c_void) -> u64;

/// Revision 1 establishes loading only; no compilation or invocation operations exist yet.
/// Layout or semantic changes require advancing ABI_REVISION in both consumers.
#[repr(C)]
pub struct RuntimeApi {
    pub abi_revision: u64,
}
