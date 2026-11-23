//! Keeps the patcher inside a memory budget by moving archives out of memory.
//!
//! With no budget set, nothing is written to disk and reads free their archives right away. Consoles
//! set a budget that suits their RAM, so a model with more memory reads the SD card less.

use std::{
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering::Relaxed},
        Mutex,
    },
};

/// How much memory the patcher may use, and where archives go when it uses more.
#[derive(Clone, Debug)]
pub struct Budget {
    /// Holds spilled archives. Created on first use, and removed by [`finish`].
    pub dir: PathBuf,
    /// Bytes in use above which archives leave memory.
    pub limit: usize,
    /// Reports the bytes in use now.
    pub usage: fn() -> usize,
    /// Moves every archive to disk before compressing, when memory is too tight for compression
    /// to work beside them.
    pub clear_before_compress: bool,
    /// Reports the largest block that could be allocated now, when memory can split into pieces.
    /// Archives also leave memory when this drops below `headroom`, since a large archive needs
    /// one block for itself even when the total in use is low.
    pub largest_free: Option<fn() -> usize>,
    /// The largest free block to keep, in bytes.
    pub headroom: usize,
}

impl Budget {
    fn tight(&self) -> bool {
        self.tight_with(0, 0)
    }

    /// Whether memory is short with `extra` more bytes about to be used, the largest piece of
    /// them `piece` bytes.
    fn tight_with(&self, extra: usize, piece: usize) -> bool {
        (self.usage)() + extra > self.limit
            || self.largest_free.is_some_and(|largest| largest() < self.headroom.max(piece))
    }
}

/// Totals for one run.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub spills: u32,
    pub reloads: u32,
    pub bytes_written: usize,
}

static BUDGET: Mutex<Option<Budget>> = Mutex::new(None);
static NEXT_ID: AtomicU32 = AtomicU32::new(0);
static SPILLS: AtomicU32 = AtomicU32::new(0);
static RELOADS: AtomicU32 = AtomicU32::new(0);
static BYTES_WRITTEN: AtomicUsize = AtomicUsize::new(0);

/// Sets the budget for the next patch.
pub fn configure(budget: Budget) {
    *BUDGET.lock().unwrap() = Some(budget);
}

/// Whether a budget is set, which also means output is written as soon as it is ready.
pub fn active() -> bool {
    BUDGET.lock().unwrap().is_some()
}

/// Whether every archive should leave memory before compression starts.
pub fn clear_before_compress() -> bool {
    BUDGET.lock().unwrap().as_ref().is_some_and(|budget| budget.clear_before_compress)
}

/// Whether archives should leave memory now.
pub fn over_limit() -> bool {
    BUDGET.lock().unwrap().as_ref().is_some_and(Budget::tight)
}

/// Whether archives should leave memory to make room for `extra` more bytes, the largest piece
/// of them `piece` bytes.
pub fn over_limit_with(extra: usize, piece: usize) -> bool {
    BUDGET.lock().unwrap().as_ref().is_some_and(|budget| budget.tight_with(extra, piece))
}

/// Whether a read should free its archive. With no budget set, reads always do.
pub(crate) fn release_after_read() -> bool {
    BUDGET.lock().unwrap().as_ref().map_or(true, Budget::tight)
}

/// Clears the budget, deletes any spill files, and returns the totals.
pub fn finish() -> Stats {
    if let Some(budget) = BUDGET.lock().unwrap().take() {
        let _ = fs::remove_dir_all(budget.dir);
    }
    Stats {
        spills: SPILLS.swap(0, Relaxed),
        reloads: RELOADS.swap(0, Relaxed),
        bytes_written: BYTES_WRITTEN.swap(0, Relaxed),
    }
}

pub(crate) fn write(data: &[u8]) -> std::io::Result<PathBuf> {
    write_with(|out| out.write_all(data))
}

/// How much a spill file moves per call to the SD card. Each call costs a few milliseconds on a
/// 3DS however little it moves, and an archive holds hundreds of files, so small pieces made
/// patching take twice as long. This is still far below the size of one archive.
const IO_CHUNK: usize = 512 * 1024;

/// Writes a spill file piece by piece, so the data never needs one large block in memory.
pub(crate) fn write_with(fill: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> std::io::Result<PathBuf> {
    let dir = BUDGET
        .lock()
        .unwrap()
        .as_ref()
        .map(|budget| budget.dir.clone())
        .ok_or_else(|| std::io::Error::other("no spill folder set"))?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.bin", NEXT_ID.fetch_add(1, Relaxed)));
    let mut out = CountingWriter { inner: BufWriter::with_capacity(IO_CHUNK, CountedFile(fs::File::create(&path)?)), written: 0 };
    let result = io::timed(io::Kind::Spill, || {
        let result = fill(&mut out).and_then(|()| out.inner.flush());
        (result, out.written)
    });
    if let Err(err) = result {
        drop(out);
        let _ = fs::remove_file(&path);
        return Err(err);
    }
    SPILLS.fetch_add(1, Relaxed);
    BYTES_WRITTEN.fetch_add(out.written, Relaxed);
    Ok(path)
}

/// A file that counts its calls to the SD card.
struct CountedFile(fs::File);

impl Write for CountedFile {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        io::count_call();
        self.0.write(data)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Read for CountedFile {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        io::count_call();
        self.0.read(out)
    }
}

struct CountingWriter<W> {
    inner: W,
    written: usize,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(data)?;
        self.written += count;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Opens a spill file for reading piece by piece. The file is deleted once read with [`done`].
pub(crate) fn open(path: &PathBuf) -> std::io::Result<impl Read> {
    Ok(BufReader::with_capacity(IO_CHUNK, CountedFile(fs::File::open(path)?)))
}

/// Deletes a spill file that [`open`] has read.
pub(crate) fn done(path: &PathBuf) {
    let _ = fs::remove_file(path);
    RELOADS.fetch_add(1, Relaxed);
}

pub(crate) fn take(path: &PathBuf) -> std::io::Result<Box<[u8]>> {
    let data = io::timed(io::Kind::Spill, || {
        let data = fs::read(path);
        let size = data.as_ref().map_or(0, Vec::len);
        (data, size)
    })?;
    let _ = fs::remove_file(path);
    RELOADS.fetch_add(1, Relaxed);
    Ok(data.into_boxed_slice())
}

/// Time and bytes spent on the SD card, by kind, to tell slow reads and writes from slow work.
pub mod io {
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
    use std::time::Instant;

    #[derive(Clone, Copy)]
    pub enum Kind {
        /// Game files, from the game or a mod folder.
        Game,
        /// Archives moved out of memory and back.
        Spill,
        /// The finished files.
        Output,
    }

    // The 3DS has no 64-bit atomics, so sizes are counted in KB.
    static MS: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
    static KB: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

    static SPILL_CALLS: AtomicU32 = AtomicU32::new(0);

    /// Counts one read or write call on a spill file.
    pub(crate) fn count_call() {
        SPILL_CALLS.fetch_add(1, Relaxed);
    }

    /// Runs `work`, counting its time and the bytes it reports.
    pub fn timed<T>(kind: Kind, work: impl FnOnce() -> (T, usize)) -> T {
        let started = Instant::now();
        let (result, bytes) = work();
        MS[kind as usize].fetch_add(started.elapsed().as_millis() as u32, Relaxed);
        KB[kind as usize].fetch_add((bytes / 1024) as u32, Relaxed);
        result
    }

    /// Totals so far, such as "game 12.3 MB in 4.5 s".
    pub fn report() -> String {
        let part = |kind: Kind, name: &str| {
            let ms = MS[kind as usize].load(Relaxed);
            let kb = KB[kind as usize].load(Relaxed);
            format!("{name} {:.1} MB in {:.1} s", kb as f64 / 1024.0, ms as f64 / 1000.0)
        };
        format!(
            "{}, {} ({} calls), {}",
            part(Kind::Game, "game"),
            part(Kind::Spill, "spill"),
            SPILL_CALLS.load(Relaxed),
            part(Kind::Output, "output")
        )
    }

    pub fn reset() {
        SPILL_CALLS.store(0, Relaxed);
        for counter in MS.iter().chain(KB.iter()) {
            counter.store(0, Relaxed);
        }
    }
}
