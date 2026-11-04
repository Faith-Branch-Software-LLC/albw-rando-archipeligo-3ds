//! Compiles the C shim with devkitARM and links citro2d and citro3d.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=shim/shim.c");
    println!("cargo:rerun-if-env-changed=DEVKITPRO");

    let devkitpro = env::var("DEVKITPRO").expect("DEVKITPRO is not set. Install devkitPro 3ds-dev.");
    let devkitarm = env::var("DEVKITARM").unwrap_or_else(|_| format!("{devkitpro}/devkitARM"));

    cc::Build::new()
        .compiler(format!("{devkitarm}/bin/arm-none-eabi-gcc"))
        .archiver(format!("{devkitarm}/bin/arm-none-eabi-ar"))
        .no_default_flags(true)
        .file("shim/shim.c")
        .include(format!("{devkitpro}/libctru/include"))
        .define("__3DS__", None)
        .flag("-march=armv6k")
        .flag("-mtune=mpcore")
        .flag("-mfloat-abi=hard")
        .flag("-mtp=soft")
        .flag("-mword-relocations")
        .flag("-ffunction-sections")
        .flag("-O2")
        .compile("citro2d_shim");

    println!("cargo:rustc-link-search=native={devkitpro}/libctru/lib");
    println!("cargo:rustc-link-lib=static=citro2d");
    println!("cargo:rustc-link-lib=static=citro3d");
}
