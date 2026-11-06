//! Makes a seed on a worker thread: finds the game, fills, patches, and saves the result.

use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use modinfo::Settings;

use crate::{memory, progress};

pub const APP_DIR: &str = "sdmc:/3ds/albw-rando";
pub const PRESET_DIR: &str = "sdmc:/3ds/albw-rando/presets";
pub const MODEL_DIR: &str = "sdmc:/3ds/albw-rando/models";
pub const VOICE_DIR: &str = "sdmc:/3ds/albw-rando/voices";
pub const SIMULATE_OLD: &str = "sdmc:/3ds/albw-rando/simulate-old.txt";
/// If this file exists, a run starts on launch with default rules. For testing with no buttons.
pub const AUTOSTART: &str = "sdmc:/3ds/albw-rando/autostart.txt";
const LOG: &str = "sdmc:/3ds/albw-rando/last-run.log";
/// If this file exists, the app opens the rules page whose number it holds. For testing.
pub const TEST_PAGE: &str = "sdmc:/3ds/albw-rando/test-page.txt";
/// What the startup check for the game found.
pub const CHECK_LOG: &str = "sdmc:/3ds/albw-rando/game-check.log";
const IMAGE: &str = "sdmc:/3ds/albw-rando/game.cci";
const EXHEADER: &str = "sdmc:/3ds/albw-rando/exheader.bin";
const BUILD_DIR: &str = "sdmc:/3ds/albw-rando/build";
const SPILL_DIR: &str = "sdmc:/3ds/albw-rando/tmp";
const SEED_LIBRARY: &str = "sdmc:/3ds/albw-rando/seeds";
const MODMOON_ROOT: &str = "sdmc:/3ds/ModMoon";
const MODMOON_SETTINGS: &str = "sdmc:/3ds/ModMoon/settings.txt";
const LUMA_TITLES: &str = "sdmc:/luma/titles";
const TITLE_ID: u64 = 0x0004_0000_000E_C300;
const MOUNT: &str = "albw";

const OLD_3DS_BUDGET_MB: usize = 18;
/// A New 3DS has room to spare at this budget, which keeps its small-object heap well clear of full.
const NEW_3DS_BUDGET_MB: usize = 40;

pub struct Request {
    pub seed: u32,
    /// A number the player typed is kept. Otherwise a seed that cannot work is replaced.
    pub seed_was_typed: bool,
    pub settings: Settings,
    pub preset_name: String,
    pub new_memory: bool,
    /// Folder names under `MODEL_DIR` and `VOICE_DIR`.
    pub model: Option<String>,
    pub voice: Option<String>,
}

/// Finds a mod's `romfs` folder. Archives often wrap it in one more folder, such as the title ID.
fn romfs_of(folder: &Path) -> Option<PathBuf> {
    let direct = folder.join("romfs");
    if direct.is_dir() {
        return Some(direct);
    }
    fs::read_dir(folder).ok()?.flatten().map(|entry| entry.path().join("romfs")).find(|path| path.is_dir())
}

/// Lists the usable mods in a folder, creating the folder so people can find where they go.
pub fn list_mods(folder: &str) -> Vec<String> {
    let _ = fs::create_dir_all(folder);
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| romfs_of(&entry.path()).is_some())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Copies one mod's untouched files into the finished patch, and logs what happened. Returns
/// false on any problem.
fn finish_mod(name: &str, source: &Path, output: &Path) -> bool {
    let on_file = |done: u32, total: u32| on_progress(progress::job_phase::ADDING_MODS, done, total);
    let report = rom::mods::copy_missing(source, output, &on_file);
    log(&format!("Mod {name}: copied {}, built into the patch {}", report.copied, report.kept.len()));
    for file in &report.kept {
        log(&format!("Mod {name} built into the patch: {file}"));
    }
    for failure in &report.failures {
        log(&format!("Mod {name} left out {failure}"));
    }
    report.failures.is_empty()
}

static REQUEST: Mutex<Option<Request>> = Mutex::new(None);

pub fn set_request(request: Request) {
    *REQUEST.lock().unwrap() = Some(request);
}

/// Stays open during a run, because opening a file needs memory that a failing run may not have.
static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);

pub fn log(line: &str) {
    let Ok(mut file) = LOG_FILE.try_lock() else {
        return;
    };
    if file.is_none() {
        *file = OpenOptions::new().create(true).append(true).open(LOG).ok();
    }
    if let Some(file) = file.as_mut() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Location {
    Installed,
    Card,
    Folder,
}

enum Source {
    Mounted(Vec<u8>),
    Image,
}

/// What the check for the game found.
pub struct GameCheck {
    source: Option<(Location, Source)>,
    /// A short description for the home page.
    pub summary: String,
    /// Result codes, for the logs.
    pub details: Vec<String>,
}

/// Asks the system whether the game is on a medium. `Err` holds the failing result code.
fn title_present(media: ctru_sys::FS_MediaType) -> Result<bool, i32> {
    // SAFETY: The calls are paired, and each pointer outlives its call. `amExit` balances `amInit`.
    unsafe {
        let result = ctru_sys::amInit();
        if result < 0 {
            return Err(result);
        }
        let mut count = 0u32;
        let mut result = ctru_sys::AM_GetTitleCount(media, &mut count);
        let mut titles = vec![0u64; count as usize];
        let mut read = 0u32;
        if result >= 0 && count > 0 {
            result = ctru_sys::AM_GetTitleList(&mut read, media, count, titles.as_mut_ptr());
        }
        ctru_sys::amExit();
        if result < 0 {
            return Err(result);
        }
        Ok(titles.iter().take(read as usize).any(|&title| title == TITLE_ID))
    }
}

/// Looks for the game on this console, then on the game card, then in the app's folder.
pub fn check_game() -> GameCheck {
    let name = CString::new(MOUNT).expect("name has no zero byte");
    let mut details = Vec::new();
    let mut unreadable = Vec::new();
    for (label, place, location, media) in [
        ("installed", "installed on this 3DS", Location::Installed, ctru_sys::MEDIATYPE_SD),
        ("game card", "the game card", Location::Card, ctru_sys::MEDIATYPE_GAME_CARD),
    ] {
        let present = match title_present(media) {
            Ok(found) => {
                details.push(format!("Looked for the {label} game: {}", if found { "found" } else { "not there" }));
                found
            }
            Err(code) => {
                details.push(format!("Looked for the {label} game: 0x{:08X}", code as u32));
                false
            }
        };
        // SAFETY: `name` outlives the call, and libctru copies it.
        let result = unsafe { ctru_sys::romfsMountFromTitle(TITLE_ID, media, name.as_ptr()) };
        details.push(format!("Read from the {label} game: 0x{:08X}", result as u32));
        if result >= 0 {
            match fs::read(EXHEADER) {
                Ok(exheader) => {
                    return GameCheck {
                        source: Some((location, Source::Mounted(exheader))),
                        summary: format!("Game: {place}."),
                        details,
                    };
                }
                Err(_) => details.push("Readable, but there is no exheader.bin.".to_owned()),
            }
        }
        if present {
            unreadable.push(place);
        }
    }
    let note = unreadable.first().map_or(String::new(), |place| format!(" Found on {place}, but this app can't read it yet."));
    if Path::new(IMAGE).exists() {
        return GameCheck {
            source: Some((Location::Folder, Source::Image)),
            summary: format!("Game: backup copy, /3ds/albw-rando/game.cci.{note}"),
            details,
        };
    }
    let summary = format!("Game not found.{note} Copy your decrypted USA game to /3ds/albw-rando/game.cci on the SD card.");
    GameCheck { source: None, summary, details }
}

/// Reads archives from the game, to pick images from them. This ends the check.
pub fn game_archives(check: GameCheck, names: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    let archive = |name: &&str| format!("Archive/{name}");
    match check.source {
        Some((_, Source::Mounted(_))) => {
            let archives = names.iter().map(|name| fs::read(format!("{MOUNT}:/{}", archive(name))).map_err(|err| err.to_string())).collect();
            let mount = CString::new(MOUNT).expect("name has no zero byte");
            // SAFETY: `mount` outlives the call. This undoes the mount that `check_game` made.
            unsafe { ctru_sys::romfsUnmount(mount.as_ptr()) };
            archives
        }
        Some((_, Source::Image)) => {
            let game = rom::Rom::load(IMAGE).map_err(|err| err.to_string())?;
            Ok(names.iter().map(|name| game.open(&archive(name))).collect())
        }
        None => Err("no game".to_owned()),
    }
}

/// Lets the next check mount the game again.
pub fn finish_check(check: GameCheck) {
    if matches!(check.source, Some((_, Source::Mounted(_)))) {
        let mount = CString::new(MOUNT).expect("name has no zero byte");
        // SAFETY: `mount` outlives the call. This undoes the mount that `check_game` made.
        unsafe { ctru_sys::romfsUnmount(mount.as_ptr()) };
    }
}

/// Finds the game, so a missing game fails right away instead of after the fill.
fn find_game() -> Result<Source, String> {
    let check = check_game();
    for line in &check.details {
        log(line);
    }
    log(&check.summary);
    match check.source {
        Some((_, source)) => Ok(source),
        None => Err(check.summary),
    }
}

fn load_game(source: Source) -> Result<rom::Rom, String> {
    match source {
        Source::Mounted(exheader) => rom::Rom::load_mounted(format!("{MOUNT}:/"), &exheader).map_err(|err| err.to_string()),
        Source::Image => rom::Rom::load(IMAGE).map_err(|err| err.to_string()),
    }
}

/// Picks a ModMoon slot for this title, without using the slot that ModMoon has moved out.
fn modmoon_slot(id: &str) -> (String, String) {
    let settings = fs::read_to_string(MODMOON_SETTINGS).ok();
    let folder = ui_core::modmoon::mods_folder(settings.as_deref().unwrap_or(""));
    let parent = format!("sdmc:{folder}{id}");
    let exists = |slot: u32| Path::new(&format!("{parent}/Slot_{slot}")).exists();
    let active = match &settings {
        Some(settings) => ui_core::modmoon::active_slot(settings, TITLE_ID),
        // With no settings to read, a mod in the Luma folder may have come from the first gap.
        None if Path::new(&format!("{LUMA_TITLES}/{id}")).exists() => Some(ui_core::modmoon::next_free_slot(None, exists)),
        None => None,
    };
    let slot = ui_core::modmoon::next_free_slot(active, exists);
    log(&format!("ModMoon folder {folder}, active slot {active:?}, using slot {slot}"));
    (parent, format!("Slot_{slot}"))
}

/// Moves the finished patch to where the player will use it. Never touches an occupied slot.
/// Returns the path to show.
fn save(seed: u32, hash: &str, preset: &str, mods: &[String]) -> Result<String, String> {
    let id = format!("{TITLE_ID:016X}");
    let source = format!("{BUILD_DIR}/{id}");
    let (parent, name) = if Path::new(MODMOON_ROOT).is_dir() {
        modmoon_slot(&id)
    } else {
        let number = (0u32..).find(|n| !Path::new(&format!("{SEED_LIBRARY}/seed-{seed}-{n}")).exists()).expect("a free name exists");
        (SEED_LIBRARY.to_owned(), format!("seed-{seed}-{number}"))
    };
    let target = format!("{parent}/{name}");
    fs::create_dir_all(&parent).map_err(|err| format!("Could not create {parent}: {err}"))?;
    fs::rename(&source, &target).map_err(|err| format!("Could not move the patch to {target}: {err}"))?;
    let _ = fs::write(format!("{target}/desc.txt"), ui_core::modmoon::slot_description(seed, preset, hash, mods));
    let _ = fs::remove_dir_all(BUILD_DIR);
    Ok(target.trim_start_matches("sdmc:").to_owned())
}

fn random_seed() -> u32 {
    // SAFETY: Reads the system clock only. The mix spreads similar times across the seed range.
    let time = unsafe { ctru_sys::osGetTime() };
    (time as u32).wrapping_mul(2_654_435_761) ^ (time >> 32) as u32
}

fn run(request: Request) -> Result<(), String> {
    let Request { mut seed, seed_was_typed, settings, preset_name, new_memory, model, voice } = request;
    log(&format!("App version {}", env!("CARGO_PKG_VERSION")));
    log(&format!("Console memory: {}", if new_memory { "New 3DS" } else { "Old 3DS" }));
    log(&format!("Seed {seed}, typed by the player: {seed_was_typed}"));
    log(&format!("Preset: {preset_name}"));
    match serde_json::to_string(&settings) {
        Ok(text) => log(&format!("Settings: {text}")),
        Err(err) => log(&format!("Settings could not be written: {err}")),
    }
    log(&format!("Model: {}", model.as_deref().unwrap_or("Original")));
    log(&format!("Voice: {}", voice.as_deref().unwrap_or("Original")));

    // Listed now, so the log names a mod's files even if the patch stops partway.
    for (name, folder) in [(&voice, VOICE_DIR), (&model, MODEL_DIR)] {
        let Some(name) = name else { continue };
        let Some(source) = romfs_of(&Path::new(folder).join(name)) else { continue };
        match rom::mods::list(&source) {
            Ok(files) => {
                log(&format!("Mod {name}: {} files in {}", files.len(), source.display()));
                for (file, size) in files {
                    log(&format!("Mod {name} file: {file} ({size} bytes)"));
                }
            }
            Err(err) => log(&format!("Mod {name}: could not list files: {err}")),
        }
    }

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
        clear_before_compress: true,
    });
    progress::report("Patching regions", 0, 0);
    // A voice pack goes first, so it beats any sound file inside a model mod.
    let selected: Vec<(String, String, PathBuf)> = [(voice, VOICE_DIR, "Voice"), (model, MODEL_DIR, "Model")]
        .into_iter()
        .filter_map(|(name, folder, kind)| {
            let name = name?;
            let found = romfs_of(&Path::new(folder).join(&name));
            if found.is_none() {
                log(&format!("Mod {name}: no romfs folder"));
            }
            Some((format!("{kind}: {name}"), name, found?))
        })
        .collect();
    let result = load_game(source).and_then(|mut game| {
        if !selected.is_empty() {
            game.overlay(selected.iter().map(|(_, _, folder)| folder.clone()).collect());
        }
        let on_fraction = |fraction: f32| on_progress("Compressing files", (fraction * 1000.0) as u32, 1000);
        randomizer::patch_loaded(&seed_info, game, Path::new(BUILD_DIR), Some(&on_fraction))
            .map_err(|err| format!("{:?}", err.into_inner()))
    });
    let stats = rom::spill::finish();
    log(&format!("Patch finished, peak {:.1} MB, {stats:?}", memory::take_peak_mb()));
    result?;

    let hash = seed_info.hash.text().to_owned();
    let output = Path::new(BUILD_DIR).join(format!("{TITLE_ID:016X}")).join("romfs");
    let mut mods_ok = true;
    for (_, name, folder) in &selected {
        mods_ok &= finish_mod(name, folder, &output);
    }
    let mods: Vec<String> = selected.iter().rev().map(|(line, _, _)| line.clone()).collect();

    let path = save(seed, &hash, &preset_name, &mods)?;
    log(&format!("Saved to {path}, hash {hash}"));
    let mut lines = vec![format!("Saved to {path}"), format!("Seed {seed}   Hash {hash}")];
    if !mods_ok {
        lines.push("Some mod files were left out. See last-run.log.".to_owned());
    }
    progress::set_result(lines);
    Ok(())
}

/// The worker thread's entry point.
pub extern "C" fn worker(_: *mut core::ffi::c_void) {
    *LOG_FILE.lock().unwrap() = None;
    let _ = fs::remove_file(LOG);
    log(&format!("{} ms  Started", now_ms() % 100_000_000));
    let request = REQUEST.lock().unwrap().take();
    let outcome = std::panic::catch_unwind(|| match request {
        Some(request) => run(request),
        None => Err("There was nothing to do.".to_owned()),
    });
    match outcome {
        Ok(Ok(())) => log(&format!("{} ms  RESULT: finished", now_ms() % 100_000_000)),
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
