//! [nb:core] Shared binary contract between the client and closed standalone engine.
//! Bootstrap status and signature are stable independently of the revision-dependent table.

use std::ffi::c_void;

pub const ABI_REVISION: u64 = 2;
pub const BOOTSTRAP_SUCCESS: u64 = 0;
pub const BOOTSTRAP_REVISION_MISMATCH: u64 = 1;
pub const BOOTSTRAP_INVALID_OUTPUT: u64 = 2;

/// The output slot must be writable and aligned when non-null. Failure clears it.
/// Success returns a matching immutable table valid until the library is unloaded.
pub type GetApi = unsafe extern "C" fn(u64, *mut *const c_void) -> u64;

/// Operations return zero on success, otherwise an owned error through the error slot.
/// All non-null pointers must be aligned and valid for their complete advertised extent.
/// Handles belong to their creating engine/thread; callers prevent concurrent/reentrant use,
/// use exact handle kinds, and release each once. Null/length checks cannot validate provenance.
/// Output slots are cleared before fallible work. Consume slots are cleared on valid attempts.
/// Paths are Unix bytes without NUL. Input descriptors are borrowed during the call except
/// invocation bindings, retained through invoke/discard. Result views expire on next preparation.
/// Linux callers load this engine with RTLD_GLOBAL and retain it through every handle lifetime.
/// Program admission rejects globally conflicting runtime support owners; distinct engine
/// implementations may not execute Vehicles concurrently in one loader namespace.
/// Layout or semantic changes require advancing ABI_REVISION in both consumers.
#[repr(C)]
pub struct RuntimeApi {
    pub abi_revision: u64,
    pub compile_program: unsafe extern "C" fn(Bytes, Bytes, Bytes, *mut CompilationRecord, *mut *mut ErrorHandle) -> u64,
    pub prepare_program: unsafe extern "C" fn(Bytes, u64, *mut *mut ProgramHandle, *mut *mut ErrorHandle) -> u64,
    pub release_program: unsafe extern "C" fn(*mut *mut ProgramHandle, *mut *mut ErrorHandle) -> u64,
    pub create_session: unsafe extern "C" fn(*mut ProgramHandle, u64, *mut *mut SessionHandle, *mut *mut ErrorHandle) -> u64,
    pub release_session: unsafe extern "C" fn(*mut *mut SessionHandle, *mut *mut ErrorHandle) -> u64,
    pub prepare_invocation: unsafe extern "C" fn(*mut SessionHandle, *const InvocationBindings, *mut *mut PreparedHandle, *mut *mut ErrorHandle) -> u64,
    pub discard_invocation: unsafe extern "C" fn(*mut *mut PreparedHandle, *mut *mut ErrorHandle) -> u64,
    pub invoke: unsafe extern "C" fn(*mut *mut PreparedHandle, *mut *mut ResultsHandle, *mut *mut ErrorHandle) -> u64,
    pub result_view: unsafe extern "C" fn(*mut ResultsHandle, *mut InvocationView, *mut *mut ErrorHandle) -> u64,
    pub release_results: unsafe extern "C" fn(*mut *mut ResultsHandle, *mut *mut ErrorHandle) -> u64,
    pub error_view: unsafe extern "C" fn(*const ErrorHandle, *mut ErrorView) -> u64,
    pub release_error: unsafe extern "C" fn(*mut ErrorHandle),
}

#[repr(C)] pub struct ProgramHandle { _opaque: [u8; 0] }
#[repr(C)] pub struct SessionHandle { _opaque: [u8; 0] }
#[repr(C)] pub struct PreparedHandle { _opaque: [u8; 0] }
#[repr(C)] pub struct ResultsHandle { _opaque: [u8; 0] }
#[repr(C)] pub struct ErrorHandle { _opaque: [u8; 0] }

/// Null is accepted exactly for an empty view. No null slice is ever constructed.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Bytes { pub ptr: *const u8, pub len: u64 }
impl Bytes {
    pub const EMPTY: Self = Self { ptr: std::ptr::null(), len: 0 };
    pub fn borrowed(bytes: &[u8]) -> Self { Self { ptr: bytes.as_ptr(), len: bytes.len() as u64 } }
}

/// Wire-only storage key: four little-endian limbs, least significant limb first.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateKey { pub limbs: [u64; 4] }

pub const HOST_CALL_CONTINUE: u32 = 0;
pub const HOST_CALL_TERMINATE: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostCallResult { pub status: u32, pub word: u32 }

/// Synchronous borrowed transport. Neither gmem nor words may escape or overlap resumed entry.
/// gmem is the raw guest-address-zero origin; its logical length includes inaccessible guards.
/// Each actual buffer must be accessible, initialized, and correctly borrowed for its operation.
/// The trusted demo guest/client supplies that guarantee; origin/length alone does not prove it.
/// Never construct a whole-gmem slice. Zero words permits null args; other arrays are aligned.
pub type HostCallWords = unsafe extern "C" fn(
    context: *mut c_void,
    gmem: *mut u8,
    gmem_len: u64,
    args: *const u32,
    args_len: u64,
) -> HostCallResult;
pub type StateRead = unsafe extern "C" fn(*mut c_void, *const StateKey) -> *const c_void;
pub type StateWrite = unsafe extern "C" fn(*mut c_void, *const StateKey, *const c_void, usize);
pub type OracleRead = unsafe extern "C" fn(*mut c_void, u64) -> *const c_void;
pub type DebugPrint = unsafe extern "C" fn(*mut c_void, *const c_void, usize);
pub type ReportError = unsafe extern "C" fn(*mut c_void, u32, u64);

/// Direct Vehicle-to-client functions, never runtime forwarding adapters.
/// The legacy callbacks retain the Vehicle's size_t lengths on this x64-only ABI.
/// Unsupported services must fail explicitly. Callback panics may not unwind through C.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClientCallbacks {
    pub state_read: StateRead,
    pub state_write: StateWrite,
    pub oracle_read: OracleRead,
    pub host_call_words: HostCallWords,
    pub debug_print: DebugPrint,
    pub report_error: ReportError,
}

/// Context/input and callback code remain live at stable addresses until invoke or discard.
/// The engine neither dereferences the opaque context nor owns/reclaims client resources.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InvocationBindings {
    pub callbacks: ClientCallbacks,
    pub context: *mut c_void,
    pub input: Bytes,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CompilationRecord {
    pub elf_sha256: [u8; 32],
    pub config_sha256: [u8; 32],
    pub vehicle_sha256: [u8; 32],
    pub frontend_ns: u64,
    pub lowering_ns: u64,
    pub compiler_ns: u64,
}

pub const OUTCOME_COMPLETED: u64 = 0;
pub const OUTCOME_CLIENT_TERMINATED: u64 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CounterView { pub name: Bytes, pub value: u64 }
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InvocationView {
    pub outcome: u64,
    pub output_present: u64,
    pub output: Bytes,
    pub counters: *const CounterView,
    pub counters_len: u64,
}
impl InvocationView {
    pub const EMPTY: Self = Self { outcome: 0, output_present: 0, output: Bytes::EMPTY,
        counters: std::ptr::null(), counters_len: 0 };
}

pub const ENTRY_NOT_APPLICABLE: u64 = 0;
pub const ENTRY_NOT_STARTED: u64 = 1;
pub const ENTRY_STARTED: u64 = 2;
pub const ENTRY_UNKNOWN: u64 = 3;
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ErrorView {
    pub code: u64,
    pub stage: Bytes,
    pub message: Bytes,
    pub source_pc_present: u64,
    pub source_pc: u64,
    pub execution_entry: u64,
}
