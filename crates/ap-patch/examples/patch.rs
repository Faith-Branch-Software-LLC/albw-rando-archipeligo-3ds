//! Writes the game files for an Archipelago patch on a computer, to compare with what the
//! apworld's own patcher writes from the same file.
//!
//! Usage: cargo run --release -p ap-patch --example patch -- ROM PATCH OUTPUT_DIR

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [rom_path, patch_path, output] = args.as_slice() else {
        eprintln!("Usage: patch ROM PATCH OUTPUT_DIR");
        std::process::exit(2);
    };
    let patch = ap_patch::read(&std::fs::read(patch_path).expect("read patch")).expect("patch file");
    let seed_info = ap_patch::build(&patch, ap_patch::Looks::default()).expect("rebuild");
    let game = rom::Rom::load(rom_path).expect("load ROM");
    randomizer::patch_loaded(&seed_info, game, Path::new(output), None).expect("patch");
    println!("Patched seed {} for {}, hash {}", patch.seed, patch.player_name, seed_info.hash.text());
}
