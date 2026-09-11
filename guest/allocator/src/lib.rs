#![no_std]

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestHeapError {
    AddressOverflow,
    AllocatorRejectedHeap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestHeap {
    start: u32,
    end: u32,
}

impl GuestHeap {
    pub fn new(base: u32, length: u32) -> Result<Self, GuestHeapError> {
        let Some(end) = base.checked_add(length) else {
            return Err(GuestHeapError::AddressOverflow);
        };
        Ok(Self { start: base, end })
    }

    pub const fn start(self) -> u32 {
        self.start
    }

    pub const fn end(self) -> u32 {
        self.end
    }

    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn contains(self, pointer: *const u8, size: usize) -> bool {
        let Ok(pointer) = u32::try_from(pointer as usize) else {
            return false;
        };
        let Ok(size) = u32::try_from(size) else {
            return false;
        };
        pointer >= self.start
            && size <= self.len()
            && pointer <= self.end - size
    }
}

#[derive(Debug, Clone, Copy)]
struct BumpState {
    heap: GuestHeap,
    cursor: u32,
    initialized: bool,
}

impl BumpState {
    const fn uninitialized() -> Self {
        Self {
            heap: GuestHeap { start: 0, end: 0 },
            cursor: 0,
            initialized: false,
        }
    }
}

pub struct BoundedBumpAllocator {
    state: UnsafeCell<BumpState>,
}

// invariant: each fixture Vehicle executes on one thread, so allocator calls cannot overlap.
unsafe impl Sync for BoundedBumpAllocator {}

impl BoundedBumpAllocator {
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(BumpState::uninitialized()),
        }
    }

    /// Replaces the complete allocator state with one ABI-delivered guest heap.
    ///
    /// # Safety
    ///
    /// No allocation returned by an earlier state may remain live.
    pub unsafe fn initialize(&self, base: u32, length: u32) -> Result<(), GuestHeapError> {
        let heap = GuestHeap::new(base, length)?;
        unsafe {
            *self.state.get() = BumpState {
                heap,
                cursor: heap.start(),
                initialized: true,
            };
        }
        Ok(())
    }

    pub fn heap(&self) -> Option<GuestHeap> {
        let state = unsafe { &*self.state.get() };
        state.initialized.then_some(state.heap)
    }

    pub fn cursor(&self) -> Option<u32> {
        let state = unsafe { &*self.state.get() };
        state.initialized.then_some(state.cursor)
    }

    fn allocate(&self, layout: Layout) -> *mut u8 {
        let state = unsafe { &mut *self.state.get() };
        if !state.initialized || layout.size() == 0 {
            return ptr::null_mut();
        }

        let Ok(alignment) = u32::try_from(layout.align()) else {
            return ptr::null_mut();
        };
        let Ok(size) = u32::try_from(layout.size()) else {
            return ptr::null_mut();
        };
        let mask = alignment - 1;
        let Some(aligned_cursor) = state
            .cursor
            .checked_add(mask)
            .map(|value| value & !mask)
        else {
            return ptr::null_mut();
        };
        let Some(end) = aligned_cursor.checked_add(size) else {
            return ptr::null_mut();
        };
        if end > state.heap.end() {
            return ptr::null_mut();
        }

        state.cursor = end;
        aligned_cursor as usize as *mut u8
    }
}

impl Default for BoundedBumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for BoundedBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout)
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}
