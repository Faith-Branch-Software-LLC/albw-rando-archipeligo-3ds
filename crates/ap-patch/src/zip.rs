//! Reads one file out of a zip archive. Archipelago patch files are zips that Python's `zipfile`
//! writes, stored or deflated, so this handles only those two methods.

const END_OF_DIRECTORY: u32 = 0x0605_4b50;
const DIRECTORY_ENTRY: u32 = 0x0201_4b50;
const LOCAL_HEADER: u32 = 0x0403_4b50;

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The contents of the file called `name`, or `None` if the archive lacks it.
pub fn read(data: &[u8], name: &str) -> Result<Option<Vec<u8>>, String> {
    let broken = || "The patch file is damaged or is not a zip file.".to_owned();
    // The end record sits in the last 22 bytes, plus a comment of up to 64 KiB.
    let lowest = data.len().saturating_sub(22 + 0xffff);
    let end = (lowest..=data.len().saturating_sub(22))
        .rev()
        .find(|&at| u32_at(data, at) == Some(END_OF_DIRECTORY))
        .ok_or_else(broken)?;
    let count = u16_at(data, end + 10).ok_or_else(broken)? as usize;
    let mut at = u32_at(data, end + 16).ok_or_else(broken)? as usize;
    for _ in 0..count {
        if u32_at(data, at) != Some(DIRECTORY_ENTRY) {
            return Err(broken());
        }
        let method = u16_at(data, at + 10).ok_or_else(broken)?;
        let packed = u32_at(data, at + 20).ok_or_else(broken)? as usize;
        let name_len = u16_at(data, at + 28).ok_or_else(broken)? as usize;
        let extra_len = u16_at(data, at + 30).ok_or_else(broken)? as usize;
        let comment_len = u16_at(data, at + 32).ok_or_else(broken)? as usize;
        let local = u32_at(data, at + 42).ok_or_else(broken)? as usize;
        let entry_name = data.get(at + 46..at + 46 + name_len).ok_or_else(broken)?;
        at += 46 + name_len + extra_len + comment_len;
        if entry_name != name.as_bytes() {
            continue;
        }
        if u32_at(data, local) != Some(LOCAL_HEADER) {
            return Err(broken());
        }
        let start = local + 30 + u16_at(data, local + 26).ok_or_else(broken)? as usize
            + u16_at(data, local + 28).ok_or_else(broken)? as usize;
        let body = data.get(start..start + packed).ok_or_else(broken)?;
        return match method {
            0 => Ok(Some(body.to_vec())),
            8 => miniz_oxide::inflate::decompress_to_vec(body).map(Some).map_err(|_| broken()),
            other => Err(format!("The patch file uses zip compression {other}, which this app cannot read.")),
        };
    }
    Ok(None)
}
