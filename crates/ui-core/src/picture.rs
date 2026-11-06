//! Turns a PNG from the SD card into a GPU texture, so players and artists can change the menu art
//! without rebuilding the app.

/// The largest texture side the GPU accepts.
const MAX_SIDE: u32 = 1024;

/// A texture in the GPU's RGBA8 layout.
#[derive(Debug, PartialEq, Eq)]
pub struct Texture {
    pub width: u16,
    pub height: u16,
    /// The texture's size in memory. The GPU needs powers of two, so it can exceed the picture.
    pub texture_width: u16,
    pub texture_height: u16,
    pub pixels: Vec<u8>,
}

/// Where pixel (x, y) of an 8 by 8 tile goes. The GPU stores tiles in Z-order.
fn tile_index(x: usize, y: usize) -> usize {
    (x & 1) | ((y & 1) << 1) | ((x & 2) << 1) | ((y & 2) << 2) | ((x & 4) << 2) | ((y & 4) << 3)
}

/// Reads a PNG of any common kind. The error says what is wrong in words a player can act on.
pub fn decode(data: &[u8]) -> Result<Texture, String> {
    let mut decoder = png::Decoder::new(data);
    // Palettes, grayscale, and 16-bit channels all become 8-bit color.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|err| format!("not a readable PNG ({err})"))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("{width}x{height} is too big; the most is {MAX_SIDE}x{MAX_SIDE}"));
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).map_err(|err| format!("damaged PNG ({err})"))?;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("palette PNG could not be expanded".to_owned()),
    };
    let (width, height) = (width as usize, height as usize);
    let texture_width = width.next_power_of_two().max(8);
    let texture_height = height.next_power_of_two().max(8);
    let mut pixels = vec![0; texture_width * texture_height * 4];
    for y in 0..height {
        let row = &buffer[y * frame.line_size..];
        for x in 0..width {
            let source = &row[x * channels..x * channels + channels];
            let [r, g, b, a] = match channels {
                4 => [source[0], source[1], source[2], source[3]],
                3 => [source[0], source[1], source[2], 255],
                2 => [source[0], source[0], source[0], source[1]],
                _ => [source[0], source[0], source[0], 255],
            };
            let tile = (y / 8) * (texture_width / 8) + x / 8;
            let at = (tile * 64 + tile_index(x % 8, y % 8)) * 4;
            // The GPU reads each RGBA8 pixel as bytes A, B, G, R.
            pixels[at..at + 4].copy_from_slice(&[a, b, g, r]);
        }
    }
    Ok(Texture {
        width: width as u16,
        height: height as u16,
        texture_width: texture_width as u16,
        texture_height: texture_height as u16,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    fn pixel(texture: &Texture, x: usize, y: usize) -> [u8; 4] {
        let tile = (y / 8) * (usize::from(texture.texture_width) / 8) + x / 8;
        let at = (tile * 64 + tile_index(x % 8, y % 8)) * 4;
        texture.pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn tile_order_covers_every_pixel_once() {
        let mut seen = [false; 64];
        for y in 0..8 {
            for x in 0..8 {
                assert!(!std::mem::replace(&mut seen[tile_index(x, y)], true));
            }
        }
    }

    #[test]
    fn places_pixels_and_pads_to_a_power_of_two() {
        // 10 by 9, with each pixel's red and green set to its position.
        let data: Vec<u8> = (0..9u8).flat_map(|y| (0..10u8).flat_map(move |x| [x, y, 7])).collect();
        let texture = decode(&encode(10, 9, png::ColorType::Rgb, &data)).unwrap();
        assert_eq!((texture.width, texture.height, texture.texture_width, texture.texture_height), (10, 9, 16, 16));
        assert_eq!(pixel(&texture, 0, 0), [255, 7, 0, 0]);
        assert_eq!(pixel(&texture, 9, 8), [255, 7, 8, 9]);
        assert_eq!(pixel(&texture, 3, 5), [255, 7, 5, 3]);
        // Padding stays clear.
        assert_eq!(pixel(&texture, 12, 12), [0; 4]);
    }

    #[test]
    fn keeps_alpha() {
        let texture = decode(&encode(1, 1, png::ColorType::Rgba, &[1, 2, 3, 4])).unwrap();
        assert_eq!(pixel(&texture, 0, 0), [4, 3, 2, 1]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(decode(b"not a png").is_err());
        assert!(decode(&encode(1025, 1, png::ColorType::Grayscale, &[0; 1025])).unwrap_err().contains("too big"));
    }
}
