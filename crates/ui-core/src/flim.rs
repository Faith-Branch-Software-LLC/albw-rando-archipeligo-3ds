//! Reads the header of a game texture (`.bflim`). The pixels are already in the GPU's layout.
//!
//! The game stores every menu texture turned: memory is `height` texels wide (rounded up to a power
//! of two) and `width` tall. The footer's flags say how to turn it back. Flag 8 means transposed:
//! each column of the picture is a row in memory. Flag 4 means memory holds the picture a quarter
//! turn clockwise, `height` wide and `width` tall; it is drawn turned back. The game's rupee
//! (16 by 32) and title logo (256 by 128) only look right that way.

const FOOTER: usize = 0x28;
/// The footer flag for a transposed texture.
const TRANSPOSED: u8 = 8;
/// The footer flag for a texture stored a quarter turn clockwise.
const TURNED: u8 = 4;

#[derive(Debug, PartialEq, Eq)]
pub struct Flim<'a> {
    /// The size of the picture as stored, before any turn.
    pub width: u16,
    pub height: u16,
    /// The texture's size in memory. The GPU needs powers of two, so it can exceed the picture.
    pub texture_width: u16,
    pub texture_height: u16,
    /// The citro3d `GPU_TEXCOLOR` value.
    pub gpu_format: u8,
    /// True if each column of the picture is a row in memory. Otherwise memory holds the picture
    /// a quarter turn clockwise.
    pub transposed: bool,
    pub pixels: &'a [u8],
}

fn word(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

/// The citro3d format and bits per texel for a BFLIM format number, if the GPU can draw it.
fn gpu_format(format: u8) -> Option<(u8, usize)> {
    Some(match format {
        0 => (7, 8),    // L8
        1 => (8, 8),    // A8
        2 => (9, 8),    // LA4
        3 => (5, 16),   // LA8
        4 => (6, 16),   // HILO8
        5 => (3, 16),   // RGB565
        6 => (1, 24),   // RGB8
        7 => (2, 16),   // RGBA5551
        8 => (4, 16),   // RGBA4
        9 => (0, 32),   // RGBA8
        10 => (12, 4),  // ETC1
        11 => (13, 8),  // ETC1A4
        18 => (10, 4),  // L4
        19 => (11, 4),  // A4
        _ => return None,
    })
}

/// Parses a texture. Returns `None` for formats the GPU cannot draw and for damaged files.
pub fn parse(data: &[u8]) -> Option<Flim<'_>> {
    let start = data.len().checked_sub(FOOTER)?;
    let footer = &data[start..];
    if &footer[..4] != b"FLIM" || &footer[0x14..0x18] != b"imag" {
        return None;
    }
    let (gpu_format, bits) = gpu_format(footer[0x22])?;
    let transposed = match footer[0x23] {
        TRANSPOSED => true,
        TURNED => false,
        _ => return None,
    };
    let (stored_width, stored_height) = (word(footer, 0x1C), word(footer, 0x1E));
    if stored_width == 0 || stored_height == 0 {
        return None;
    }
    let texture_width = stored_height.checked_next_power_of_two()?.max(8);
    let texture_height = stored_width.checked_next_power_of_two()?.max(8);
    let size = usize::from(texture_width) * usize::from(texture_height) * bits / 8;
    if size != start {
        return None;
    }
    let (width, height) = if transposed { (stored_width, stored_height) } else { (stored_height, stored_width) };
    Some(Flim { width, height, texture_width, texture_height, gpu_format, transposed, pixels: &data[..start] })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ETC1A4: u8 = 11;
    const RGBA4: u8 = 8;

    fn texture(width: u16, height: u16, format: u8, flags: u8, pixels: usize) -> Vec<u8> {
        let mut data = vec![7; pixels];
        data.extend_from_slice(b"FLIM\xFF\xFE\x14\x00\x00\x00\x00\x03");
        data.extend_from_slice(&[0; 8]);
        data.extend_from_slice(b"imag\x10\x00\x00\x00");
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(&[0x80, 0, format, flags]);
        data.extend_from_slice(&(pixels as u32).to_le_bytes());
        data
    }

    #[test]
    fn reads_a_square_icon() {
        let data = texture(32, 32, ETC1A4, TRANSPOSED, 1024);
        let flim = parse(&data).unwrap();
        assert_eq!((flim.width, flim.height, flim.texture_width, flim.texture_height), (32, 32, 32, 32));
        assert_eq!((flim.gpu_format, flim.transposed, flim.pixels.len()), (13, true, 1024));
    }

    #[test]
    fn rounds_the_texture_up_to_a_power_of_two() {
        let data = texture(40, 40, ETC1A4, TRANSPOSED, 4096);
        let flim = parse(&data).unwrap();
        assert_eq!((flim.width, flim.texture_width), (40, 64));
    }

    #[test]
    fn memory_is_as_wide_as_the_picture_is_tall() {
        // The game's "Start" banner: 256 by 64, stored 64 texels wide and 256 tall.
        let data = texture(256, 64, ETC1A4, TRANSPOSED, 64 * 256);
        let flim = parse(&data).unwrap();
        assert_eq!((flim.width, flim.height, flim.texture_width, flim.texture_height), (256, 64, 64, 256));
    }

    #[test]
    fn turned_textures_swap_width_and_height() {
        // A counter the footer calls 76 by 50, stored turned as 50 wide and 76 tall.
        let data = texture(76, 50, RGBA4, TURNED, 64 * 128 * 2);
        let flim = parse(&data).unwrap();
        assert_eq!((flim.width, flim.height, flim.texture_width, flim.texture_height), (50, 76, 64, 128));
        assert_eq!((flim.gpu_format, flim.transposed), (4, false));
    }

    #[test]
    fn rejects_unknown_formats_flags_and_wrong_sizes() {
        assert!(parse(&texture(32, 32, 12, TRANSPOSED, 1024)).is_none());
        assert!(parse(&texture(32, 32, ETC1A4, 0, 1024)).is_none());
        assert!(parse(&texture(32, 32, ETC1A4, TRANSPOSED, 1000)).is_none());
        assert!(parse(b"short").is_none());
    }
}
