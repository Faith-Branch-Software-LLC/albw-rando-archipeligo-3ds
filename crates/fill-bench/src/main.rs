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
static IN_REPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
            // Read before the lock: the environment's strings come back through this allocator.
            let want_map = std::env::var_os("POOL_MAP").is_some();
            let (ptr, used, largest, map) = {
                let mut pool = POOL.lock().unwrap();
                let ptr = pool.alloc(layout.size(), layout.align());
                let map = if ptr.is_null() && want_map {
                    let mut map = [0u8; 4096];
                    let mut len = 0;
                    for (offset, size, in_use) in pool.blocks() {
                        if len + 40 < map.len() {
                            use std::io::Write;
                            let mut cursor = std::io::Cursor::new(&mut map[len..]);
                            let _ = write!(cursor, "{}{:.2}@{:.1} ", if in_use { "U" } else { "f" }, size as f32 / 1048576.0, offset as f32 / 1048576.0);
                            len += cursor.position() as usize;
                        }
                    }
                    Some((map, len))
                } else {
                    None
                };
                (ptr, pool.used(), pool.largest_free(), map)
            };
            if let Some((map, len)) = map {
                eprintln!("POOL MAP: {}", String::from_utf8_lossy(&map[..len]));
            }
            if !ptr.is_null() {
                add(layout.size(), false);
                return ptr;
            }
            // With POOL_MB set, the pool is as small as a console's, and a block that does not fit
            // is where the console would run out of memory. The run goes on with system memory.
            if std::env::var_os("POOL_MB").is_some() && !IN_REPORT.swap(true, Relaxed) {
                eprintln!(
                    "POOL FULL: asked {:.2} MB, pool used {:.1} MB, largest gap {:.1} MB\n{}",
                    layout.size() as f32 / 1048576.0,
                    used as f32 / 1048576.0,
                    largest as f32 / 1048576.0,
                    std::backtrace::Backtrace::force_capture()
                );
                IN_REPORT.store(false, Relaxed);
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
    // SETTINGS holds settings as JSON, for trying the rules from a console's run log.
    let text = std::env::var("SETTINGS").unwrap_or_else(|_| "{}".to_owned());
    let mut settings: Settings = serde_json::from_str(&text).expect("settings");
    // Matches src/main.rs in the upstream CLI.
    settings.yuganon_requirement = settings.lc_requirement;
    settings
}

fn patch(rom_path: &str, output: &str, budget_mb: Option<usize>) {
    let settings = default_settings();
    let start = Instant::now();
    let seed = std::env::var("SEED").ok().and_then(|seed| seed.parse().ok()).unwrap_or(1);
    let seed_info = randomizer::build_seed(seed, settings).expect("fill");
    println!("fill: {:.2} s, peak {:.1} MB", start.elapsed().as_secs_f32(), peak_mb());

    let start = Instant::now();
    let mut game = rom::Rom::load(rom_path).expect("load ROM");
    // MODS lists romfs folders to include, separated by colons.
    let mods: Vec<std::path::PathBuf> =
        std::env::var("MODS").map_or_else(|_| Vec::new(), |mods| mods.split(':').map(Into::into).collect());
    if !mods.is_empty() {
        game.overlay(mods.clone());
    }
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
            largest_free: Some(|| POOL.lock().unwrap().largest_free()),
            headroom: std::env::var("HEADROOM_MB").ok().and_then(|mb| mb.parse().ok()).unwrap_or(9) * 1024 * 1024,
        });
    }
    randomizer::patch_loaded(&seed_info, game, Path::new(output), Some(&progress)).expect("patch");
    let stats = rom::spill::finish();
    let romfs = Path::new(output).join("00040000000EC300/romfs");
    for source in &mods {
        let report = rom::mods::copy_missing(source, &romfs, &|_, _| {});
        println!("mod {}: {report:?}", source.display());
    }
    println!("patch: {:.2} s, overall peak {:.1} MB, {stats:?}", start.elapsed().as_secs_f32(), peak_mb());
    println!(
        "pool peak {:.1} MB, small peak {:.1} MB",
        POOL.lock().unwrap().peak() as f32 / (1024.0 * 1024.0),
        SMALL_PEAK.load(Relaxed) as f32 / (1024.0 * 1024.0)
    );
}

/// Prints each location's course, stage and position, for the logic map's table.
fn positions(rom_path: &str) {
    let game = rom::Rom::load(rom_path).expect("load ROM");
    for (subregion, course, name, patch) in randomizer::regions::locations() {
        let place = randomizer::tracker::patch_place(patch, course);
        let position = place.and_then(|(course, stage, unq)| {
            let mut stage_file = game.stage(course, stage - 1).ok()?;
            let obj = stage_file.get_obj(unq)?;
            Some((obj.srt.translate.x, obj.srt.translate.z))
        });
        let (course, stage) = place.map_or(("-".to_owned(), 0), |(course, stage, _)| (format!("{course:?}"), stage));
        let (x, z) = position.unwrap_or((f32::NAN, f32::NAN));
        println!("{}\t{}\t{:?}\t{}\t{}\t{}\t{:.0}\t{:.0}", subregion.name(), subregion.id(), subregion.world(), name, course, stage, x, z);
    }
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
    // POOL_MB sets a console-sized pool, such as 30.7 for an Old 3DS.
    let pool_bytes = std::env::var("POOL_MB")
        .ok()
        .and_then(|mb| mb.parse::<f32>().ok())
        .map_or(POOL_BYTES, |mb| (mb * 1048576.0) as usize & !15);
    let memory = unsafe { System.alloc(Layout::from_size_align(pool_bytes, 16).unwrap()) };
    // SAFETY: The memory is never freed and used only by the pool.
    unsafe { POOL.lock().unwrap().init(memory, pool_bytes) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, rom_path] if command == "positions" => positions(rom_path),
        [command, dir] if command == "mapcases" => {
            let seed = std::env::var("SEED").ok().and_then(|seed| seed.parse().ok()).unwrap_or(1);
            let seed_info = randomizer::build_seed(seed, default_settings()).expect("fill");
            let started = Instant::now();
            let data = randomizer::tracker::map_data(&seed_info);
            println!("map file: {} bytes in {:?}", data.len(), started.elapsed());
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(Path::new(dir).join("albw-map.bin"), data).unwrap();
            for (step, (checked, status)) in randomizer::tracker::test_steps(&seed_info, 8).into_iter().enumerate() {
                let text: String = checked.iter().zip(&status).map(|(c, s)| format!("{} {}\n", *c as u8, s)).collect();
                std::fs::write(Path::new(dir).join(format!("case-{step}.txt")), text).unwrap();
            }
        }
        [command] if command == "spoiler" => {
            // Prints the seed's layout and cracks as JSON, like a spoiler log.
            let seed = std::env::var("SEED").ok().and_then(|seed| seed.parse().ok()).unwrap_or(1);
            let seed_info = randomizer::build_seed(seed, default_settings()).expect("fill");
            println!("{}", serde_json::to_string_pretty(&seed_info).unwrap());
        }
        [command, rom_path, dir, files @ ..] if command == "romfiles" => {
            // Copies files out of the ROM's RomFS, for looking at them.
            let game = rom::Rom::load(rom_path).expect("load ROM");
            std::fs::create_dir_all(dir).unwrap();
            for file in files {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| game.open(file))) {
                    Ok(data) => std::fs::write(Path::new(dir).join(file.replace('/', "_")), data).unwrap(),
                    Err(_) => println!("not found: {file}"),
                }
            }
        }
        [command, path] if command == "byaml" => {
            let data = std::fs::read(path).expect("read");
            let value: serde_json::Value = rom::byaml::from_bytes(&data).expect("parse");
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        }
        [rom_path, output] => patch(rom_path, output, None),
        [rom_path, output, budget] => patch(rom_path, output, Some(budget.parse().expect("budget in MB"))),
        _ => fill_only(),
    }
}
