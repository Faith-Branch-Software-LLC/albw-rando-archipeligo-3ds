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
/// What the app did to start copying the game files.
pub const COPY_LOG: &str = "sdmc:/3ds/albw-rando/game-copy.log";
const IMAGE: &str = "sdmc:/3ds/albw-rando/game.cci";
/// The game files that the plugin copied from the installed game or game card. The folder holds
/// `romfs/`, `exheader.bin`, and `done.txt`, which the plugin writes last.
const GAME_COPY: &str = "sdmc:/3ds/albw-rando/game";
/// Where the app saves the plugin for Luma to load, as Luma names SD card paths.
const PLUGIN_FILE: &str = "/3ds/albw-rando/albw-files.3gx";
/// This app's title ID when installed as a CIA.
const APP_TITLE_ID: u64 = 0x0004_0000_0FA1_B000;
const EXHEADER: &str = "sdmc:/3ds/albw-rando/exheader.bin";
const BUILD_DIR: &str = "sdmc:/3ds/albw-rando/build";
const SPILL_DIR: &str = "sdmc:/3ds/albw-rando/tmp";
const SEED_LIBRARY: &str = "sdmc:/3ds/albw-rando/seeds";
/// One `NNNN-seed-<seed>.json` per seed made. The number grows, so names sort oldest first.
pub const SPOILER_DIR: &str = "sdmc:/3ds/albw-rando/spoilers";
/// Archipelago patch files from the host, `.apalbw3ds`.
pub const AP_DIR: &str = "sdmc:/3ds/albw-rando/ap";
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
    /// An Archipelago patch file under `AP_DIR` and the look-only choices. When given, the game is
    /// rebuilt from the patch, and `seed`, `settings`, and `preset_name` are not used.
    pub archipelago: Option<(String, ap_patch::Looks)>,
}

/// Lists the Archipelago patch files, newest first, creating the folder so people can find it.
pub fn list_patches() -> Vec<String> {
    let _ = fs::create_dir_all(AP_DIR);
    let Ok(entries) = fs::read_dir(AP_DIR) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            let wanted = Path::new(&name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case(ap_patch::EXTENSION));
            let time = entry.metadata().and_then(|meta| meta.modified()).unwrap_or(std::time::UNIX_EPOCH);
            wanted.then_some((time, name))
        })
        .collect();
    found.sort();
    found.reverse();
    found.into_iter().map(|(_, name)| name).collect()
}

fn read_patch_file(name: &str) -> Result<ap_patch::Patch, String> {
    let bytes = fs::read(Path::new(AP_DIR).join(name)).map_err(|err| format!("Can't read {name}: {err}"))?;
    ap_patch::read(&bytes)
}

/// Reads a patch file for the confirm page.
pub fn read_patch(name: &str) -> Result<ui_core::menu::PatchSummary, String> {
    let patch = read_patch_file(name)?;
    Ok(ui_core::menu::PatchSummary {
        file: name.to_owned(),
        player: patch.player_name.clone(),
        seed: patch.seed,
        version: patch.version.clone(),
        lines: ap_patch::describe(&patch),
    })
}

/// Finds a mod's `romfs` folder. Archives often wrap it in one more folder, such as the title ID.
pub fn romfs_of(folder: &Path) -> Option<PathBuf> {
    let direct = folder.join("romfs");
    if direct.is_dir() {
        return Some(direct);
    }
    fs::read_dir(folder).ok()?.flatten().map(|entry| entry.path().join("romfs")).find(|path| path.is_dir())
}

/// The saved spoiler logs' names without the extension, newest first.
pub fn list_spoilers() -> Vec<String> {
    let Ok(entries) = fs::read_dir(SPOILER_DIR) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".json").map(str::to_owned))
        .collect();
    names.sort();
    names.reverse();
    names
}

/// Reads a saved spoiler log for the menu.
pub fn read_spoiler(name: &str) -> Result<ui_core::spoiler::Spoiler, String> {
    let text = fs::read_to_string(format!("{SPOILER_DIR}/{name}.json")).map_err(|err| format!("Could not read the log: {err}"))?;
    ui_core::spoiler::parse(&text)
}

/// Writes the spoiler log in the randomizer's own format, so other tools can read it too.
fn save_spoiler(seed_info: &randomizer::SeedInfo) -> Result<String, String> {
    fs::create_dir_all(SPOILER_DIR).map_err(|err| format!("Could not create {SPOILER_DIR}: {err}"))?;
    let next = list_spoilers().iter().filter_map(|name| name.split('-').next()?.parse::<u32>().ok()).max().map_or(1, |n| n + 1);
    let path = format!("{SPOILER_DIR}/{next:04}-seed-{}.json", seed_info.seed);
    let text = serde_json::to_string_pretty(seed_info).map_err(|err| format!("Could not write the spoiler log: {err}"))?;
    fs::write(&path, text).map_err(|err| format!("Could not save {path}: {err}"))?;
    Ok(path.trim_start_matches("sdmc:").to_owned())
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
    /// A RomFS folder, either mounted from the game or the plugin's copy, with the exheader.
    Mounted { root: String, exheader: Vec<u8> },
    Image,
}

impl Source {
    fn is_romfs_mount(&self) -> bool {
        matches!(self, Source::Mounted { root, .. } if root.starts_with(MOUNT))
    }
}

/// What the check for the game found.
pub struct GameCheck {
    source: Option<(Location, Source)>,
    /// Where the game is installed or inserted when its files are not on the SD card yet, so the
    /// plugin can copy them.
    pub copy_from: Option<ctru_sys::FS_MediaType>,
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

/// Reads the plugin's copy of the game files, if it finished. `Err` says why it can't be used.
fn read_copy() -> Result<Option<Vec<u8>>, String> {
    if !Path::new(&format!("{GAME_COPY}/done.txt")).exists() {
        return Ok(None);
    }
    let exheader = fs::read(format!("{GAME_COPY}/exheader.bin")).map_err(|err| format!("no exheader.bin: {err}"))?;
    if exheader.len() != 0x400 || exheader[0x200..0x208] != TITLE_ID.to_le_bytes() {
        return Err("exheader.bin is not this game's".to_owned());
    }
    Ok(Some(restore_code_sizes(exheader)?))
}

/// Start addresses and original sizes of the game's code segments, from its exheader.
const TEXT: (u32, u32) = (0x0010_0000, 0x0059_211C);
const RODATA: (u32, u32) = (0x0069_3000, 0x0006_B5F8);
const DATA_ADDRESS: u32 = 0x006F_F000;

/// Puts back the original code sizes in an exheader.
///
/// When a seed is active, Luma hands out the seed's exheader, whose text and rodata sizes the
/// randomizer grew to fit its own code. Patching on top of that runs out of room
/// ("rodata segment overflow"). The randomizer changes only these two sizes, so resetting them
/// gives back the game's own exheader.
fn restore_code_sizes(mut exheader: Vec<u8>) -> Result<Vec<u8>, String> {
    let word = |exheader: &[u8], at: usize| u32::from_le_bytes(exheader[at..at + 4].try_into().unwrap());
    if word(&exheader, 0x10) != TEXT.0 || word(&exheader, 0x20) != RODATA.0 || word(&exheader, 0x30) != DATA_ADDRESS {
        return Err("the exheader's code layout is not this game's".to_owned());
    }
    exheader[0x18..0x1C].copy_from_slice(&TEXT.1.to_le_bytes());
    exheader[0x28..0x2C].copy_from_slice(&RODATA.1.to_le_bytes());
    Ok(exheader)
}

/// Looks for the game files copied to the SD card, then on this console, then on the game card,
/// then in the app's folder.
pub fn check_game() -> GameCheck {
    let name = CString::new(MOUNT).expect("name has no zero byte");
    let mut details = Vec::new();
    match read_copy() {
        Ok(Some(exheader)) => {
            return GameCheck {
                source: Some((Location::Folder, Source::Mounted { root: format!("{GAME_COPY}/romfs/"), exheader })),
                copy_from: None,
                summary: "Game: files copied from your game.".to_owned(),
                details: vec![format!("Using the game files in {GAME_COPY}.")],
            };
        }
        Ok(None) => details.push(format!("No game files in {GAME_COPY}.")),
        Err(err) => details.push(format!("The game files in {GAME_COPY} can't be used: {err}")),
    }
    let mut unreadable = Vec::new();
    let mut copy_from = None;
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
                        source: Some((location, Source::Mounted { root: format!("{MOUNT}:/"), exheader })),
                        copy_from: None,
                        summary: format!("Game: {place}."),
                        details,
                    };
                }
                Err(_) => {
                    details.push("Readable, but there is no exheader.bin.".to_owned());
                    // SAFETY: `name` outlives the call. This undoes the mount above, so the next
                    // place can use the same name.
                    let _ = unsafe { ctru_sys::romfsUnmount(name.as_ptr()) };
                }
            }
        }
        if present {
            unreadable.push(place);
            copy_from.get_or_insert(media);
        }
    }
    // The 3DS only lets a program open its own title's files (0xD9004676, "command not allowed"),
    // even with every file system permission, so an installed game is found but not read. The
    // plugin copies the files from inside the game instead.
    if Path::new(IMAGE).exists() {
        let note = unreadable.first().map_or(String::new(), |place| format!(" It is also {place}."));
        return GameCheck {
            source: Some((Location::Folder, Source::Image)),
            copy_from,
            summary: format!("Game: backup copy, /3ds/albw-rando/game.cci.{note}"),
            details,
        };
    }
    let summary = match unreadable.first() {
        Some(place) => format!("Game: {place}. Choose Copy Game Files once so the randomizer can read it."),
        None => "Game not found. Install A Link Between Worlds (USA) or insert the game card.".to_owned(),
    };
    GameCheck { source: None, copy_from, summary, details }
}

/// Reads archives from the game, to pick images from them. This ends the check.
pub fn game_archives(check: GameCheck, names: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    let archive = |name: &&str| format!("Archive/{name}");
    match &check.source {
        Some((_, Source::Mounted { root, .. })) => {
            let archives = names.iter().map(|name| fs::read(format!("{root}{}", archive(name))).map_err(|err| err.to_string())).collect();
            finish_check(check);
            archives
        }
        Some((_, Source::Image)) => {
            let game = rom::Rom::load(IMAGE).map_err(|err| err.to_string())?;
            Ok(names.iter().map(|name| game.open(&archive(name))).collect())
        }
        None => Err("no game".to_owned()),
    }
}

/// The game's sound archive, which holds the player's voice.
const SOUND_ARCHIVE: &str = "World/Sound/Jack.bcsar";

/// Reads part of a file in the backup copy.
struct RomFile<'a>(&'a rom::Rom, &'static str);

impl ui_core::csar::ReadAt for RomFile<'_> {
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        self.0.read_range(self.1, offset, len).map_err(|err| err.to_string())
    }
}

/// Reads voice clips from the game's own sound archive, a piece at a time.
pub fn game_voice_clips(names: &[&str]) -> Result<(Vec<ui_core::csar::Clip>, Vec<String>), String> {
    let check = check_game();
    let clips = match &check.source {
        Some((_, Source::Mounted { root, .. })) => fs::File::open(format!("{root}{SOUND_ARCHIVE}"))
            .map_err(|err| err.to_string())
            .and_then(|file| ui_core::csar::read_clips(&mut ui_core::csar::Seekable(file), names)),
        Some((_, Source::Image)) => rom::Rom::load(IMAGE)
            .map_err(|err| err.to_string())
            .and_then(|game| ui_core::csar::read_clips(&mut RomFile(&game, SOUND_ARCHIVE), names)),
        None => Err(check.summary.clone()),
    };
    finish_check(check);
    clips
}

/// Reads voice clips from a voice pack's sound archive.
pub fn mod_voice_clips(name: &str, names: &[&str]) -> Result<(Vec<ui_core::csar::Clip>, Vec<String>), String> {
    let romfs = romfs_of(&Path::new(VOICE_DIR).join(name)).ok_or("The voice pack has no romfs folder.")?;
    let file = fs::File::open(romfs.join(SOUND_ARCHIVE)).map_err(|_| "The voice pack has no Jack.bcsar.".to_owned())?;
    ui_core::csar::read_clips(&mut ui_core::csar::Seekable(std::io::BufReader::new(file)), names)
}

/// Lets the next check mount the game again.
pub fn finish_check(check: GameCheck) {
    if check.source.as_ref().is_some_and(|(_, source)| source.is_romfs_mount()) {
        let mount = CString::new(MOUNT).expect("name has no zero byte");
        // SAFETY: `mount` outlives the call. This undoes the mount that `check_game` made.
        let _ = unsafe { ctru_sys::romfsUnmount(mount.as_ptr()) };
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
        Source::Mounted { root, exheader } => rom::Rom::load_mounted(root, &exheader).map_err(|err| err.to_string()),
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
    let Request { mut seed, seed_was_typed, settings, mut preset_name, new_memory, model, voice, archipelago } = request;
    log(&format!("App version {}", env!("CARGO_PKG_VERSION")));
    // SAFETY: libctru sets this once during startup, before main.
    let heap_mb = unsafe { crate::__ctru_heap_size } as f32 / (1024.0 * 1024.0);
    log(&format!("Console memory: {}, heap {heap_mb:.1} MB", if new_memory { "New 3DS" } else { "Old 3DS" }));
    if archipelago.is_none() {
        log(&format!("Seed {seed}, typed by the player: {seed_was_typed}"));
        log(&format!("Preset: {preset_name}"));
        match serde_json::to_string(&settings) {
            Ok(text) => log(&format!("Settings: {text}")),
            Err(err) => log(&format!("Settings could not be written: {err}")),
        }
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
    let seed_info = if let Some((file, looks)) = &archipelago {
        log(&format!("Archipelago patch: {file}, looks {looks:?}"));
        progress::report("Reading the patch", 0, 0);
        let patch = read_patch_file(file)?;
        log(&format!("Patch version {}, player {}, seed {}", patch.version, patch.player_name, patch.seed));
        match serde_json::to_string(&patch.options) {
            Ok(text) => log(&format!("Patch options: {text}")),
            Err(err) => log(&format!("Patch options could not be written: {err}")),
        }
        progress::report("Rebuilding the multiworld layout", 0, 0);
        let info = ap_patch::build(&patch, *looks)?;
        seed = patch.seed;
        preset_name = format!("Archipelago: {}", patch.player_name);
        info
    } else {
        loop {
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
    // The seed is already saved, so a log that cannot be written is only reported.
    match save_spoiler(&seed_info) {
        Ok(spoiler) => log(&format!("Spoiler log saved to {spoiler}")),
        Err(err) => {
            log(&err);
            lines.push("The spoiler log could not be saved. See last-run.log.".to_owned());
        }
    }
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

include!(concat!(env!("OUT_DIR"), "/plugin.rs"));

/// The plugin's request, which Luma passes to it in the plugin header's config words. The plugin
/// (`plugin/main.c`) reads the same layout.
const PLUGIN_REQUEST_MAGIC: u32 = 0x5742_4C41; // "ALBW"
const PLUGIN_REQUEST_VERSION: u32 = 1;

/// Sends one request to a service session. `words` fills the command buffer from the header on.
/// Returns the reply's result code and the command buffer.
///
/// # Safety
///
/// Any buffer descriptors in `words` must point at memory that outlives the call.
unsafe fn request(session: ctru_sys::Handle, words: &[u32]) -> (i32, [u32; 8]) {
    // SAFETY: The command buffer belongs to this thread and holds 64 words.
    unsafe {
        let command = ctru_sys::getThreadCommandBuffer();
        for (at, word) in words.iter().enumerate() {
            *command.add(at) = *word;
        }
        let mut result = ctru_sys::svcSendSyncRequest(session);
        if result >= 0 {
            result = *command.add(1) as i32;
        }
        let mut reply = [0u32; 8];
        for (at, word) in reply.iter_mut().enumerate() {
            *word = *command.add(at);
        }
        (result, reply)
    }
}

/// Sets up Luma to load the game files plugin the next time the game starts, then asks the
/// system to start the game when the app closes. The plugin copies the files to `GAME_COPY` and
/// comes back to the app. Returns what happened, for the log.
pub fn start_copy(media: ctru_sys::FS_MediaType) -> Result<Vec<String>, String> {
    use ctru_sys as sys;
    let mut details = Vec::new();
    fs::create_dir_all(APP_DIR).map_err(|err| format!("Could not make the app's folder: {err}"))?;
    fs::write(format!("sdmc:{PLUGIN_FILE}"), PLUGIN).map_err(|err| format!("Could not save the plugin to the SD card: {err}"))?;

    let port = CString::new("plg:ldr").expect("no zero byte");
    let mut session: sys::Handle = 0;
    // SAFETY: The name outlives the call. The session is closed below.
    let result = unsafe { sys::svcConnectToPort(&mut session, port.as_ptr()) };
    if result < 0 {
        return Err(format!("Luma's plugin loader isn't running (0x{:08X}). Update Luma3DS to version 13 or newer.", result as u32));
    }
    let outcome = (|| {
        // SAFETY: These requests carry no buffers.
        let (result, reply) = unsafe { request(session, &[0x0002_0000]) };
        if result < 0 {
            return Err(format!("Could not ask whether the plugin loader is on (0x{:08X}).", result as u32));
        }
        if reply[2] == 0 {
            // SAFETY: As above.
            let (result, _) = unsafe { request(session, &[0x0003_0040, 1]) };
            if result < 0 {
                return Err(format!("Could not turn on Luma's plugin loader (0x{:08X}).", result as u32));
            }
            details.push("Turned on Luma's plugin loader.".to_owned());
        }
        let mut path = [0u8; 256];
        path[..PLUGIN_FILE.len()].copy_from_slice(PLUGIN_FILE.as_bytes());
        // SAFETY: Reads a value libctru sets at startup.
        let return_to = if unsafe { sys::envIsHomebrew() } { 0 } else { APP_TITLE_ID };
        let mut config = [0u32; 32];
        config[..5].copy_from_slice(&[PLUGIN_REQUEST_MAGIC, PLUGIN_REQUEST_VERSION, return_to as u32, (return_to >> 32) as u32, u32::from(sys::MEDIATYPE_SD)]);
        // No flash, no memory swapping, not kept after one launch, only for this game.
        let words = [0x0004_0084, 1 | (2 << 8), TITLE_ID as u32, (256 << 4) | 0xA, path.as_ptr() as u32, (128 << 4) | 0xA, config.as_ptr() as u32];
        // SAFETY: `path` and `config` outlive the request.
        let (result, _) = unsafe { request(session, &words) };
        if result < 0 {
            return Err(format!("Could not tell Luma to load the plugin (0x{:08X}).", result as u32));
        }
        details.push(format!("Plugin set for the next launch, coming back to {return_to:016X}."));
        Ok(())
    })();
    // SAFETY: The session is open.
    let _ = unsafe { sys::svcCloseHandle(session) };
    outcome?;

    // Start the game without an active seed or mod, so the plugin saves the original header.
    let name = CString::new("Loader").expect("no zero byte");
    let mut loader: sys::Handle = 0;
    // SAFETY: The name outlives the call, and the session is closed below.
    let result = unsafe { sys::srvGetServiceHandle(&mut loader, name.as_ptr()) };
    if result >= 0 {
        // SAFETY: The request carries no buffers. The session is open.
        let (result, _) = unsafe { request(loader, &[0x0100_0000]) };
        // SAFETY: The session is open.
        let _ = unsafe { sys::svcCloseHandle(loader) };
        details.push(format!("Turned off game patching for one launch: 0x{:08X}", result as u32));
    } else {
        details.push(format!("No Luma loader service: 0x{:08X}", result as u32));
    }
    // SAFETY: Takes effect when the app closes.
    unsafe { sys::aptSetChainloader(TITLE_ID, media as u8) };
    Ok(details)
}
