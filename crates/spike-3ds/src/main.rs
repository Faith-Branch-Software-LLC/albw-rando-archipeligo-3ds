//! Milestone 2 go/no-go test. Logs time and peak heap for fill and patch on real hardware, and
//! shows progress on both screens.
//!
//! Each log line is flushed to the SD card right away, so the log still shows the last phase
//! that started if the console runs out of memory or crashes.
//!
//! The work runs on its own thread. The screen thread has higher priority and sleeps between frames,
//! so the kernel lets it interrupt the work on time. A New 3DS gives the work its own CPU core.

mod ui;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

use block_pool::Pool;
use ctru::prelude::*;
use modinfo::Settings;

const DIR: &str = "sdmc:/3ds/albw-rando";
const LOG: &str = "sdmc:/3ds/albw-rando/spike.log";
const EXHEADER: &str = "sdmc:/3ds/albw-rando/exheader.bin";
const OUTPUT: &str = "sdmc:/3ds/albw-rando/spike-out";
/// ModMoon's default mods folder for this game.
const SEED: u32 = 1;
const MODMOON_ROOT: &str = "sdmc:/3ds/ModMoon";
const MODMOON_GAME: &str = "sdmc:/3ds/ModMoon/00040000000EC300";
const SPILL_DIR: &str = "sdmc:/3ds/albw-rando/tmp";
/// Optional file holding a budget in MB, to try other values without a rebuild.
const BUDGET_FILE: &str = "sdmc:/3ds/albw-rando/budget.txt";
const OLD_3DS_BUDGET_MB: usize = 18;
const NEW_3DS_BUDGET_MB: usize = 60;
/// Memory left outside the pool for small objects and thread stacks.
/// The heap measured on a real Old 3DS, used when simulating one.
const OLD_3DS_HEAP: usize = 52 * 1024 * 1024 + 512 * 1024;
const OLD_3DS_RESERVE_MB: usize = 18;
const NEW_3DS_RESERVE_MB: usize = 24;
/// If this file exists, memory is set up as on an Old 3DS, for testing in an emulator.
const SIMULATE_OLD: &str = "sdmc:/3ds/albw-rando/simulate-old.txt";
const IMAGE: &str = "sdmc:/3ds/albw-rando/game.cci";
const TITLE_ID: u64 = 0x0004_0000_000E_C300;
const UPDATE_ID: u64 = 0x0004_000E_000E_C300;
const MOUNT: &str = "albw";
const MB: f32 = 1024.0 * 1024.0;
const WORKER_STACK: usize = 2 * 1024 * 1024;

// libctru's default split leaves an Old 3DS about 24 MB of heap. Graphics here need only a few MB
// of linear memory, so the heap gets the rest. These override weak symbols in libctru.
#[no_mangle]
#[used]
static mut __ctru_linear_heap_size: u32 = 8 * 1024 * 1024;

extern "C" {
    static __ctru_heap_size: u32;
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
fn heap_state() -> String {
    // SAFETY: Takes no arguments and only reads allocator state.
    let info = unsafe { mallinfo() };
    let (used, largest, size) = with_pool(|pool| (pool.used(), pool.largest_free(), pool.size()));
    format!(
        "pool {:.1} of {:.1} MB, largest gap {:.1} MB; small {:.1} MB; arena {:.1} MB, free {:.1} MB in {} pieces",
        megabytes(used),
        megabytes(size),
        megabytes(largest),
        megabytes(SMALL_CURRENT.load(Relaxed)),
        megabytes(info.arena),
        megabytes(info.fordblks),
        info.ordblks
    )
}

struct CountingAlloc;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static REPORTED_OOM: AtomicBool = AtomicBool::new(false);
static SMALL_PEAK: AtomicUsize = AtomicUsize::new(0);
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
        let now = SMALL_CURRENT.fetch_add(size, Relaxed) + size;
        SMALL_PEAK.fetch_max(now, Relaxed);
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
            let ptr = with_pool(|pool| pool.alloc(layout.size(), layout.align()));
            if !ptr.is_null() {
                add(layout.size(), false);
                return ptr;
            }
        }
        if !REPORTED_OOM.swap(true, Relaxed) {
            // Rust aborts after this returns, so this is the last chance to record why.
            log(&format!(
                "OUT OF MEMORY: asked for {:.2} MB with {:.1} MB in use",
                megabytes(layout.size()),
                megabytes(CURRENT.load(Relaxed))
            ));
            log(&heap_state());
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
fn start_pool(heap: usize, reserve: usize) {
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

fn log(line: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(LOG) {
        let _ = writeln!(file, "{line}");
    }
}

fn now_ms() -> u64 {
    // SAFETY: Reads the system clock only.
    unsafe { ctru_sys::osGetTime() }
}

fn megabytes(bytes: usize) -> f32 {
    bytes as f32 / MB
}

// ARMv6k has no 64-bit atomics. Wrapping 32-bit milliseconds cover about 49 days.
static PHASE_START: AtomicU32 = AtomicU32::new(0);
static LAST_PHASE: Mutex<&str> = Mutex::new("");

fn elapsed_s() -> f32 {
    (now_ms() as u32).wrapping_sub(PHASE_START.load(Relaxed)) as f32 / 1000.0
}

/// Updates the screen state, and logs each new phase to the SD card with its start time.
fn on_progress(phase: &'static str, done: u32, total: u32) {
    let mut last = LAST_PHASE.lock().unwrap();
    if *last != phase {
        *last = phase;
        log(&format!("  {:6.1} s  {phase}  [{}]", elapsed_s(), heap_state()));
    }
    drop(last);
    ui::report(phase, done, total);
}

/// Runs one phase and logs its duration and peak heap.
fn measure<T>(name: &str, phase: impl FnOnce() -> T) -> T {
    log(&format!("START {name} (heap {:.1} MB)", megabytes(CURRENT.load(Relaxed))));
    PEAK.store(CURRENT.load(Relaxed), Relaxed);
    PHASE_START.store(now_ms() as u32, Relaxed);
    *LAST_PHASE.lock().unwrap() = "";
    let result = phase();
    log(&format!("END   {name}: {:.1} s, peak {:.1} MB", elapsed_s(), megabytes(PEAK.load(Relaxed))));
    result
}

/// Logs the console's details and returns whether it is a New 3DS.
fn log_system() -> bool {
    let mut is_new = false;
    // SAFETY: The pointer is valid, and the region calls read kernel state only.
    let (size, free) = unsafe {
        ctru_sys::APT_CheckNew3DS(&mut is_new);
        if is_new {
            ctru_sys::osSetSpeedupEnable(true);
        }
        (
            ctru_sys::osGetMemRegionSize(ctru_sys::MEMREGION_APPLICATION),
            ctru_sys::osGetMemRegionFree(ctru_sys::MEMREGION_APPLICATION),
        )
    };
    log(&format!("Model: {}", if is_new { "New 3DS (speedup on)" } else { "Old 3DS" }));
    log(&format!("APPLICATION region: {:.1} MB, free {:.1} MB", size as f32 / MB, free as f32 / MB));
    // SAFETY: libctru sets this once during startup, before main.
    let heap = unsafe { __ctru_heap_size };
    log(&format!("Heap: {:.1} MB", heap as f32 / MB));
    is_new
}

/// Moves the finished patch into the first free ModMoon slot, if ModMoon is set up. Never touches an
/// occupied slot. Returns the slot's display path.
fn save_to_modmoon(seed: u32) -> Option<String> {
    if !Path::new(MODMOON_ROOT).is_dir() {
        log("ModMoon folder not found, so the patch stays in spike-out.");
        return None;
    }
    let slot = (1u32..).find(|n| !Path::new(&format!("{MODMOON_GAME}/Slot_{n}")).exists())?;
    let target = format!("{MODMOON_GAME}/Slot_{slot}");
    let source = format!("{OUTPUT}/{TITLE_ID:016X}");
    let moved = fs::create_dir_all(MODMOON_GAME).and_then(|()| fs::rename(&source, &target));
    if let Err(err) = moved {
        log(&format!("Could not move the patch to {target}: {err}"));
        return None;
    }
    let _ = fs::write(format!("{target}/desc.txt"), format!("ALBW Randomizer seed {seed}\n"));
    log(&format!("Saved to {target}"));
    Some(format!("/3ds/ModMoon/{TITLE_ID:016X}/Slot_{slot}"))
}

fn budget_mb(has_new_memory: bool) -> usize {
    let default = if has_new_memory { NEW_3DS_BUDGET_MB } else { OLD_3DS_BUDGET_MB };
    fs::read_to_string(BUDGET_FILE).ok().and_then(|text| text.trim().parse().ok()).unwrap_or(default)
}

/// Logs whether the system sees the game, so a failed mount can be told apart from a missing game.
fn log_titles() {
    let mut inserted = false;
    // SAFETY: The pointer is valid for the call.
    let result = unsafe { ctru_sys::FSUSER_CardSlotIsInserted(&mut inserted) };
    log(&format!("Card inserted: {inserted} (0x{:08X})", result as u32));

    // SAFETY: Paired with amExit below.
    let result = unsafe { ctru_sys::amInit() };
    if result < 0 {
        log(&format!("amInit failed: 0x{:08X}", result as u32));
        return;
    }
    for (label, media, id) in [
        ("base on SD", ctru_sys::MEDIATYPE_SD, TITLE_ID),
        ("update on SD", ctru_sys::MEDIATYPE_SD, UPDATE_ID),
        ("base on card", ctru_sys::MEDIATYPE_GAME_CARD, TITLE_ID),
    ] {
        let mut id = id;
        // SAFETY: Both pointers refer to one element, matching the count of 1. All-zero is valid
        // for this plain C struct.
        let (result, info) = unsafe {
            let mut info: ctru_sys::AM_TitleInfo = std::mem::zeroed();
            (ctru_sys::AM_GetTitleInfo(media, 1, &mut id, &mut info), info)
        };
        if result < 0 {
            log(&format!("Title {label}: not found (0x{:08X})", result as u32));
        } else {
            log(&format!("Title {label}: {:.1} MB, version {}", info.size as f32 / MB, info.version));
        }
    }
    // SAFETY: amInit succeeded above.
    unsafe { ctru_sys::amExit() };
}

/// Mounts the game from cartridge or SD. Returns the source that worked.
fn mount_game() -> Option<&'static str> {
    let name = CString::new(MOUNT).unwrap();
    for (label, media) in [("cartridge", ctru_sys::MEDIATYPE_GAME_CARD), ("SD", ctru_sys::MEDIATYPE_SD)] {
        // SAFETY: `name` outlives the call, and libctru copies it.
        let result = unsafe { ctru_sys::romfsMountFromTitle(TITLE_ID, media, name.as_ptr()) };
        log(&format!("Mount from {label}: 0x{:08X}", result as u32));
        if result >= 0 {
            return Some(label);
        }
    }
    None
}

/// Loads the game from a mounted title if possible, else from a decrypted image on the SD card.
fn load_game() -> Result<rom::Rom, String> {
    if let Some(source) = mount_game() {
        log(&format!("Game source: {source}"));
        let root = format!("{MOUNT}:/");
        let sample = format!("{root}US/RegionBoot.szs");
        match measure("read RegionBoot.szs", || fs::read(&sample)) {
            Ok(data) => log(&format!("RegionBoot.szs: {} bytes", data.len())),
            Err(err) => log(&format!("Read failed: {err}")),
        }
        let exheader = fs::read(EXHEADER).map_err(|err| format!("no exheader.bin on SD: {err}"))?;
        return rom::Rom::load_mounted(root, &exheader).map_err(|err| format!("{err}"));
    }
    log(&format!("Game source: image at {IMAGE}"));
    rom::Rom::load(IMAGE).map_err(|err| format!("{err}"))
}

fn run_spike() {
    log_system();
    let is_new = has_new_memory();
    randomizer::progress::set_hook(on_progress);

    let settings: Settings = serde_json::from_str("{}").expect("default settings");

    let mut last_seed = None;
    for seed in [SEED] {
        match measure(&format!("fill seed {seed}"), || randomizer::build_seed(seed, settings.clone())) {
            Ok(info) => last_seed = Some(info),
            Err(err) => log(&format!("Fill failed: {:?}", err.into_inner())),
        }
    }
    let Some(seed_info) = last_seed else {
        log("RESULT: every fill failed. Stopping before patch test.");
        ui::fail("Could not place items for this seed.");
        return;
    };

    log_titles();
    let _ = fs::remove_dir_all(OUTPUT);
    let _ = fs::remove_dir_all(SPILL_DIR);
    let budget = budget_mb(is_new);
    log(&format!("Memory budget: {budget} MB"));
    rom::spill::configure(rom::spill::Budget {
        dir: SPILL_DIR.into(),
        limit: budget * 1024 * 1024,
        usage: || CURRENT.load(Relaxed),
        clear_before_compress: !is_new,
    });
    let result = measure("full patch", || {
        // Loading the game comes before the first report from the patcher.
        ui::report("Patching regions", 0, 0);
        let game = load_game()?;
        let progress = |fraction: f32| on_progress("Compressing files", (fraction * 1000.0) as u32, 1000);
        randomizer::patch_loaded(&seed_info, game, Path::new(OUTPUT), Some(&progress))
            .map_err(|err| format!("{:?}", err.into_inner()))
    });
    let stats = rom::spill::finish();
    log(&format!(
        "Spilled {:.1} MB in {} writes, {} reloads",
        megabytes(stats.bytes_written),
        stats.spills,
        stats.reloads
    ));
    log(&format!(
        "Pool peak {:.1} MB, small peak {:.1} MB",
        megabytes(with_pool(|pool| pool.peak())),
        megabytes(SMALL_PEAK.load(Relaxed))
    ));
    match result {
        Ok(()) => {
            log("RESULT: full patch finished.");
            if let Some(path) = save_to_modmoon(SEED) {
                ui::saved_to(path);
            }
        }
        Err(err) => {
            log(&format!("RESULT: patch failed: {err}"));
            ui::fail(&err);
        }
    }
}

fn has_new_memory() -> bool {
    let mut is_new = false;
    // SAFETY: The pointer is valid for the call.
    let _ = unsafe { ctru_sys::APT_CheckNew3DS(&mut is_new) };
    is_new && !Path::new(SIMULATE_OLD).exists()
}

/// Runs on the worker thread.
extern "C" fn worker(_: *mut core::ffi::c_void) {
    let outcome = std::panic::catch_unwind(run_spike);
    if outcome.is_err() {
        log("RESULT: stopped by panic.");
        ui::fail("The patcher stopped unexpectedly.");
    }
    log("Done. Press START to exit.");
    ui::finish();
}

fn main() {
    let gfx = Gfx::new().expect("failed to start GFX");
    let mut hid = Hid::new().expect("failed to start HID");
    let apt = Apt::new().expect("failed to start APT");
    let _ = fs::create_dir_all(DIR);
    let _ = fs::remove_file(LOG);

    std::panic::set_hook(Box::new(|info| {
        log(&format!("PANIC: {info}"));
        ui::fail(&info.to_string());
    }));

    let new_memory = has_new_memory();
    // SAFETY: libctru sets this once during startup, before main.
    let heap = unsafe { __ctru_heap_size } as usize;
    let reserve = if new_memory { NEW_3DS_RESERVE_MB } else { OLD_3DS_RESERVE_MB };
    let heap_for_pool = if new_memory { heap } else { heap.min(OLD_3DS_HEAP) };
    start_pool(heap_for_pool, reserve * 1024 * 1024);

    let mut is_new = false;
    let mut priority = 0;
    // SAFETY: Both pointers are valid, and the pseudo-handle refers to this thread.
    unsafe {
        let _ = ctru_sys::APT_CheckNew3DS(&mut is_new);
        let _ = ctru_sys::svcGetThreadPriority(&mut priority, ctru_sys::CUR_THREAD_HANDLE);
    }
    let mut screen = ui::Screen::new(is_new, unsafe { __ctru_heap_size } as f32 / MB, || {
        CURRENT.load(Relaxed) as f32 / MB
    });

    // A larger priority number runs less urgently, so the screen thread can interrupt the worker.
    let core = if is_new { 2 } else { -2 };
    // SAFETY: The worker takes no argument, and it runs until the process ends.
    let thread = unsafe { ctru_sys::threadCreate(Some(worker), core::ptr::null_mut(), WORKER_STACK, priority + 1, core, true) };
    if thread.is_null() {
        log("RESULT: could not start the worker thread.");
        ui::fail("Could not start the worker thread.");
        ui::finish();
    }

    let mut vblanks = 0u32;
    while apt.main_loop() {
        hid.scan_input();
        if ui::finished() && hid.keys_down().contains(KeyPad::START) {
            break;
        }
        // Drawing takes time from the work on an Old 3DS, so it draws less often there.
        if vblanks % if is_new { 2 } else { 6 } == 0 {
            screen.draw();
        } else {
            gfx.wait_for_vblank();
        }
        vblanks = vblanks.wrapping_add(1);
    }
    drop(screen);
}
