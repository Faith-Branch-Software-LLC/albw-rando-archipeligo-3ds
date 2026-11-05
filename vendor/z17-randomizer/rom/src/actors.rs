use {
    crate::{files::sarc::Sarc, spill, File, Result, RomSource},
    std::{cell::RefCell, path::Path, rc::Rc},
};

#[derive(Debug)]
pub struct Actors {
    archive: File<Sarc>,
    dirty: bool,
    /// Set when the archive can be read again from the ROM, so reads need not keep it in memory.
    source: Option<Rc<RefCell<RomSource>>>,
}

impl Actors {
    pub(crate) fn new(archive: File<Sarc>) -> Self {
        Self { archive, dirty: false, source: None }
    }

    pub(crate) fn lazy(archive: File<Sarc>, source: Rc<RefCell<RomSource>>) -> Self {
        Self { archive, dirty: false, source: Some(source) }
    }

    /// Frees the archive after a read, unless it has edits that exist only in memory.
    fn release(&self) {
        if spill::release_after_read() {
            self.drop_clean();
        }
    }

    /// Returns true if the archive can come back from the ROM, and frees it.
    fn drop_clean(&self) -> bool {
        if let (false, Some(source)) = (self.dirty, &self.source) {
            self.archive.get().release(source, self.archive.path());
            true
        } else {
            false
        }
    }

    /// Frees the archive, writing it to disk if the ROM copy would not be enough.
    pub fn spill(&self) -> Result<()> {
        if self.drop_clean() {
            Ok(())
        } else {
            self.archive.get().spill()
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        let found = self.archive.get().contains(format!("World/Actor/{}.bch", name)).unwrap_or(false);
        self.release();
        found
    }

    pub fn get_actor_bch(&self, name: &str) -> Result<Actor> {
        let actor = self.archive.get().extract(format!("World/Actor/{}.bch", name));
        self.release();
        actor
    }

    pub fn get_actor_kcl(&self, name: &str) -> Result<Actor> {
        let actor = self.archive.get().extract(format!("World/Actor/{}.kcl", name));
        self.release();
        actor
    }

    pub fn get_map_actor_bch(&self, name: &str) -> Result<Actor> {
        let actor = self.archive.get().extract(format!("World/MapActor/{}.bch", name));
        self.release();
        actor
    }

    pub fn get_map_actor_kcl(&self, name: &str) -> Result<Actor> {
        let actor = self.archive.get().extract(format!("World/MapActor/{}.kcl", name));
        self.release();
        actor
    }

    pub fn add(&mut self, actor: Actor) -> Result<()> {
        self.dirty = true;
        self.archive.get_mut().add(actor)
    }

    pub fn update(&mut self, actor: Actor) -> Result<()> {
        self.dirty = true;
        self.archive.get_mut().update(actor)
    }

    pub fn into_archive(self) -> Option<File<Sarc>> {
        self.dirty.then(|| self.archive.map(Sarc::compress))
    }

    pub fn dump<P>(self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        if self.dirty {
            self.archive.map(Sarc::compress).dump(path)?;
        }
        Ok(())
    }
}

pub type Actor = File<Box<[u8]>>;
