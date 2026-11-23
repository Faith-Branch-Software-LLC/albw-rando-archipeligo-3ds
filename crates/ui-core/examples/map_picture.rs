//! Writes the logic map's picture of a world as raw RGB565 (size by size, top row first), to check
//! how it looks: `cargo run -p ui-core --example map_picture -- <Lyt_FieldLight.bch> <out.raw> <size> [large]`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let picture = ui_core::texture::map_picture(std::fs::read(&args[0]).expect("read"), args[2].parse().expect("size"), args.get(3).is_some()).expect("picture");
    let bytes: Vec<u8> = picture.iter().flat_map(|p| p.to_le_bytes()).collect();
    std::fs::write(&args[1], bytes).expect("write");
}
