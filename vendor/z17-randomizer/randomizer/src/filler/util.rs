use crate::Error;
use rand::{rngs::StdRng, Rng};
use std::collections::BTreeMap;
use std::hash::Hash;

/// A random index below `len`, drawn the same way on 32-bit and 64-bit builds.
///
/// `gen_range` on `usize` samples by pointer width, so a 3DS and a PC would make different seeds
/// from the same number. Archipelago patches made on a PC are rebuilt on the 3DS, so both must
/// agree. Drawing through `u64` gives what a 64-bit build always gave.
pub(crate) fn index<R: Rng + ?Sized>(rng: &mut R, len: usize) -> usize {
    rng.gen_range(0..len as u64) as usize
}

/// Shuffles elements of a Vec
pub(crate) fn shuffle<K>(rng: &mut StdRng, mut vec: Vec<K>) -> Vec<K> {
    if vec.len() < 2 {
        return vec;
    }

    let mut shuffled: Vec<K> = Vec::new();

    while !vec.is_empty() {
        shuffled.push(vec.remove(index(rng, vec.len())));
    }

    shuffled
}

/// Randomly pair together entries from a Vec and return the resultant Map.
pub(crate) fn pair_randomly<T>(rng: &mut StdRng, vec: Vec<T>) -> crate::Result<BTreeMap<T, T>>
where
    T: Eq + Hash + Clone + Ord,
{
    if vec.len() % 2 != 0 {
        return Err(Error::internal("Cannot pair entries in Vec with odd length."));
    }

    let shuffled = shuffle(rng, vec);
    let mut map: BTreeMap<T, T> = Default::default();

    let mut i = 0;
    while i < shuffled.len() {
        let x = shuffled[i].clone();
        let y = shuffled[i + 1].clone();

        map.insert(x.clone(), y.clone());
        map.insert(y, x);

        i += 2;
    }

    Ok(map)
}
