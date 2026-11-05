//! A memory pool that holds only large allocations.
//!
//! On a general heap, small objects land between large blocks. When a large block is freed, its
//! neighbors stop it from merging into a bigger gap, and a later large request can fail with plenty
//! of memory free. Keeping large blocks here means every gap merges with its free neighbors.
//!
//! The pool takes no lock, so the caller must provide one.
#![no_std]

const CAPACITY: usize = 2048;
const GRANULE: usize = 16;

#[derive(Clone, Copy)]
struct Block {
    offset: usize,
    len: usize,
    used: bool,
}

/// Block records live outside the managed memory and stay sorted by offset.
pub struct Pool {
    base: usize,
    size: usize,
    blocks: [Block; CAPACITY],
    count: usize,
    used: usize,
    peak: usize,
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

impl Pool {
    pub const fn empty() -> Self {
        Self {
            base: 0,
            size: 0,
            blocks: [Block { offset: 0, len: 0, used: false }; CAPACITY],
            count: 0,
            used: 0,
            peak: 0,
        }
    }

    /// Takes ownership of a memory range for the life of the pool.
    ///
    /// # Safety
    /// The range must be valid for reads and writes, and used by nothing else, for as long as the
    /// pool hands out pointers into it.
    pub unsafe fn init(&mut self, base: *mut u8, size: usize) {
        let start = align_up(base as usize, GRANULE);
        let size = (size - (start - base as usize)) & !(GRANULE - 1);
        self.base = start;
        self.size = size;
        self.blocks[0] = Block { offset: 0, len: size, used: false };
        self.count = 1;
        self.used = 0;
        self.peak = 0;
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn peak(&self) -> usize {
        self.peak
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// The largest request that would succeed right now, ignoring alignment.
    pub fn largest_free(&self) -> usize {
        self.blocks[..self.count].iter().filter(|b| !b.used).map(|b| b.len).max().unwrap_or(0)
    }

    pub fn contains(&self, ptr: *mut u8) -> bool {
        let at = ptr as usize;
        self.size > 0 && at >= self.base && at < self.base + self.size
    }

    fn insert(&mut self, index: usize, block: Block) {
        self.blocks.copy_within(index..self.count, index + 1);
        self.blocks[index] = block;
        self.count += 1;
    }

    fn remove(&mut self, index: usize) {
        self.blocks.copy_within(index + 1..self.count, index);
        self.count -= 1;
    }

    /// Returns null if nothing fits, or if the pool has no room for more block records.
    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        let size = align_up(size.max(1), GRANULE);
        let align = align.max(GRANULE);
        // A split adds at most two records.
        if self.count + 2 > CAPACITY {
            return core::ptr::null_mut();
        }
        for index in 0..self.count {
            let block = self.blocks[index];
            if block.used {
                continue;
            }
            let address = self.base + block.offset;
            let padding = align_up(address, align) - address;
            if block.len < padding + size {
                continue;
            }
            let tail = block.len - padding - size;
            let mut at = index;
            if padding > 0 {
                self.blocks[at].len = padding;
                at += 1;
                self.insert(at, Block { offset: block.offset + padding, len: size, used: true });
            } else {
                self.blocks[at] = Block { offset: block.offset, len: size, used: true };
            }
            if tail > 0 {
                self.insert(at + 1, Block { offset: block.offset + padding + size, len: tail, used: false });
            }
            self.used += size;
            self.peak = self.peak.max(self.used);
            return (self.base + block.offset + padding) as *mut u8;
        }
        core::ptr::null_mut()
    }

    fn find(&self, ptr: *mut u8) -> Option<usize> {
        let offset = (ptr as usize).checked_sub(self.base)?;
        self.blocks[..self.count].binary_search_by_key(&offset, |b| b.offset).ok().filter(|&i| self.blocks[i].used)
    }

    /// Frees a block from [`Pool::alloc`], and merges it with free neighbors.
    pub fn free(&mut self, ptr: *mut u8) {
        let Some(mut index) = self.find(ptr) else { return };
        self.used -= self.blocks[index].len;
        self.blocks[index].used = false;
        if index + 1 < self.count && !self.blocks[index + 1].used {
            self.blocks[index].len += self.blocks[index + 1].len;
            self.remove(index + 1);
        }
        if index > 0 && !self.blocks[index - 1].used {
            index -= 1;
            self.blocks[index].len += self.blocks[index + 1].len;
            self.remove(index + 1);
        }
    }

    /// Grows a block without moving it, if the space after it is free. Returns whether it worked.
    pub fn grow_in_place(&mut self, ptr: *mut u8, new_size: usize) -> bool {
        let Some(index) = self.find(ptr) else { return false };
        let new_size = align_up(new_size, GRANULE);
        let current = self.blocks[index].len;
        if new_size <= current {
            return true;
        }
        let extra = new_size - current;
        if index + 1 >= self.count || self.blocks[index + 1].used || self.blocks[index + 1].len < extra {
            return false;
        }
        self.blocks[index].len = new_size;
        if self.blocks[index + 1].len == extra {
            self.remove(index + 1);
        } else {
            self.blocks[index + 1].offset += extra;
            self.blocks[index + 1].len -= extra;
        }
        self.used += extra;
        self.peak = self.peak.max(self.used);
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::boxed::Box;
    use std::vec;

    const SIZE: usize = 1024 * 1024;

    fn pool() -> (Box<Pool>, std::vec::Vec<u8>) {
        let mut memory = vec![0u8; SIZE];
        let mut pool = Box::new(Pool::empty());
        // SAFETY: The test keeps the memory alive and uses it only through the pool.
        unsafe { pool.init(memory.as_mut_ptr(), SIZE) };
        (pool, memory)
    }

    #[test]
    fn freed_blocks_merge_into_one_gap() {
        let (mut pool, _memory) = pool();
        let whole = pool.largest_free();
        let parts = [pool.alloc(1000, 16), pool.alloc(2000, 16), pool.alloc(3000, 16)];
        assert!(parts.iter().all(|p| !p.is_null()));
        pool.free(parts[1]);
        pool.free(parts[0]);
        pool.free(parts[2]);
        assert_eq!(pool.largest_free(), whole);
        assert_eq!(pool.used(), 0);
    }

    #[test]
    fn blocks_do_not_overlap_and_honor_alignment() {
        let (mut pool, _memory) = pool();
        let first = pool.alloc(100, 16) as usize;
        let second = pool.alloc(100, 4096) as usize;
        let third = pool.alloc(100, 16) as usize;
        assert_eq!(second % 4096, 0);
        assert!(first + 100 <= second);
        assert!(second + 100 <= third || third + 100 <= second);
    }

    #[test]
    fn grows_in_place_only_into_free_space() {
        let (mut pool, _memory) = pool();
        let first = pool.alloc(1000, 16);
        assert!(pool.grow_in_place(first, 5000));
        let second = pool.alloc(1000, 16);
        assert!(!pool.grow_in_place(first, 9000));
        pool.free(second);
        assert!(pool.grow_in_place(first, 9000));
    }

    #[test]
    fn reports_failure_when_full() {
        let (mut pool, _memory) = pool();
        assert!(!pool.alloc(SIZE / 2, 16).is_null());
        assert!(pool.alloc(SIZE, 16).is_null());
    }

    #[test]
    fn large_request_succeeds_after_churn() {
        let (mut pool, _memory) = pool();
        for round in 0..200 {
            let a = pool.alloc(1000 + round * 37, 16);
            let b = pool.alloc(3000, 16);
            pool.free(a);
            pool.free(b);
        }
        assert!(!pool.alloc(SIZE - 64, 16).is_null());
    }
}
