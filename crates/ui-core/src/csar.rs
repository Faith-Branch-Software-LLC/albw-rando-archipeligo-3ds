//! Pulls short voice clips out of the game's sound archive (`World/Sound/Jack.bcsar`), so the menu
//! can play a sample of a voice pack.
//!
//! The archive is about 48 MB, so it is read in pieces through [`ReadAt`]. A clip is found by name:
//! the sound's entry points to a wave sound file (CWSD), whose first note names a wave archive
//! (CWAR) and a wave in it (CWAV). Player voices are mono DSP-ADPCM, which is decoded here to
//! 16-bit samples.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

type Result<T> = std::result::Result<T, String>;

/// The player voice sounds to sample, in the order the menu plays them.
pub const VOICE_CLIPS: [&str; 8] = [
    "SE_PV_SW_SWING_360",
    "SE_PV_DAMAGE",
    "SE_PV_LIFT_L",
    "SE_PV_THROW_L",
    "SE_PV_FALL",
    "SE_PV_PUSH_PULL",
    "SE_PV_SURPRISE_01",
    "SE_PV_CALL",
];

/// Reads bytes at an offset. Reads past the end return fewer bytes.
pub trait ReadAt {
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>>;
}

/// Reads from any seekable file.
pub struct Seekable<R>(pub R);

impl<R: Read + Seek> ReadAt for Seekable<R> {
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.0.seek(SeekFrom::Start(offset)).map_err(|err| err.to_string())?;
        let mut data = Vec::with_capacity(len);
        (&mut self.0).take(len as u64).read_to_end(&mut data).map_err(|err| err.to_string())?;
        Ok(data)
    }
}

impl ReadAt for &[u8] {
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let start = (offset as usize).min(self.len());
        Ok(self[start..(start + len).min(self.len())].to_vec())
    }
}

/// A decoded mono clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

/// Little-endian reads that fail instead of panicking on short data.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&self, at: usize) -> Result<u8> {
        self.0.get(at).copied().ok_or_else(|| format!("Sound data ends early at {at:#x}."))
    }

    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes([self.u8(at)?, self.u8(at + 1)?]))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes([self.u8(at)?, self.u8(at + 1)?, self.u8(at + 2)?, self.u8(at + 3)?]))
    }

    fn i16(&self, at: usize) -> Result<i16> {
        Ok(self.u16(at)? as i16)
    }

    fn at(&self, at: usize) -> Result<usize> {
        Ok(self.u32(at)? as usize)
    }

    fn slice(&self, at: usize, len: usize) -> Result<&[u8]> {
        self.0.get(at..at + len).ok_or_else(|| format!("Sound data ends early at {at:#x}."))
    }
}

/// The offset of a block's body. Each Nintendo sound file lists its blocks after a 0x14-byte
/// header as (type, offset, size), and each block starts with an 8-byte magic and size.
fn block(file: &Bytes, index: usize) -> Result<usize> {
    Ok(file.at(0x18 + index * 12)? + 8)
}

/// The archive's tables, read once for any number of clips.
struct Archive {
    /// The INFO block body.
    info: Vec<u8>,
    /// Where the FILE block's body starts in the archive.
    file_data: u64,
    files: usize,
    wave_archives: usize,
    /// Sound entry offsets in `info`, by name.
    by_name: HashMap<String, usize>,
}

const STRINGS: u16 = 0x2000;
const INFO: u16 = 0x2001;
const FILES: u16 = 0x2002;
const SOUND_TABLE: u16 = 0x2100;
const WAVE_ARCHIVE_TABLE: u16 = 0x2103;
const FILE_TABLE: u16 = 0x2106;
const STRING_TABLE: u16 = 0x2400;
const INTERNAL_FILE: u16 = 0x220C;
const WAVE_SOUND: u16 = 0x2202;

/// The header's entry for a block. Blocks are listed in the header as (type, offset, size).
fn block_entry(header: &Bytes, wanted: u16) -> Result<usize> {
    let count = usize::from(header.u16(0x10)?);
    for index in 0..count {
        let entry = 0x14 + index * 12;
        if header.u16(entry)? == wanted {
            return Ok(entry);
        }
    }
    Err(format!("The sound archive has no block {wanted:#x}."))
}

fn block_offset(header: &Bytes, wanted: u16) -> Result<u64> {
    Ok(u64::from(header.u32(block_entry(header, wanted)? + 4)?))
}

/// Reads a whole block's body, after its magic and size.
fn read_block(reader: &mut dyn ReadAt, header: &Bytes, wanted: u16) -> Result<(u64, Vec<u8>)> {
    let entry = block_entry(header, wanted)?;
    let (offset, size) = (u64::from(header.u32(entry + 4)?), header.at(entry + 8)?);
    Ok((offset + 8, reader.read_at(offset + 8, size.saturating_sub(8))?))
}

/// Finds a reference in a list of (type u16, padding, offset u32) entries.
fn reference(data: &Bytes, list: usize, count: usize, wanted: u16) -> Result<usize> {
    for index in 0..count {
        let entry = list + index * 8;
        if data.u16(entry)? == wanted {
            return data.at(entry + 4);
        }
    }
    Err(format!("The sound archive has no table {wanted:#x}."))
}

impl Archive {
    fn read(reader: &mut dyn ReadAt) -> Result<Self> {
        let header = reader.read_at(0, 0x40)?;
        let header = Bytes(&header);
        if header.slice(0, 4)? != b"CSAR" || header.u16(4)? != 0xFEFF {
            return Err("Not a 3DS sound archive.".to_owned());
        }
        let (_, strings) = read_block(reader, &header, STRINGS)?;
        let (_, info) = read_block(reader, &header, INFO)?;
        // Only the FILE block's position is needed. Its data is read clip by clip.
        let file_data = block_offset(&header, FILES)? + 8;

        let names = {
            let strings = Bytes(&strings);
            let table = reference(&strings, 0, 2, STRING_TABLE)?;
            let count = strings.at(table)?;
            (0..count)
                .map(|index| {
                    let entry = table + 4 + index * 12;
                    let (offset, size) = (strings.at(entry + 4)?, strings.at(entry + 8)?);
                    let text = strings.slice(table + offset, size.saturating_sub(1))?;
                    Ok(String::from_utf8_lossy(text).into_owned())
                })
                .collect::<Result<Vec<_>>>()?
        };

        let data = Bytes(&info);
        let sounds = reference(&data, 0, 8, SOUND_TABLE)?;
        let files = reference(&data, 0, 8, FILE_TABLE)?;
        let wave_archives = reference(&data, 0, 8, WAVE_ARCHIVE_TABLE)?;
        let mut by_name = HashMap::new();
        for index in 0..data.at(sounds)? {
            let entry = sounds + data.at(sounds + 4 + index * 8 + 4)?;
            // Flag bit 0 means the first optional field is the sound's string number.
            if data.u32(entry + 0x14)? & 1 == 1 {
                if let Some(name) = names.get(data.at(entry + 0x18)?) {
                    by_name.insert(name.clone(), entry);
                }
            }
        }
        Ok(Self { info, file_data, files, wave_archives, by_name })
    }

    fn table_entry(&self, table: usize, index: usize) -> Result<usize> {
        let data = Bytes(&self.info);
        if index >= data.at(table)? {
            return Err(format!("The sound archive has no entry {index}."));
        }
        Ok(table + data.at(table + 4 + index * 8 + 4)?)
    }

    /// Reads a file stored in the archive's FILE block.
    fn file(&self, reader: &mut dyn ReadAt, id: usize) -> Result<Vec<u8>> {
        let data = Bytes(&self.info);
        let entry = self.table_entry(self.files, id)?;
        if data.u16(entry)? != INTERNAL_FILE {
            return Err(format!("Sound file {id} is stored outside the archive."));
        }
        let location = entry + data.at(entry + 4)?;
        let (offset, size) = (data.u32(location + 4)?, data.at(location + 8)?);
        if offset == u32::MAX {
            return Err(format!("Sound file {id} is only inside a group."));
        }
        let file = reader.read_at(self.file_data + u64::from(offset), size)?;
        if file.len() != size {
            return Err(format!("Sound file {id} is cut short."));
        }
        Ok(file)
    }

    fn clip(&self, reader: &mut dyn ReadAt, name: &str) -> Result<Clip> {
        let data = Bytes(&self.info);
        let entry = *self.by_name.get(name).ok_or_else(|| format!("No sound named {name}."))?;
        let file_id = data.at(entry)?;
        if data.u16(entry + 0xC)? != WAVE_SOUND {
            return Err(format!("{name} is not a wave sound."));
        }
        let detail = entry + data.at(entry + 0x10)?;
        let index = data.at(detail)?;

        // The wave sound file: its INFO lists wave ids, then each sound's notes.
        let set = self.file(reader, file_id)?;
        let set = Bytes(&set);
        if set.slice(0, 4)? != b"CWSD" {
            return Err(format!("{name} does not lead to a wave sound file."));
        }
        let info = block(&set, 0)?;
        let wave_ids = info + set.at(info + 4)?;
        let sound_list = info + set.at(info + 12)?;
        if index >= set.at(sound_list)? {
            return Err(format!("{name} is missing from its wave sound file."));
        }
        let sound = sound_list + set.at(sound_list + 4 + index * 8 + 4)?;
        let notes = sound + set.at(sound + 20)?;
        if set.at(notes)? == 0 {
            return Err(format!("{name} has no notes."));
        }
        let note = notes + set.at(notes + 8)?;
        let wave_id = set.at(note)?;
        let id_entry = wave_ids + 4 + wave_id * 8;
        let (archive_id, wave) = (set.at(id_entry)? & 0xFF_FFFF, set.at(id_entry + 4)?);

        // The wave archive: read its header and index, then only the one wave.
        let archive_entry = self.table_entry(self.wave_archives, archive_id)?;
        let archive_file = data.at(archive_entry)?;
        let entry = self.table_entry(self.files, archive_file)?;
        if data.u16(entry)? != INTERNAL_FILE {
            return Err(format!("{name}'s wave archive is stored outside the archive."));
        }
        let location = entry + data.at(entry + 4)?;
        let archive_at = self.file_data + u64::from(data.u32(location + 4)?);
        let header = reader.read_at(archive_at, 0x40)?;
        let header = Bytes(&header);
        if header.slice(0, 4)? != b"CWAR" {
            return Err(format!("{name} does not lead to a wave archive."));
        }
        let (info_at, file_at) = (block(&header, 0)? as u64, block(&header, 1)? as u64);
        let index_entry = reader.read_at(archive_at + info_at + 4 + wave as u64 * 12, 12)?;
        let index_entry = Bytes(&index_entry);
        let (offset, size) = (u64::from(index_entry.u32(4)?), index_entry.at(8)?);
        let wave = reader.read_at(archive_at + file_at + offset, size)?;
        decode_wave(&wave)
    }
}

/// Decodes the first channel of a CWAV file.
pub fn decode_wave(wave: &[u8]) -> Result<Clip> {
    let file = Bytes(wave);
    if file.slice(0, 4)? != b"CWAV" {
        return Err("Not a wave file.".to_owned());
    }
    let (info, data) = (block(&file, 0)?, block(&file, 1)?);
    let encoding = file.u8(info)?;
    let sample_rate = file.u32(info + 4)?;
    let count = file.at(info + 12)?;
    let channels = info + 0x14;
    if file.at(channels)? == 0 {
        return Err("The wave has no channels.".to_owned());
    }
    let channel = channels + file.at(channels + 8)?;
    let samples_at = data + file.at(channel + 4)?;
    let samples = match encoding {
        0 => file.slice(samples_at, count)?.iter().map(|&byte| i16::from(byte as i8) << 8).collect(),
        1 => file.slice(samples_at, count * 2)?.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect(),
        2 => {
            let adpcm = channel + file.at(channel + 12)?;
            let mut coefficients = [0i16; 16];
            for (index, coefficient) in coefficients.iter_mut().enumerate() {
                *coefficient = file.i16(adpcm + index * 2)?;
            }
            let history = [file.i16(adpcm + 0x22)?, file.i16(adpcm + 0x24)?];
            // The last frame can stop at its last used byte, so pad it out.
            let mut frames = wave.get(samples_at..).ok_or("The wave's samples are missing.")?.to_vec();
            frames.resize(count.div_ceil(14) * 8, 0);
            decode_adpcm(&frames, &coefficients, history, count)
        }
        other => return Err(format!("Wave encoding {other} is not supported.")),
    };
    Ok(Clip { sample_rate, samples })
}

/// Decodes Nintendo DSP-ADPCM: 8-byte frames of a header byte and 14 four-bit samples.
fn decode_adpcm(data: &[u8], coefficients: &[i16; 16], history: [i16; 2], count: usize) -> Vec<i16> {
    let (mut last, mut before) = (i32::from(history[0]), i32::from(history[1]));
    let mut out = Vec::with_capacity(count);
    for frame in data.chunks_exact(8) {
        let header = frame[0];
        let scale = 1i32 << (header & 0xF);
        let pair = usize::from(header >> 4) * 2;
        let (first, second) = (i32::from(coefficients[pair % 16]), i32::from(coefficients[(pair + 1) % 16]));
        for index in 0..14 {
            if out.len() == count {
                return out;
            }
            let byte = frame[1 + index / 2];
            let nibble = if index % 2 == 0 { byte >> 4 } else { byte & 0xF };
            let nibble = i32::from(((nibble << 4) as i8) >> 4);
            let sample = ((nibble * scale) << 11) + 1024 + first * last + second * before;
            let sample = (sample >> 11).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            out.push(sample as i16);
            before = last;
            last = sample;
        }
    }
    out
}

/// Reads each named clip. Names the archive lacks are left out, with a note for the log.
pub fn read_clips(reader: &mut dyn ReadAt, names: &[&str]) -> Result<(Vec<Clip>, Vec<String>)> {
    let archive = Archive::read(reader)?;
    let mut clips = Vec::new();
    let mut problems = Vec::new();
    for name in names {
        match archive.clip(reader, name) {
            Ok(clip) if !clip.samples.is_empty() => clips.push(clip),
            Ok(_) => problems.push(format!("{name} is empty.")),
            Err(err) => problems.push(err),
        }
    }
    Ok((clips, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adpcm_follows_its_predictor() {
        // Coefficient pair 0 is (2048, 0): each sample is the last one plus the nibble times scale.
        let mut coefficients = [0i16; 16];
        coefficients[0] = 2048;
        let frame = [0x00, 0x12, 0xF0, 0, 0, 0, 0, 0];
        let samples = decode_adpcm(&frame, &coefficients, [10, 0], 4);
        assert_eq!(samples, vec![11, 13, 12, 12]);
    }

    #[test]
    fn rejects_other_files() {
        let mut data: &[u8] = b"not a sound archive at all, but long enough to read a header from it....";
        assert!(read_clips(&mut data, &VOICE_CLIPS).is_err());
        assert!(decode_wave(b"CWAV").is_err());
    }

    #[test]
    fn reads_real_voice_clips_when_an_archive_is_given() {
        let Ok(path) = std::env::var("ALBW_JACK_BCSAR") else { return };
        let mut file = Seekable(std::fs::File::open(path).unwrap());
        let (clips, problems) = read_clips(&mut file, &VOICE_CLIPS).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(clips.len(), VOICE_CLIPS.len());
        for clip in &clips {
            assert!((8000..=48000).contains(&clip.sample_rate), "{}", clip.sample_rate);
            // Real speech changes smoothly from sample to sample; noise from a bad decode does not.
            let level: i64 = clip.samples.iter().map(|&sample| i64::from(sample).abs()).sum();
            let change: i64 = clip.samples.windows(2).map(|pair| (i64::from(pair[1]) - i64::from(pair[0])).abs()).sum();
            assert!(level > 0 && change * 10 < level * 7, "{change} {level}");
        }
    }
}
