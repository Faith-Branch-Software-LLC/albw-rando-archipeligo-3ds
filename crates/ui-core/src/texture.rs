//! Turns a texture in the 3DS GPU's layout into plain pixels. The GPU keeps textures in 8 by 8
//! tiles, with the texels of each tile in Z order, and ETC1 textures as 4 by 4 blocks, four to a
//! tile. Used for the game's map pictures, which the QoL plugin draws behind its logic map.

use crate::bch::Texture;

/// The ETC1 brightness changes, by table and by the texel's two bits.
const ETC1_TABLES: [[i32; 2]; 8] = [[2, 8], [5, 17], [9, 29], [13, 42], [18, 60], [24, 80], [33, 106], [47, 183]];

/// Decodes `texture` to RGBA rows, top row first. None for formats this does not read.
pub fn decode(texture: &Texture) -> Option<Vec<[u8; 4]>> {
    let (width, height) = (usize::from(texture.width), usize::from(texture.height));
    let mut out = vec![[0u8; 4]; width * height];
    let data = &texture.pixels;
    match texture.gpu_format {
        12 | 13 => {
            let alpha = texture.gpu_format == 13;
            let block_size = if alpha { 16 } else { 8 };
            let mut at = 0;
            for tile_y in (0..height).step_by(8) {
                for tile_x in (0..width).step_by(8) {
                    for (bx, by) in [(0, 0), (4, 0), (0, 4), (4, 4)] {
                        let block = data.get(at..at + block_size)?;
                        at += block_size;
                        let (alphas, color) = if alpha { (u64::from_le_bytes(block[..8].try_into().ok()?), &block[8..]) } else { (u64::MAX, block) };
                        let texels = etc1_block(u64::from_le_bytes(color.try_into().ok()?));
                        for (i, texel) in texels.iter().enumerate() {
                            let (x, y) = (i / 4, i % 4);
                            let a = if alpha { ((alphas >> (4 * i)) & 0xF) as u8 * 17 } else { 255 };
                            let (px, py) = (tile_x + bx + x, tile_y + by + y);
                            if px < width && py < height {
                                out[py * width + px] = [texel[0], texel[1], texel[2], a];
                            }
                        }
                    }
                }
            }
        }
        format => {
            let bits = match format {
                0 => 32,
                1 => 24,
                2..=5 => 16,
                _ => return None,
            };
            let bytes = bits / 8;
            for tile_y in (0..height).step_by(8) {
                for tile_x in (0..width).step_by(8) {
                    let tile = (tile_y / 8 * (width / 8) + tile_x / 8) * 64;
                    for i in 0..64 {
                        // Z order: x and y bits interleaved.
                        let x = (i & 1) | (i >> 1 & 2) | (i >> 2 & 4);
                        let y = (i >> 1 & 1) | (i >> 2 & 2) | (i >> 3 & 4);
                        let at = (tile + i) * bytes;
                        let p = data.get(at..at + bytes)?;
                        let pixel = match format {
                            0 => [p[3], p[2], p[1], p[0]],
                            1 => [p[2], p[1], p[0], 255],
                            2 => {
                                let v = u16::from_le_bytes([p[0], p[1]]);
                                [five(v >> 11), five(v >> 6), five(v >> 1), if v & 1 != 0 { 255 } else { 0 }]
                            }
                            3 => {
                                let v = u16::from_le_bytes([p[0], p[1]]);
                                [five(v >> 11), ((v >> 5 & 0x3F) << 2 | (v >> 9 & 3)) as u8, five(v), 255]
                            }
                            4 => {
                                let v = u16::from_le_bytes([p[0], p[1]]);
                                [(v >> 12) as u8 * 17, (v >> 8 & 0xF) as u8 * 17, (v >> 4 & 0xF) as u8 * 17, (v & 0xF) as u8 * 17]
                            }
                            _ => [p[1], p[1], p[1], p[0]],
                        };
                        out[(tile_y + y) * width + tile_x + x] = pixel;
                    }
                }
            }
        }
    }
    Some(out)
}

fn five(value: u16) -> u8 {
    let v = (value & 0x1F) as u8;
    v << 3 | v >> 2
}

/// The 16 texels of one ETC1 block, in the order x * 4 + y.
fn etc1_block(block: u64) -> [[u8; 3]; 16] {
    let high = (block >> 32) as u32;
    let low = block as u32;
    let flip = high & 1 != 0;
    let diff = high & 2 != 0;
    let tables = [(high >> 5 & 7) as usize, (high >> 2 & 7) as usize];
    let channel = |shift: u32| -> [i32; 2] {
        if diff {
            let base = (high >> (shift + 3) & 0x1F) as i32;
            let delta = ((high >> shift & 7) as i32) << 29 >> 29;
            let second = (base + delta).clamp(0, 31);
            [base << 3 | base >> 2, second << 3 | second >> 2]
        } else {
            let first = (high >> (shift + 4) & 0xF) as i32;
            let second = (high >> shift & 0xF) as i32;
            [first * 17, second * 17]
        }
    };
    let (r, g, b) = (channel(24), channel(16), channel(8));
    let mut texels = [[0u8; 3]; 16];
    for (i, texel) in texels.iter_mut().enumerate() {
        let (x, y) = (i / 4, i % 4);
        let half = if flip { usize::from(y >= 2) } else { usize::from(x >= 2) };
        let index = ((low >> (i + 16) & 1) << 1 | (low >> i & 1)) as usize;
        let step = ETC1_TABLES[tables[half]][index & 1];
        let change = if index & 2 != 0 { -step } else { step };
        *texel = [r[half], g[half], b[half]].map(|c| (c + change).clamp(0, 255) as u8);
    }
    texels
}

/// The logic map's picture of a world: the game's field map from `Lyt_FieldLight.bch` or
/// `Lyt_FieldDark.bch` (the small one, or the large one for the zoomed view), scaled to `size` by
/// `size` and darkened so the plugin's dots stand out. RGB565, top row first.
pub fn map_picture(bch: Vec<u8>, size: usize, large: bool) -> Result<Vec<u16>, String> {
    let textures = crate::bch::textures(bch)?;
    let prefix = if large { "FieldL" } else { "FieldS" };
    let texture = textures.iter().find(|texture| texture.name.starts_with(prefix)).ok_or("no field map")?;
    let pixels = decode(texture).ok_or("the map's format is not one this app reads")?;
    let (width, height) = (usize::from(texture.width), usize::from(texture.height));
    // Map units cover the whole picture, frame included. Up to 0.3.13 this cut the picture to
    // 41/256..216/256, which hid the edges and put every check in the wrong spot.
    let (left, top, across, down) = (0, 0, width, height);
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let [r, g, b, _] = pixels[(top + y * down / size) * width + left + x * across / size];
            let dim = |c: u8| u16::from(c) * 3 / 5;
            out.push((dim(r) >> 3) << 11 | (dim(g) >> 2) << 5 | dim(b) >> 3);
        }
    }
    Ok(out)
}
