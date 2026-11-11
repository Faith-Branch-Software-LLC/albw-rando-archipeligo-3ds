//! Builds the menu sounds and the game files plugin into the app.
//!
//! The sounds are WAV files in `sounds/`, which is gitignored because their license forbids sharing
//! the raw files. A missing sound is left out, and the menu is silent for it.
//!
//! The plugin (`plugin/` at the repository root) is C, built with devkitARM and libctru, then
//! packed as a Luma3DS `.3gx` file. Set `ALBW_PLUGIN_CFLAGS=-DPLUGIN_TEST` for a build that runs
//! whenever the game starts, for testing on an emulator.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

/// Each sound's file and the name of its constant.
const SOUNDS: &[(&str, &str)] = &[
    ("change_hover.wav", "HOVER"),
    ("choose.wav", "CHOOSE"),
    ("select.wav", "SELECT"),
    ("page_change.wav", "PAGE_CHANGE"),
    ("back.wav", "BACK"),
    ("start_randomizing.wav", "START"),
];

/// The sample rate and 16-bit mono samples of a PCM WAV file. Stereo is mixed down to mono.
fn read_wav(data: &[u8]) -> Result<(u32, Vec<u8>), String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a WAV file".to_owned());
    }
    let mut format = None;
    let mut at = 12;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let size = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &data[at + 8..(at + 8 + size).min(data.len())];
        if id == b"fmt " && body.len() >= 16 {
            let kind = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            let bits = u16::from_le_bytes([body[14], body[15]]);
            if kind != 1 || bits != 16 || !(1..=2).contains(&channels) {
                return Err(format!("needs 16-bit PCM in 1 or 2 channels, not format {kind}, {bits}-bit, {channels} channels"));
            }
            format = Some((channels, rate));
        } else if id == b"data" {
            let (channels, rate) = format.ok_or("the data comes before the format")?;
            let samples = body.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]));
            let mono: Vec<i16> = if channels == 2 {
                let all: Vec<i16> = samples.collect();
                all.chunks_exact(2).map(|pair| ((i32::from(pair[0]) + i32::from(pair[1])) / 2) as i16).collect()
            } else {
                samples.collect()
            };
            return Ok((rate, mono.iter().flat_map(|sample| sample.to_le_bytes()).collect()));
        }
        at += 8 + size + (size & 1);
    }
    Err("no sound data".to_owned())
}

/// Where Luma's plugin loader places plugin code.
const PLUGIN_BASE: u32 = 0x0700_0100;
/// ALBW's title ID, low half. The plugin only loads for this game.
const PLUGIN_TARGET: u32 = 0x000E_C300;

/// The "exe load" function Luma requires in a plugin. Luma runs it over the loaded code, and its
/// result must match the checksum in the header. `u32 load(u32 *start, u32 *end, u32 *args)`
/// returns the sum of the words from start to end. Luma copies it up to the first NOP.
const PLUGIN_LOAD_FUNC: [u32; 8] = [
    0xE3A0_3000, // mov   r3, #0
    0xE150_0001, // cmp   r0, r1         <- loop
    0x3490_C004, // ldrlo r12, [r0], #4
    0x3083_300C, // addlo r3, r3, r12
    0x3AFF_FFFB, // blo   loop
    0xE1A0_0003, // mov   r0, r3
    0xE12F_FF1E, // bx    lr
    0xE320_F000, // nop
];

fn run(command: &mut Command) {
    let status = command.status().unwrap_or_else(|err| panic!("could not run {command:?}: {err}"));
    assert!(status.success(), "{command:?} failed");
}

/// Packs a plugin's memory image as a `.3gx` file, following Luma3DS's `3gx.h`.
fn pack_3gx(code: &[u8], bss: u32) -> Vec<u8> {
    let words = |data: &[u32]| data.iter().flat_map(|word| word.to_le_bytes()).collect::<Vec<u8>>();
    let mut code = code.to_vec();
    code.resize(code.len().next_multiple_of(4), 0);
    let checksum = code.chunks_exact(4).fold(0u32, |sum, word| sum.wrapping_add(u32::from_le_bytes(word.try_into().unwrap())));
    let author = b"Faith Branch Software\0";
    let title = b"ALBW Randomizer game files\0";
    let targets = PLUGIN_TARGET.to_le_bytes();
    let load_func = words(&PLUGIN_LOAD_FUNC);

    const HEADER_LEN: usize = 148;
    let mut file = vec![0u8; HEADER_LEN];
    let mut place = |data: &[u8], align: usize| {
        file.resize(file.len().next_multiple_of(align), 0);
        let at = file.len() as u32;
        file.extend_from_slice(data);
        at
    };
    let author_at = place(author, 1);
    let title_at = place(title, 1);
    let targets_at = place(&targets, 4);
    let load_func_at = place(&load_func, 4);
    let code_at = place(&code, 16);
    let code_len = code.len() as u32;

    // Flags: embedded exe load function, works on console and emulator, events self managed (the
    // plugin never hands control back to the game), swap not needed.
    let flags = 1 | (2 << 4) | (1 << 6) | (1 << 7);
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(&0x3230_3030_2458_4733u64.to_le_bytes()); // "3GX$0002"
    header.extend(words(&[1, 0])); // plugin version, reserved
    header.extend(words(&[author.len() as u32, author_at, title.len() as u32, title_at, 0, 0, 0, 0]));
    header.extend(words(&[flags, checksum, 0, 0, 0, 0, 0, 0, 0, 0]));
    header.extend(words(&[code_at, code_at + code_len, code_at + code_len, code_len, 0, 0, bss, load_func_at, 0, 0]));
    header.extend(words(&[1, targets_at])); // targets
    header.extend(words(&[0, 0, 0])); // symbol table
    assert_eq!(header.len(), HEADER_LEN);
    file[..HEADER_LEN].copy_from_slice(&header);
    file
}

/// Builds the plugin and returns the path of the `.3gx` file.
fn build_plugin(out: &Path) -> PathBuf {
    let source = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../plugin");
    for file in ["main.c", "crt0.s", "plugin.ld"] {
        println!("cargo:rerun-if-changed={}", source.join(file).display());
    }
    println!("cargo:rerun-if-env-changed=ALBW_PLUGIN_CFLAGS");
    let devkitpro = PathBuf::from(env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".to_owned()));
    let bin = env::var("DEVKITARM").map_or_else(|_| devkitpro.join("devkitARM"), PathBuf::from).join("bin");
    let ctru = devkitpro.join("libctru");
    let elf = out.join("albw-files.elf");
    let flat = out.join("albw-files.bin");
    let mut gcc = Command::new(bin.join("arm-none-eabi-gcc"));
    gcc.args(["-march=armv6k", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft", "-O2", "-Wall", "-Wno-format-truncation"])
        .args(["-ffunction-sections", "-fdata-sections", "-D__3DS__"])
        .args(env::var("ALBW_PLUGIN_CFLAGS").unwrap_or_default().split_whitespace())
        .arg(format!("-I{}", ctru.join("include").display()))
        .args(["-nostartfiles", "-specs=sync-dmb.specs", "-Wl,--gc-sections", "-T"])
        .arg(source.join("plugin.ld"))
        .arg(source.join("crt0.s"))
        .arg(source.join("main.c"))
        .arg(format!("-L{}", ctru.join("lib").display()))
        .args(["-lctru", "-lm", "-o"])
        .arg(&elf);
    run(&mut gcc);
    run(Command::new(bin.join("arm-none-eabi-objcopy")).args(["-O", "binary"]).arg(&elf).arg(&flat));
    let symbols = Command::new(bin.join("arm-none-eabi-nm")).arg(&elf).output().expect("nm runs");
    let end = String::from_utf8_lossy(&symbols.stdout)
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let address = parts.next()?;
            (parts.nth(1)? == "__end__").then(|| u32::from_str_radix(address, 16).ok())?
        })
        .expect("the plugin has an __end__ symbol");
    let code = fs::read(&flat).unwrap();
    let bss = end.saturating_sub(PLUGIN_BASE + code.len().next_multiple_of(4) as u32);
    let plugin = out.join("albw-files.3gx");
    fs::write(&plugin, pack_3gx(&code, bss)).unwrap();
    plugin
}

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    let folder = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("sounds");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut code = String::new();
    for (file, name) in SOUNDS {
        let path = folder.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let sound = match fs::read(&path) {
            Ok(data) => match read_wav(&data) {
                Ok(sound) => Some(sound),
                Err(err) => panic!("{}: {err}", path.display()),
            },
            Err(_) => {
                println!("cargo:warning=No {file}, so that menu sound is left out.");
                None
            }
        };
        match sound {
            Some((rate, samples)) => {
                let raw = Path::new(&out).join(format!("{name}.pcm"));
                fs::write(&raw, samples).unwrap();
                code.push_str(&format!("pub const {name}: Option<(u32, &[u8])> = Some(({rate}, include_bytes!({:?})));\n", raw.display().to_string()));
            }
            None => code.push_str(&format!("pub const {name}: Option<(u32, &[u8])> = None;\n")),
        }
    }
    fs::write(Path::new(&out).join("sounds.rs"), code).unwrap();

    let plugin = build_plugin(Path::new(&out));
    fs::write(Path::new(&out).join("plugin.rs"), format!("pub const PLUGIN: &[u8] = include_bytes!({:?});\n", plugin.display().to_string())).unwrap();
}
