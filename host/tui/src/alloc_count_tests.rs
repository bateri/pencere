//! A counter of the large allocations a thread makes, for the tests that say what a frame asks of the
//! system: every allocation of a test binary goes through it, and only those of at least [`LARGE`]
//! bytes, made by the thread that reads the count, are counted. (A picture's worth of pixels, a mask,
//! the deflate's state: the memory the system makes zero and the drawing then writes over.)
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// How large an allocation is to be counted, in bytes.
pub(crate) const LARGE: usize = 16 * 1024;

thread_local! {
    /// How many large allocations this thread made since the last [`reset`], and how many bytes.
    static LARGE_ONES: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

fn note(size: usize) {
    if size >= LARGE {
        let _ = LARGE_ONES.try_with(|ones| {
            let (count, bytes) = ones.get();
            ones.set((count + 1, bytes + size as u64));
        });
    }
}

struct Counting;

// SAFETY: every call is handed to the system's allocator with the arguments it was given.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            note(new_size);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Starts the count of this thread anew.
pub(crate) fn reset() {
    LARGE_ONES.with(|ones| ones.set((0, 0)));
}

/// How many large allocations this thread made since [`reset`], and how many bytes they were.
pub(crate) fn large() -> (u64, u64) {
    LARGE_ONES.with(Cell::get)
}
