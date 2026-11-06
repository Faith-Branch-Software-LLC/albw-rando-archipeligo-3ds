//! Finishes adding a model or voice mod to a patch.
//!
//! The patcher reads mods as part of the game (see `Rom::overlay`), so any file it writes already
//! has both the mod's changes and the randomizer's. Mod files it never touches are copied here.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const COPY_BUFFER: usize = 64 * 1024;

/// What happened while copying one mod's files.
#[derive(Debug, Default)]
pub struct Report {
    /// Files copied as they are.
    pub copied: u32,
    /// Files already in the patch, which the patcher built from the mod.
    pub kept: Vec<String>,
    /// Problems, each naming a file that was left out.
    pub failures: Vec<String>,
}

/// Lists a folder's files, skipping hidden files such as the ones macOS adds to SD cards.
fn list_files(folder: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            list_files(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let result = (|| {
        let mut source = fs::File::open(from)?;
        let mut target = fs::File::create(to)?;
        let mut buffer = vec![0; COPY_BUFFER];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            target.write_all(&buffer[..count])?;
        }
    })();
    if result.is_err() {
        // A partial file could crash the game, so it must not stay in the patch.
        let _ = fs::remove_file(to);
    }
    result
}

/// Lists a mod's files with their sizes in bytes, for the run log.
pub fn list(source: &Path) -> io::Result<Vec<(String, u64)>> {
    let mut files = Vec::new();
    list_files(source, &mut files)?;
    files.sort();
    files
        .iter()
        .map(|file| {
            let relative = file.strip_prefix(source).expect("listed under the source folder");
            Ok((relative.display().to_string(), fs::metadata(file)?.len()))
        })
        .collect()
}

/// Copies the files of the mod in `source`, a `romfs` folder, that `output` does not have yet.
pub fn copy_missing(source: &Path, output: &Path, on_file: &dyn Fn(u32, u32)) -> Report {
    let mut report = Report::default();
    let mut files = Vec::new();
    if let Err(err) = list_files(source, &mut files) {
        report.failures.push(format!("{}: {err}", source.display()));
        return report;
    }
    files.sort();
    let total = files.len() as u32;
    for (index, from) in files.iter().enumerate() {
        on_file(index as u32, total);
        let relative = from.strip_prefix(source).expect("listed under the source folder");
        let to = output.join(relative);
        if to.exists() {
            report.kept.push(relative.display().to_string());
            continue;
        }
        match copy_file(from, &to) {
            Ok(()) => report.copied += 1,
            Err(err) => report.failures.push(format!("{}: {err}", relative.display())),
        }
    }
    report
}
