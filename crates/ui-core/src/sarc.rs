//! Finds one file by name in an uncompressed game archive (`.arc`, or a `.szs` after Yaz0).

fn word(data: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// Returns the contents of the file called `name`, if the archive has it.
pub fn find<'a>(data: &'a [u8], name: &str) -> Option<&'a [u8]> {
    if data.get(..4)? != b"SARC" || data.get(6..8)? != [0xFF, 0xFE] {
        return None;
    }
    let header = usize::from(u16::from_le_bytes(data.get(4..6)?.try_into().ok()?));
    let data_start = word(data, 0x0C)?;
    if data.get(header..header + 4)? != b"SFAT" {
        return None;
    }
    let count = usize::from(u16::from_le_bytes(data.get(header + 6..header + 8)?.try_into().ok()?));
    let nodes = header + 12;
    // The name table follows the nodes, after its own 8-byte header.
    let names = nodes + count * 16 + 8;
    for index in 0..count {
        let node = nodes + index * 16;
        let offset = (word(data, node + 4)? & 0xFFFF) * 4;
        let rest = data.get(names + offset..)?;
        if rest.starts_with(name.as_bytes()) && rest.get(name.len()) == Some(&0) {
            return data.get(data_start + word(data, node + 8)?..data_start + word(data, node + 12)?);
        }
    }
    None
}

/// Returns the contents of the file called `name` by its name hash, which works for archives that
/// keep no name table, such as `ActorCommon.szs`.
pub fn find_hashed<'a>(data: &'a [u8], name: &str) -> Option<&'a [u8]> {
    if data.get(..4)? != b"SARC" || data.get(6..8)? != [0xFF, 0xFE] {
        return None;
    }
    let header = usize::from(u16::from_le_bytes(data.get(4..6)?.try_into().ok()?));
    let data_start = word(data, 0x0C)?;
    if data.get(header..header + 4)? != b"SFAT" {
        return None;
    }
    let count = usize::from(u16::from_le_bytes(data.get(header + 6..header + 8)?.try_into().ok()?));
    let multiplier = word(data, header + 8)? as u32;
    let hash = name.chars().fold(0u32, |hash, ch| (ch as u32).wrapping_add(hash.wrapping_mul(multiplier)));
    let nodes = header + 12;
    (0..count).find_map(|index| {
        let node = nodes + index * 16;
        (word(data, node)? as u32 == hash).then(|| data.get(data_start + word(data, node + 8)?..data_start + word(data, node + 12)?))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"SARC\x14\x00\xFF\xFE\0\0\0\0");
        let data_start = 0x14 + 12 + 2 * 16 + 8 + 16;
        out.extend_from_slice(&(data_start as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(b"SFAT\x0C\x00\x02\x00\x65\0\0\0");
        for (name_word, start, end) in [(0u32, 0u32, 3u32), (1, 4, 6)] {
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&(0x0100_0000 | name_word).to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&end.to_le_bytes());
        }
        out.extend_from_slice(b"SFNT\x08\x00\0\0");
        out.extend_from_slice(b"a.b\0long.bin\0\0\0\0");
        out.extend_from_slice(b"abc\0de");
        out
    }

    #[test]
    fn finds_files_by_name_hash() {
        let mut data = archive();
        // Stores the hash of "a.b" in the first node.
        let hash = "a.b".chars().fold(0u32, |hash, ch| (ch as u32).wrapping_add(hash.wrapping_mul(0x65)));
        data[0x20..0x24].copy_from_slice(&hash.to_le_bytes());
        assert_eq!(find_hashed(&data, "a.b"), Some(&b"abc"[..]));
        assert_eq!(find_hashed(&data, "other"), None);
    }

    #[test]
    fn finds_files_by_exact_name() {
        let data = archive();
        assert_eq!(find(&data, "a.b"), Some(&b"abc"[..]));
        assert_eq!(find(&data, "long.bin"), Some(&b"de"[..]));
    }

    #[test]
    fn missing_names_and_other_data_give_nothing() {
        let data = archive();
        assert_eq!(find(&data, "a"), None);
        assert_eq!(find(&data, "missing"), None);
        assert_eq!(find(b"not an archive", "a.b"), None);
    }
}
