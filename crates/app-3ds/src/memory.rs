//! The memory setup: a pool for large blocks, so small objects cannot split up its free space, and
//! counters for the memory budget and the screen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

use block_pool::Pool;

const MB: f32 = 1024.0 * 1024.0;

fn mb(bytes: usize) -> f32 {
    bytes as f32 / MB
}

/// Bytes in use now, for the memory budget.
pub fn usage() -> usize {
    CURRENT.load(Relaxed)
}

pub fn usage_mb() -> f32 {
    mb(usage())
}

/// Newlib's heap statistics.
#[repr(C)]
#[derive(Default)]
struct MallInfo {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

extern "C" {
    fn mallinfo() -> MallInfo;
}

/// Describes the heap: all memory taken from the system, the free part inside it, and its pieces.
pub fn heap_state() -> String {
    // SAFETY: Takes no arguments and only reads allocator state.
    let info = unsafe { mallinfo() };
    let (used, largest, size) = with_pool(|pool| (pool.used(), pool.largest_free(), pool.size()));
    format!(
        "pool {:.1} of {:.1} MB, largest gap {:.1} MB; small {:.1} MB; arena {:.1} MB, free {:.1} MB in {} pieces",
        mb(used),
        mb(size),
        mb(largest),
        mb(SMALL_CURRENT.load(Relaxed)),
        mb(info.arena),
        mb(info.fordblks),
        info.ordblks
    )
}

struct CountingAlloc;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static REPORTED_OOM: AtomicBool = AtomicBool::new(false);
static REPORTED_SMALL_FULL: AtomicBool = AtomicBool::new(false);
static SMALL_CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Requests this big go to the pool, away from small objects that would split up the free space.
const LARGE: usize = 64 * 1024;

/// The large-block pool with its lock. A value of 1 means unlocked, and allocations can happen
/// before `main`. The kernel-backed lock lets a low-priority holder finish
/// when a higher-priority thread waits.
struct LockedPool {
    pool: UnsafeCell<Pool>,
    lock: UnsafeCell<ctru_sys::LightLock>,
}

// SAFETY: `with_pool` takes the lock around every use of the pool.
unsafe impl Sync for LockedPool {}

static POOL: LockedPool = LockedPool { pool: UnsafeCell::new(Pool::empty()), lock: UnsafeCell::new(1) };

fn with_pool<T>(action: impl FnOnce(&mut Pool) -> T) -> T {
    // SAFETY: The lock was set up in `main` before any other thread started, or is unused before
    // that because only one thread exists. Holding it makes the pool access exclusive.
    unsafe {
        ctru_sys::LightLock_Lock(POOL.lock.get());
        let result = action(&mut *POOL.pool.get());
        ctru_sys::LightLock_Unlock(POOL.lock.get());
        result
    }
}

fn add(size: usize, small: bool) {
    let now = CURRENT.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(now, Relaxed);
    if small {
        SMALL_CURRENT.fetch_add(size, Relaxed);
    }
}

// SAFETY: Large blocks come from the pool and the rest from the system allocator. Either side
// covers for the other when full. Each block returns to its maker, which `contains` decides.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let large = layout.size() >= LARGE;
        if large {
            let ptr = with_pool(|pool| pool.alloc(layout.size(), layout.align()));
            if !ptr.is_null() {
                add(layout.size(), false);
                return ptr;
            }
        }
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add(layout.size(), true);
            return ptr;
        }
        if !large {
            // C library calls cannot use this fallback, so a full small heap can still stop them.
            if !REPORTED_SMALL_FULL.swap(true, Relaxed) {
                crate::job::log("Small-object heap is full. Rust objects now use the pool.");
            }
            let ptr = with_pool(|pool| pool.alloc(layout.size(), layout.align()));
            if !ptr.is_null() {
                add(layout.size(), false);
                return ptr;
            }
        }
        if !REPORTED_OOM.swap(true, Relaxed) {
            // Rust aborts after this returns, so this is the last chance to record why.
            crate::job::log(&format!(
                "OUT OF MEMORY: asked for {:.2} MB with {:.1} MB in use",
                mb(layout.size()),
                mb(CURRENT.load(Relaxed))
            ));
            crate::job::log(&heap_state());
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        let in_pool = with_pool(|pool| {
            let found = pool.contains(ptr);
            if found {
                pool.free(ptr);
            }
            found
        });
        if !in_pool {
            SMALL_CURRENT.fetch_sub(layout.size(), Relaxed);
            unsafe { System.dealloc(ptr, layout) };
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let (in_pool, grown) = with_pool(|pool| {
            let found = pool.contains(ptr);
            (found, found && pool.grow_in_place(ptr, new_size))
        });
        if grown {
            add(new_size.saturating_sub(layout.size()), false);
            CURRENT.fetch_sub(layout.size().saturating_sub(new_size), Relaxed);
            return ptr;
        }
        if !in_pool && new_size < LARGE {
            let new = unsafe { System.realloc(ptr, layout, new_size) };
            if !new.is_null() {
                SMALL_CURRENT.fetch_sub(layout.size(), Relaxed);
                CURRENT.fetch_sub(layout.size(), Relaxed);
                add(new_size, true);
            }
            return new;
        }
        // SAFETY: The caller guarantees that this layout is valid.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            unsafe { core::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size)) };
            unsafe { self.dealloc(ptr, layout) };
        }
        new
    }
}

/// Reserves the pool, leaving the given amount for small objects and thread stacks.
pub fn start_pool(heap: usize, reserve: usize) {
    // SAFETY: Only this thread exists yet.
    unsafe { ctru_sys::LightLock_Init(POOL.lock.get()) };
    let size = heap.saturating_sub(reserve);
    let layout = Layout::from_size_align(size, 16).expect("pool layout");
    // SAFETY: The size is not zero on any console, and the block is never freed.
    let memory = unsafe { System.alloc(layout) };
    if !memory.is_null() {
        // SAFETY: The block is exclusively the pool's for the rest of the run.
        with_pool(|pool| unsafe { pool.init(memory, size) });
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

/// The most memory used since the last reset.
pub fn take_peak_mb() -> f32 {
    let peak = PEAK.swap(CURRENT.load(Relaxed), Relaxed);
    mb(peak)
}
