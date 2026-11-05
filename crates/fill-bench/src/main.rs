//! Host baseline for 3DS measurements. Times the fill, and with a ROM path also patches.
//!
//! Usage: fill-bench [ROM OUTPUT_DIR [BUDGET_MB]]
//! With a ROM it patches seed 1 only, using the same settings as the upstream CLI. A budget turns on
//! spilling and streamed output, to test the paths that consoles use.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use block_pool::Pool;
use modinfo::Settings;

static STARTED: OnceLock<Instant> = OnceLock::new();

struct CountingAlloc;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static SMALL_CURRENT: AtomicUsize = AtomicUsize::new(0);
static SMALL_PEAK: AtomicUsize = AtomicUsize::new(0);
static POOL: Mutex<Pool> = Mutex::new(Pool::empty());
const LARGE: usize = 64 * 1024;
const POOL_BYTES: usize = 256 * 1024 * 1024;

fn add(size: usize, small: bool) {
    let now = CURRENT.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(now, Relaxed);
    if small {
        let now = SMALL_CURRENT.fetch_add(size, Relaxed) + size;
        SMALL_PEAK.fetch_max(now, Relaxed);
    }
}

// SAFETY: Large blocks come from the pool, and everything else from the system allocator. Each
// block goes back to the allocator that made it, which `contains` decides.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() >= LARGE {
            let ptr = POOL.lock().unwrap().alloc(layout.size(), layout.align());
            if !ptr.is_null() {
                add(layout.size(), false);
                return ptr;
            }
        }
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add(layout.size(), true);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        {
            let mut pool = POOL.lock().unwrap();
            if pool.contains(ptr) {
                pool.free(ptr);
                return;
            }
        }
        SMALL_CURRENT.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        {
            let mut pool = POOL.lock().unwrap();
            if pool.contains(ptr) && pool.grow_in_place(ptr, new_size) {
                let grown = new_size.saturating_sub(layout.size());
                drop(pool);
                add(grown, false);
                CURRENT.fetch_sub(layout.size().saturating_sub(new_size), Relaxed);
                return ptr;
            }
        }
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size)) };
            unsafe { self.dealloc(ptr, layout) };
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

fn peak_mb() -> f32 {
    PEAK.load(Relaxed) as f32 / (1024.0 * 1024.0)
}

fn default_settings() -> Settings {
    let mut settings: Settings = serde_json::from_str("{}").expect("default settings");
    // Matches src/main.rs in the upstream CLI.
    settings.yuganon_requirement = settings.lc_requirement;
    settings
}

fn patch(rom_path: &str, output: &str, budget_mb: Option<usize>) {
    let settings = default_settings();
    let start = Instant::now();
    let seed_info = randomizer::build_seed(1, settings).expect("fill");
    println!("fill: {:.2} s, peak {:.1} MB", start.elapsed().as_secs_f32(), peak_mb());

    let start = Instant::now();
    let game = rom::Rom::load(rom_path).expect("load ROM");
    randomizer::progress::set_hook(|phase, done, total| {
        if done == 0 {
            println!(
                "  {:6.2} s  now {:6.1} MB  peak {:6.1} MB  {phase} ({total})",
                STARTED.get().map_or(0.0, |t| t.elapsed().as_secs_f32()),
                CURRENT.load(Relaxed) as f32 / (1024.0 * 1024.0),
                peak_mb()
            );
        }
    });
    let _ = STARTED.set(Instant::now());
    let progress = |_: f32| {};
    if let Some(budget_mb) = budget_mb {
        rom::spill::configure(rom::spill::Budget {
            dir: Path::new(output).join("spill-tmp"),
            limit: budget_mb * 1024 * 1024,
            usage: || CURRENT.load(Relaxed),
            clear_before_compress: budget_mb < 30,
        });
    }
    randomizer::patch_loaded(&seed_info, game, Path::new(output), Some(&progress)).expect("patch");
    let stats = rom::spill::finish();
    println!("patch: {:.2} s, overall peak {:.1} MB, {stats:?}", start.elapsed().as_secs_f32(), peak_mb());
    println!(
        "pool peak {:.1} MB, small peak {:.1} MB",
        POOL.lock().unwrap().peak() as f32 / (1024.0 * 1024.0),
        SMALL_PEAK.load(Relaxed) as f32 / (1024.0 * 1024.0)
    );
}

fn fill_only() {
    for seed in [1u32, 2, 3] {
        let start = Instant::now();
        let outcome = match randomizer::build_seed(seed, default_settings()) {
            Ok(_) => "ok".to_owned(),
            Err(err) => format!("failed: {:?}", err.into_inner()),
        };
        println!("seed {seed}: {:.2} s, {outcome}", start.elapsed().as_secs_f32());
    }
}

fn main() {
    let memory = unsafe { System.alloc(Layout::from_size_align(POOL_BYTES, 16).unwrap()) };
    // SAFETY: The memory is never freed and used only by the pool.
    unsafe { POOL.lock().unwrap().init(memory, POOL_BYTES) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [rom_path, output] => patch(rom_path, output, None),
        [rom_path, output, budget] => patch(rom_path, output, Some(budget.parse().expect("budget in MB"))),
        _ => fill_only(),
    }
}
