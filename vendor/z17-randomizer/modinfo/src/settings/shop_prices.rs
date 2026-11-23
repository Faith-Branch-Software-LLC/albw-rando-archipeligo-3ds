#[cfg(feature = "python")]
use pyo3::pyclass;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// What the shops charge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "python", cfg_eval, pyclass(eq, eq_int, hash, frozen))]
pub enum ShopPrices {
    /// The game's own prices.
    #[default]
    Vanilla,
    /// Each shop slot gets a random price.
    Random,
}

impl ShopPrices {
    pub fn is_vanilla(&self) -> bool {
        *self == Self::Vanilla
    }
}

// The vanilla choice adds nothing to the hash, so seeds made before this setting existed keep their
// seed hash.
impl Hash for ShopPrices {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.is_vanilla() {
            (*self as u8).hash(state);
        }
    }
}
