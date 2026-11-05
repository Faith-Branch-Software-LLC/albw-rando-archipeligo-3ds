#[cfg(feature = "python")]
use pyo3::pyclass;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "python", cfg_eval, pyclass(eq, eq_int, hash, frozen))]
pub enum HintGhosts {
    Off,
    #[default]
    Always,
    Glasses,
}
