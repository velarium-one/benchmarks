#![no_std]

pub const SYSCALL_INPUT_SIZE: usize = 0x01;
pub const SYSCALL_INPUT_READ_U32: usize = 0x02;
pub const SYSCALL_ABORT: usize = 0x03;
pub const SYSCALL_RETURN: usize = 0x04;
pub const SYSCALL_FILE_QUERY: usize = 0x05;
pub const SYSCALL_FILE_READ: usize = 0x06;

/// Status returned by the fixture-only runtime file service.
///
/// Keep these discriminants synchronized with the harness's `FixtureFileStatus` at the private
/// guest/host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FileStatus {
    Success = 0,
    InvalidPath = 1,
    NotFound = 2,
    IoError = 3,
    AssetTooLarge = 4,
    UnknownHandle = 5,
    LengthMismatch = 6,
    InvalidGuestRange = 7,
    ServiceUnavailable = 8,
    InvalidArguments = 9,
    ProtocolError = 10,
}

impl FileStatus {
    fn from_word(word: u32) -> Self {
        match word {
            0 => Self::Success,
            1 => Self::InvalidPath,
            2 => Self::NotFound,
            3 => Self::IoError,
            4 => Self::AssetTooLarge,
            5 => Self::UnknownHandle,
            6 => Self::LengthMismatch,
            7 => Self::InvalidGuestRange,
            8 => Self::ServiceUnavailable,
            9 => Self::InvalidArguments,
            10 => Self::ProtocolError,
            _ => Self::ProtocolError,
        }
    }
}

/// Little-endian query result written into guest memory after a successful file query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct FileQueryResult {
    pub handle: u32,
    pub file_size: u32,
}

#[inline(always)]
pub fn input_size() -> u32 {
    let size: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_INPUT_SIZE,
            lateout("a0") size,
            options(nostack)
        );
    }
    size
}

#[inline(always)]
pub fn input_read_u32(field_index: u32) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_INPUT_READ_U32,
            inlateout("a0") field_index => value,
            options(nostack)
        );
    }
    value
}

/// Query one CWD-relative fixture asset and fix its bytes for the current execution.
#[inline(always)]
pub fn file_query(path: &[u8], result: &mut FileQueryResult) -> FileStatus {
    let status: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_FILE_QUERY,
            inlateout("a0") path.as_ptr() as u32 => status,
            in("a1") path.len() as u32,
            in("a2") result as *mut FileQueryResult as u32,
            options(nostack)
        );
    }
    FileStatus::from_word(status)
}

/// Copy one complete queried snapshot into an exactly sized guest-owned slice.
#[inline(always)]
pub fn file_read(handle: u32, destination: &mut [u8]) -> FileStatus {
    let status: u32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_FILE_READ,
            inlateout("a0") handle => status,
            in("a1") destination.as_mut_ptr() as u32,
            in("a2") destination.len() as u32,
            options(nostack)
        );
    }
    FileStatus::from_word(status)
}

#[inline(always)]
pub fn abort() -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_ABORT,
            options(noreturn)
        );
    }
}

#[inline(always)]
pub fn return_slice(ptr: u32, len: u32) -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYSCALL_RETURN,
            in("a0") ptr,
            in("a1") len,
            options(noreturn)
        );
    }
}
