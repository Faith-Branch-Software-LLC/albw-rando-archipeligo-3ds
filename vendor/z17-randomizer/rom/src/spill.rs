//! Keeps the patcher inside a memory budget by moving archives out of memory.
//!
//! With no budget set, nothing is written to disk and reads free their archives right away. Consoles
//! set a budget that suits their RAM, so a model with more memory reads the SD card less.

use std::{
    fs,
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
    BUDGET.lock().unwrap().as_ref().is_some_and(|budget| (budget.usage)() > budget.limit)
}

/// Whether a read should free its archive. With no budget set, reads always do.
pub(crate) fn release_after_read() -> bool {
    BUDGET.lock().unwrap().as_ref().map_or(true, |budget| (budget.usage)() > budget.limit)
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
    let dir = BUDGET
        .lock()
        .unwrap()
        .as_ref()
        .map(|budget| budget.dir.clone())
        .ok_or_else(|| std::io::Error::other("no spill folder set"))?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.bin", NEXT_ID.fetch_add(1, Relaxed)));
    fs::write(&path, data)?;
    SPILLS.fetch_add(1, Relaxed);
    BYTES_WRITTEN.fetch_add(data.len(), Relaxed);
    Ok(path)
}

pub(crate) fn take(path: &PathBuf) -> std::io::Result<Box<[u8]>> {
    let data = fs::read(path)?;
    let _ = fs::remove_file(path);
    RELOADS.fetch_add(1, Relaxed);
    Ok(data.into_boxed_slice())
}
