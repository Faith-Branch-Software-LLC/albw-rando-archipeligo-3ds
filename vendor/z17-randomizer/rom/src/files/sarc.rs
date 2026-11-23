use {
    super::{align, File, FromFile, IntoBytes, yaz0},
    crate::{spill, Error, Result, RomSource},
    bytey::*,
    log::debug,
    std::{
        cell::{Ref, RefCell},
        collections::BTreeMap,
        rc::Rc,
    },
};

#[derive(Debug)]
pub struct Sarc(RefCell<Inner>);

impl Sarc {
    pub fn from(data: Box<[u8]>) -> Self {
        Self(RefCell::new(Inner::Compressed(data)))
    }

    /// An archive that is read from the ROM on first use.
    pub(crate) fn lazy(source: Rc<RefCell<RomSource>>, path: String) -> Self {
        Self(RefCell::new(Inner::OnRom(source, path)))
    }

    /// Drops the loaded data of an unmodified lazy archive, given where it came from.
    pub(crate) fn release(&self, source: &Rc<RefCell<RomSource>>, path: &str) {
        if let Ok(mut inner) = self.0.try_borrow_mut() {
            *inner = Inner::OnRom(source.clone(), path.to_owned());
        }
    }

    /// Moves the loaded data to disk. It comes back on next use. On failure it stays in memory.
    pub(crate) fn spill(&self) -> Result<()> {
        let Ok(mut inner) = self.0.try_borrow_mut() else {
            return Ok(());
        };
        // A decompressed archive is written file by file, so spilling needs no large block.
        let (path, compressed) = match &*inner {
            Inner::Decompressed(archive) => (spill::write_with(|out| archive.write_spill(out))?, false),
            Inner::Compressed(data) => (spill::write(data)?, true),
            _ => return Ok(()),
        };
        *inner = Inner::Spilled(path, compressed);
        Ok(())
    }

    fn decompress(&self) -> Result<Ref<'_, Archive>> {
        self.0.borrow_mut().decompress()?;
        Ok(Ref::map(self.0.borrow(), |inner| match inner {
            Inner::Decompressed(archive) => archive,
            _ => unreachable!(),
        }))
    }

    fn decompress_mut(&mut self) -> Result<&mut Archive> {
        self.0.get_mut().decompress()
    }

    pub fn contains<P>(&self, path: P) -> Result<bool>
    where
        P: Into<String>,
    {
        let path = path.into();
        let archive = self.decompress()?;
        Ok(archive.get(&path).is_ok())
    }

    pub fn read<P>(&self, path: P) -> Result<File<Ref<'_, [u8]>>>
    where
        P: Into<String>,
    {
        let path = path.into();
        debug!("Reading {} from archive", &path);
        let archive = self.decompress()?;
        let data = Ref::filter_map(archive, |archive| archive.get(&path).ok())
            .map_err(|_| Error::new(format!("File not found: '{}'.", path)))?;
        Ok(File::new(path, data))
    }

    pub fn read_from_file<'a, T>(&'a self, args: &T::PathArgs) -> Result<File<T>>
    where
        T: FromFile<Input = Ref<'a, [u8]>>,
    {
        let path = T::path(args);
        debug!("Reading {} from archive", &path);
        let archive = self.decompress()?;
        let data = Ref::filter_map(archive, |archive| archive.get(&path).ok())
            .map_err(|_| Error::new(format!("File not found: '{}'.", path)))?;
        Ok(File::new(path, T::from_file(data)?))
    }

    pub fn extract<P>(&self, path: P) -> Result<File<Box<[u8]>>>
    where
        P: Into<String>,
    {
        let path = path.into();
        debug!("Extracting {} from archive", &path);
        let archive = self.decompress()?;
        let data = archive.get(&path)
            .map_err(|_| Error::new(format!("File not found: '{}'.", path)))?;
        Ok(File::new(path, data.into()))
    }

    pub fn open<P>(&mut self, path: P) -> Result<File<&mut [u8]>>
    where
        P: Into<String>,
    {
        let path = path.into();
        debug!("Opening {} from archive", &path);
        let data = self.decompress_mut()?.get_mut(&path)
            .map_err(|_| Error::new(format!("File not found: '{}'.", path)))?;
        Ok(File::new(path, data))
    }

    pub fn open_from_file<'s, T>(&'s mut self, args: &T::PathArgs) -> Result<File<T>>
    where
        T: FromFile<Input = &'s mut [u8]>,
    {
        let path = T::path(args);
        let data = self.decompress_mut()?.get_mut(&path)
            .map_err(|_| Error::new(format!("File not found: '{}'.", path)))?;
        Ok(File::new(path, T::from_file(data)?))
    }

    pub fn add(&mut self, file: File<Box<[u8]>>) -> Result<()> {
        self.decompress_mut()?.add(file);
        Ok(())
    }

    pub fn update(&mut self, file: File<Box<[u8]>>) -> Result<()> {
        self.decompress_mut()?.update(file);
        Ok(())
    }

    pub fn compress(self) -> Self {
        match self.0.into_inner() {
            Inner::OnRom(source, path) => Self::lazy(source, path),
            Inner::Compressed(data) => Self::from(data),
            Inner::Decompressed(archive) => Self::from(compress(&archive.into_bytes())),
            Inner::Spilled(path, true) => Self::from(spill::take(&path).expect("spill file was just written")),
            Inner::Spilled(path, false) => {
                Self::from(compress(&Archive::load_spill(&path).expect("spill file was just written").into_bytes()))
            }
        }
    }
}

impl IntoBytes for Sarc {
    fn into_bytes(self) -> Box<[u8]> {
        match self.0.into_inner() {
            Inner::OnRom(source, path) => {
                source.borrow_mut().read(path).expect("archive was readable when its scene loaded").into_inner()
            }
            Inner::Compressed(data) => data,
            Inner::Decompressed(archive) => archive.into_bytes(),
            Inner::Spilled(path, true) => spill::take(&path).expect("spill file was just written"),
            Inner::Spilled(path, false) => {
                Archive::load_spill(&path).expect("spill file was just written").into_bytes()
            }
        }
    }
}

impl IntoBytes for Ref<'_, [u8]> {
    fn into_bytes(self) -> Box<[u8]> {
        (&*self).into()
    }
}

#[derive(Debug)]
pub enum Inner {
    OnRom(Rc<RefCell<RomSource>>, String),
    Compressed(Box<[u8]>),
    Decompressed(Archive),
    /// On disk. The flag tells whether the bytes are still Yaz0 compressed.
    Spilled(std::path::PathBuf, bool),
}

impl Inner {
    fn decompress(&mut self) -> Result<&mut Archive> {
        if let Self::Spilled(path, compressed) = self {
            *self = if *compressed { Self::Compressed(spill::take(path)?) } else { Self::Decompressed(Archive::load_spill(path)?) };
        }
        if let Self::OnRom(source, path) = self {
            let data = source.borrow_mut().read(path.clone())?;
            *self = Self::Compressed(data.into_inner());
        }
        if let Self::Compressed(data) = self {
            // Unpacking file by file needs no block the size of the whole archive. An archive
            // laid out in a way that cannot be unpacked in order takes the old way.
            let archive = match Archive::from_yaz0(data) {
                Ok(archive) => archive,
                Err(_) => Archive::from(
                    yaz0::decompress(data)
                        .map_err(|_| Error::new("Archive could not be decompressed.".to_string()))?
                        .into(),
                )?,
            };
            *self = Self::Decompressed(archive);
        }
        match self {
            Self::Decompressed(archive) => Ok(archive),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub struct Archive {
    multiplier: u32,
    files: BTreeMap<u32, Vec<u8>>,
}

impl Archive {
    pub fn from(file: Box<[u8]>) -> Result<Self> {
        typedef! { struct Header: TryFromBytes<'_> [0x14] {
            #b"SARC",
            [4] header_len: u16 where header_len == 0x14,
            [6] bom: u16 where bom == 0xFEFF,
            [8] len: u32,
            [0xC] offset: u32,
        }}
        let (header, sfat) = Header::try_from_slice(&file)?;
        if header.len as usize == file.len() {
            typedef! { struct SFAT: TryFromBytes<'_> [0xC] {
                #b"SFAT",
                [6] count: u16,
                [8] multiplier: u32,
            }}
            let (sfat, nodes) = SFAT::try_from_slice(sfat)?;
            let mut nodes: Vec<_> = nodes.into();
            nodes.truncate(0x10 * sfat.count as usize);
            let data = &file[header.offset as usize..];
            typedef! { struct Node: TryFromBytes<'_> [0x10] {
                [0] hash: u32,
                [4] attr: u32,
                [8] start: u32,
                [0xc] end: u32,
            }}
            let files = nodes
                .chunks(0x10)
                .map(|chunk| {
                    let (node, _) = Node::try_from_slice(chunk)?;
                    if node.attr != 0 {
                        Err(Error::new("Hash collision".to_string()))
                    } else {
                        let start = node.start as usize;
                        let end = node.end as usize;
                        Ok((node.hash, data[start..end].to_vec()))
                    }
                })
                .collect::<Result<_>>()?;
            Ok(Self { multiplier: sfat.multiplier, files })
        } else {
            Err(Error::new("unimpl113".to_string()))
        }
    }

    /// Decompresses Yaz0 data straight into separate files, so no block holds the whole archive.
    /// Gives the same result as decompressing and then calling [`Archive::from`].
    fn from_yaz0(data: &[u8]) -> Result<Self> {
        let mut unpacker = Unpacker::default();
        let total = yaz0::decompress_with(data, |piece| unpacker.feed(piece))?;
        unpacker.finish(total)
    }

    /// Writes every file with its exact length. SARC cannot do this, because its entries include
    /// the padding after each file, and some game files reject that padding when read back.
    fn write_spill(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        out.write_all(&self.multiplier.to_le_bytes())?;
        out.write_all(&(self.files.len() as u32).to_le_bytes())?;
        for (hash, file) in &self.files {
            out.write_all(&hash.to_le_bytes())?;
            out.write_all(&(file.len() as u32).to_le_bytes())?;
            out.write_all(file)?;
        }
        Ok(())
    }

    /// Reads back what `write_spill` wrote, one file at a time, and deletes the spill file.
    fn load_spill(path: &std::path::PathBuf) -> std::io::Result<Self> {
        use std::io::Read;
        fn word(input: &mut impl Read) -> std::io::Result<u32> {
            let mut bytes = [0; 4];
            input.read_exact(&mut bytes)?;
            Ok(u32::from_le_bytes(bytes))
        }
        let mut input = spill::open(path)?;
        let multiplier = word(&mut input)?;
        let count = word(&mut input)?;
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let hash = word(&mut input)?;
            let len = word(&mut input)? as usize;
            let mut file = vec![0; len];
            input.read_exact(&mut file)?;
            files.insert(hash, file);
        }
        drop(input);
        spill::done(path);
        Ok(Self { multiplier, files })
    }

    fn hash(&self, path: &str) -> u32 {
        path.chars().fold(0, |hash, ch| (ch as u32) + hash.wrapping_mul(self.multiplier))
    }

    fn get(&self, path: &str) -> Result<&[u8]> {
        self.files.get(&self.hash(path)).map(|file| &file[..]).ok_or(Error::new(format!("File not found: {path}")))
    }

    fn get_mut(&mut self, path: &str) -> Result<&mut [u8]> {
        self.files.get_mut(&self.hash(path)).map(|file| &mut file[..]).ok_or(Error::new(format!("File not found: {path}")))
    }

    fn update(&mut self, file: File<Box<[u8]>>) {
        debug!("Updating: {}", file.path);
        if self.files.contains_key(&self.hash(&file.path)) {
            self.files.insert(self.hash(&file.path), file.inner.into());
        } else {
            panic!("Can't update non-existent file: {}", file.path);
        }
    }

    fn add(&mut self, file: File<Box<[u8]>>) {
        debug!("Add {}", file.path);
        if self.files.contains_key(&self.hash(&file.path)) {
            debug!("Not adding duplicate file: {}", file.path);
        } else {
            self.files.insert(self.hash(&file.path), file.inner.into());
        }
    }
}

/// Splits a SARC into its files as its bytes arrive.
#[derive(Default)]
struct Unpacker {
    /// Everything before the file data: the header and the file table.
    head: Vec<u8>,
    data_offset: Option<usize>,
    multiplier: u32,
    /// Each file as (hash, start, end) within the file data, in order.
    nodes: Vec<(u32, usize, usize)>,
    next: usize,
    current: Vec<u8>,
    files: BTreeMap<u32, Vec<u8>>,
    /// Bytes received so far.
    position: usize,
    /// The archive's length as its header gives it.
    stated_len: usize,
}

impl Unpacker {
    fn word(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn half(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    /// Reads the file table once the head has arrived.
    fn read_table(&mut self, data_offset: usize) -> Result<()> {
        let head = &self.head;
        if head.len() < 0x20 || &head[0x14..0x18] != b"SFAT" {
            return Err(Error::new("SARC has no file table".to_string()));
        }
        let count = Self::half(head, 0x1A) as usize;
        self.multiplier = Self::word(head, 0x1C);
        let table_end = 0x20 + 0x10 * count;
        if table_end > data_offset {
            return Err(Error::new("SARC file table runs into its data".to_string()));
        }
        let mut nodes = Vec::with_capacity(count);
        for index in 0..count {
            let at = 0x20 + 0x10 * index;
            if Self::word(head, at + 4) != 0 {
                return Err(Error::new("Hash collision".to_string()));
            }
            let (start, end) = (Self::word(head, at + 8) as usize, Self::word(head, at + 12) as usize);
            if end < start {
                return Err(Error::new("SARC file ends before it starts".to_string()));
            }
            nodes.push((Self::word(head, at), start, end));
        }
        nodes.sort_by_key(|&(_, start, end)| (start, end));
        // Files that share bytes would need the data twice, which this cannot do.
        if nodes.windows(2).any(|pair| pair[1].1 < pair[0].2) {
            return Err(Error::new("SARC files overlap".to_string()));
        }
        // Two entries with one hash would replace each other, as in `Archive::from`, but in another
        // order than here.
        if nodes.iter().map(|node| node.0).collect::<std::collections::BTreeSet<_>>().len() != nodes.len() {
            return Err(Error::new("SARC has a repeated hash".to_string()));
        }
        self.nodes = nodes;
        self.data_offset = Some(data_offset);
        self.head = Vec::new();
        Ok(())
    }

    fn feed(&mut self, mut piece: &[u8]) -> Result<()> {
        loop {
            let Some(data_offset) = self.data_offset else {
                if piece.is_empty() {
                    return Ok(());
                }
                // The header says where the file data starts.
                let wanted = if self.head.len() < 0x14 { 0x14 } else { Self::word(&self.head, 0xC) as usize };
                let count = piece.len().min(wanted - self.head.len());
                self.head.extend_from_slice(&piece[..count]);
                piece = &piece[count..];
                self.position += count;
                if self.head.len() == 0x14 && wanted == 0x14 {
                    if &self.head[..4] != b"SARC" || Self::half(&self.head, 4) != 0x14 || Self::half(&self.head, 6) != 0xFEFF {
                        return Err(Error::new("Not a SARC archive".to_string()));
                    }
                    self.stated_len = Self::word(&self.head, 8) as usize;
                    let offset = Self::word(&self.head, 0xC) as usize;
                    if offset < 0x20 {
                        return Err(Error::new("SARC data starts inside its header".to_string()));
                    }
                } else if self.head.len() == wanted {
                    self.read_table(wanted)?;
                }
                continue;
            };
            let at = self.position - data_offset;
            // Store every file that is complete.
            while let Some(&(hash, start, end)) = self.nodes.get(self.next) {
                if at >= start && self.current.len() == end - start {
                    self.files.insert(hash, std::mem::take(&mut self.current));
                    self.next += 1;
                } else {
                    break;
                }
            }
            if piece.is_empty() {
                return Ok(());
            }
            let Some(&(_, start, end)) = self.nodes.get(self.next) else {
                // Padding after the last file.
                self.position += piece.len();
                return Ok(());
            };
            let count = if at < start {
                piece.len().min(start - at)
            } else {
                if self.current.capacity() == 0 {
                    self.current.reserve_exact(end - start);
                }
                let count = piece.len().min(end - at);
                self.current.extend_from_slice(&piece[..count]);
                count
            };
            piece = &piece[count..];
            self.position += count;
        }
    }

    fn finish(mut self, total: usize) -> Result<Archive> {
        self.feed(&[])?;
        let Some(_) = self.data_offset else {
            return Err(Error::new("SARC ended inside its header".to_string()));
        };
        // `Archive::from` takes only archives whose stated length matches their data.
        if self.next != self.nodes.len() || self.position != total || self.stated_len != total {
            return Err(Error::new("SARC ended inside a file".to_string()));
        }
        Ok(Archive { multiplier: self.multiplier, files: self.files })
    }
}

impl IntoBytes for Archive {
    fn into_bytes(self) -> Box<[u8]> {
        let count = self.files.len() as u16;
        // Sizing this up front keeps a growing buffer from leaving holes in tight memory.
        let data_size: usize = self.files.values().map(|file| align::<0x80>(file.len() as u32) as usize).sum();
        let mut buf = Vec::with_capacity(0x100 + 0x10 * self.files.len() + data_size);
        buf.extend_from_slice(b"SARC");
        buf.extend_from_slice(&[0x14, 0, 0xFF, 0xFE]);
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&[0, 1, 0, 0]);
        buf.extend_from_slice(b"SFAT");
        buf.extend_from_slice(&[0xC, 0]);
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&self.multiplier.to_le_bytes());
        let mut offset = 0u32;
        for (hash, file) in &self.files {
            buf.extend_from_slice(&hash.to_le_bytes());
            buf.extend_from_slice(&[0, 0, 0, 0]);
            buf.extend_from_slice(&offset.to_le_bytes());
            offset = align::<0x80>(offset + file.len() as u32);
            buf.extend_from_slice(&offset.to_le_bytes());
        }
        buf.extend_from_slice(b"SFNT");
        buf.extend_from_slice(&[0x8, 0, 0, 0]);
        buf.resize(align::<0x80>(buf.len() as u32) as usize, 0);
        let file_offset = buf.len() as u32;
        buf[0xc..0x10].copy_from_slice(&file_offset.to_le_bytes());
        for (_, file) in &self.files {
            buf.extend_from_slice(&file);
            buf.resize(align::<0x80>(buf.len() as u32) as usize, 0);
        }
        let size = buf.len() as u32;
        buf[8..0xc].copy_from_slice(&size.to_le_bytes());
        buf.into()
    }
}

fn compress(data: &[u8]) -> Box<[u8]> {
    yaz0::compress(&data)
        .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

#[cfg(test)]
mod unpack_tests {
    use super::*;

    /// Builds a SARC with the given files, the way the game's archives are laid out.
    fn sarc(files: &[(u32, &[u8])]) -> Vec<u8> {
        let count = files.len();
        let head = 0x20 + 0x10 * count + 8;
        let data_offset = (head + 0x7F) & !0x7F;
        let mut data = Vec::new();
        let mut nodes = Vec::new();
        for (hash, file) in files {
            let start = data.len();
            data.extend_from_slice(file);
            nodes.push((*hash, start, data.len()));
            data.resize((data.len() + 0x7F) & !0x7F, 0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"SARC");
        out.extend_from_slice(&0x14u16.to_le_bytes());
        out.extend_from_slice(&0xFEFFu16.to_le_bytes());
        out.extend_from_slice(&((data_offset + data.len()) as u32).to_le_bytes());
        out.extend_from_slice(&(data_offset as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(b"SFAT");
        out.extend_from_slice(&0xCu16.to_le_bytes());
        out.extend_from_slice(&(count as u16).to_le_bytes());
        out.extend_from_slice(&0x65u32.to_le_bytes());
        for (hash, start, end) in nodes {
            for value in [hash, 0, start as u32, end as u32] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out.extend_from_slice(b"SFNT");
        out.extend_from_slice(&[0; 4]);
        out.resize(data_offset, 0);
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn unpacking_in_pieces_matches_unpacking_whole() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let repeated = vec![9u8; 70_000];
        let raw = sarc(&[(3, &big), (1, b"tiny"), (7, &[]), (5, &repeated)]);
        let packed = yaz0::compress(&raw);
        let whole = Archive::from(yaz0::decompress(&packed).unwrap().into()).unwrap();
        let pieces = Archive::from_yaz0(&packed).unwrap();
        assert_eq!(whole.multiplier, pieces.multiplier);
        assert_eq!(whole.files, pieces.files);
        assert_eq!(pieces.files[&3], big);
        assert!(pieces.files[&7].is_empty());
    }

    #[test]
    fn a_cut_short_archive_is_refused() {
        let raw = sarc(&[(3, b"some file data")]);
        let mut cut = raw[..raw.len() - 4].to_vec();
        cut[8..12].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        assert!(Archive::from_yaz0(&yaz0::compress(&cut)).is_err());
    }
}
