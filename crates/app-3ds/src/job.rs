//! Makes a seed on a worker thread: finds the game, fills, patches, and saves the result.

use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use modinfo::Settings;

use crate::{memory, progress};

pub const APP_DIR: &str = "sdmc:/3ds/albw-rando";
pub const PRESET_DIR: &str = "sdmc:/3ds/albw-rando/presets";
pub const SIMULATE_OLD: &str = "sdmc:/3ds/albw-rando/simulate-old.txt";
/// If this file exists, a run starts on launch with default rules. For testing with no buttons.
pub const AUTOSTART: &str = "sdmc:/3ds/albw-rando/autostart.txt";
const LOG: &str = "sdmc:/3ds/albw-rando/last-run.log";
const IMAGE: &str = "sdmc:/3ds/albw-rando/game.cci";
const EXHEADER: &str = "sdmc:/3ds/albw-rando/exheader.bin";
const BUILD_DIR: &str = "sdmc:/3ds/albw-rando/build";
const SPILL_DIR: &str = "sdmc:/3ds/albw-rando/tmp";
const SEED_LIBRARY: &str = "sdmc:/3ds/albw-rando/seeds";
const MODMOON_ROOT: &str = "sdmc:/3ds/ModMoon";
const TITLE_ID: u64 = 0x0004_0000_000E_C300;
const MOUNT: &str = "albw";

const OLD_3DS_BUDGET_MB: usize = 18;
const NEW_3DS_BUDGET_MB: usize = 60;

pub struct Request {
    pub seed: u32,
    /// A number the player typed is kept. Otherwise a seed that cannot work is replaced.
    pub seed_was_typed: bool,
    pub settings: Settings,
    pub preset_name: String,
    pub new_memory: bool,
}

static REQUEST: Mutex<Option<Request>> = Mutex::new(None);

pub fn set_request(request: Request) {
    *REQUEST.lock().unwrap() = Some(request);
}

pub fn log(line: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(LOG) {
        let _ = writeln!(file, "{line}");
    }
}

fn now_ms() -> u64 {
    // SAFETY: Reads the system clock only.
    unsafe { ctru_sys::osGetTime() }
}

static LAST_PHASE: Mutex<&str> = Mutex::new("");

/// Passed to the randomizer as its progress hook.
pub fn on_progress(phase: &'static str, done: u32, total: u32) {
    let mut last = LAST_PHASE.lock().unwrap();
    if *last != phase {
        *last = phase;
        log(&format!("{} ms  {phase}  [{}]", now_ms() % 100_000_000, memory::heap_state()));
    }
    drop(last);
    progress::report(phase, done, total);
}

enum Source {
    Mounted(Vec<u8>),
    Image,
}

/// Finds the game, so a missing game fails right away instead of after the fill.
fn find_game() -> Result<Source, String> {
    let name = CString::new(MOUNT).expect("name has no zero byte");
    for (label, media) in [("cartridge", ctru_sys::MEDIATYPE_GAME_CARD), ("SD", ctru_sys::MEDIATYPE_SD)] {
        // SAFETY: `name` outlives the call, and libctru copies it.
        let result = unsafe { ctru_sys::romfsMountFromTitle(TITLE_ID, media, name.as_ptr()) };
        log(&format!("Mount from {label}: 0x{:08X}", result as u32));
        if result >= 0 {
            if let Ok(exheader) = fs::read(EXHEADER) {
                return Ok(Source::Mounted(exheader));
            }
            log("Mounted, but there is no exheader.bin.");
        }
    }
    if Path::new(IMAGE).exists() {
        return Ok(Source::Image);
    }
    Err("Game not found. Copy your decrypted USA game to /3ds/albw-rando/game.cci on the SD card.".to_owned())
}

fn load_game(source: Source) -> Result<rom::Rom, String> {
    match source {
        Source::Mounted(exheader) => rom::Rom::load_mounted(format!("{MOUNT}:/"), &exheader).map_err(|err| err.to_string()),
        Source::Image => rom::Rom::load(IMAGE).map_err(|err| err.to_string()),
    }
}

/// Moves the finished patch to where the player will use it. Never touches an occupied slot.
/// Returns the path to show.
fn save(seed: u32, hash: &str, preset: &str) -> Result<String, String> {
    let id = format!("{TITLE_ID:016X}");
    let source = format!("{BUILD_DIR}/{id}");
    let (parent, name) = if Path::new(MODMOON_ROOT).is_dir() {
        let parent = format!("{MODMOON_ROOT}/{id}");
        let slot = (1u32..).find(|n| !Path::new(&format!("{parent}/Slot_{n}")).exists()).expect("a free slot exists");
        (parent, format!("Slot_{slot}"))
    } else {
        let number = (0u32..).find(|n| !Path::new(&format!("{SEED_LIBRARY}/seed-{seed}-{n}")).exists()).expect("a free name exists");
        (SEED_LIBRARY.to_owned(), format!("seed-{seed}-{number}"))
    };
    let target = format!("{parent}/{name}");
    fs::create_dir_all(&parent).map_err(|err| format!("Could not create {parent}: {err}"))?;
    fs::rename(&source, &target).map_err(|err| format!("Could not move the patch to {target}: {err}"))?;
    let _ = fs::write(format!("{target}/desc.txt"), format!("ALBW Randomizer: seed {seed}, preset {preset}, hash {hash}\n"));
    let _ = fs::remove_dir_all(BUILD_DIR);
    Ok(target.trim_start_matches("sdmc:").to_owned())
}

fn random_seed() -> u32 {
    // SAFETY: Reads the system clock only. The mix spreads similar times across the seed range.
    let time = unsafe { ctru_sys::osGetTime() };
    (time as u32).wrapping_mul(2_654_435_761) ^ (time >> 32) as u32
}

fn run(request: Request) -> Result<(), String> {
    let Request { mut seed, seed_was_typed, settings, preset_name, new_memory } = request;
    let source = find_game()?;

    let mut attempts = 0;
    let seed_info = loop {
        attempts += 1;
        log(&format!("Fill: seed {seed}, attempt {attempts}"));
        match randomizer::build_seed(seed, settings.clone()) {
            Ok(info) => break info,
            Err(err) => {
                let reason = format!("{:?}", err.into_inner());
                log(&format!("Fill failed: {reason}"));
                if seed_was_typed {
                    return Err(format!("Seed {seed} cannot work with these rules. Try another number."));
                }
                if attempts >= ui_core::MAX_SEED_ATTEMPTS {
                    return Err(format!("These rules failed {attempts} times. Last problem: {reason}"));
                }
                seed = random_seed();
            }
        }
    };
    log(&format!("Fill finished, peak {:.1} MB", memory::take_peak_mb()));

    let _ = fs::remove_dir_all(BUILD_DIR);
    let _ = fs::remove_dir_all(SPILL_DIR);
    let budget = if new_memory { NEW_3DS_BUDGET_MB } else { OLD_3DS_BUDGET_MB };
    rom::spill::configure(rom::spill::Budget {
        dir: SPILL_DIR.into(),
        limit: budget * 1024 * 1024,
        usage: memory::usage,
        clear_before_compress: !new_memory,
    });
    progress::report("Patching regions", 0, 0);
    let result = load_game(source).and_then(|game| {
        let on_fraction = |fraction: f32| on_progress("Compressing files", (fraction * 1000.0) as u32, 1000);
        randomizer::patch_loaded(&seed_info, game, Path::new(BUILD_DIR), Some(&on_fraction))
            .map_err(|err| format!("{:?}", err.into_inner()))
    });
    let stats = rom::spill::finish();
    log(&format!("Patch finished, peak {:.1} MB, {stats:?}", memory::take_peak_mb()));
    result?;

    let hash = seed_info.hash.text().to_owned();
    let path = save(seed, &hash, &preset_name)?;
    log(&format!("Saved to {path}, hash {hash}"));
    progress::set_result(vec![format!("Saved to {path}"), format!("Seed {seed}   Hash {hash}")]);
    Ok(())
}

/// The worker thread's entry point.
pub extern "C" fn worker(_: *mut core::ffi::c_void) {
    let _ = fs::remove_file(LOG);
    let request = REQUEST.lock().unwrap().take();
    let outcome = std::panic::catch_unwind(|| match request {
        Some(request) => run(request),
        None => Err("There was nothing to do.".to_owned()),
    });
    match outcome {
        Ok(Ok(())) => log("RESULT: finished"),
        Ok(Err(message)) => {
            log(&format!("RESULT: {message}"));
            progress::fail(&message);
        }
        Err(_) => {
            log("RESULT: stopped by panic");
            progress::fail("The patcher stopped unexpectedly.");
        }
    }
    let _ = fs::remove_dir_all(SPILL_DIR);
    progress::finish();
}
