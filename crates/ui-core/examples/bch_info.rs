//! Lists the textures in a `.bch` file: `cargo run -p ui-core --example bch_info -- <file>`.
fn main() {
    let path = std::env::args().nth(1).expect("a .bch file");
    let textures = ui_core::bch::textures(std::fs::read(path).expect("read")).expect("parse");
    for texture in &textures {
        println!("{} {}x{} format {} ({} bytes)", texture.name, texture.width, texture.height, texture.gpu_format, texture.pixels.len());
    }
}
