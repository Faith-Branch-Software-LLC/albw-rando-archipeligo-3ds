//! Random shop prices.
//!
//! Every shop keeps its prices in its stage file, on the objects the player buys from. Ravio's
//! slots charge what their fourth argument says, and the other shops what their third says. The
//! game reads the price from there both to show it and to charge it.

use std::collections::BTreeMap;

use game::Course::{self, *};
use modinfo::settings::ShopPrices;
use rand::{rngs::StdRng, Rng, SeedableRng};

/// Which argument of a stand object holds its price.
#[derive(Clone, Copy, Debug)]
pub enum PriceArg {
    /// Ravio's slots.
    Fourth,
    /// The other shops.
    Third,
}

/// A place that sells something: its location name, and the stand object in its stage file. The
/// stage number counts from 1, as `Patcher::modify_objs` takes it.
pub struct Stand {
    pub name: &'static str,
    pub course: Course,
    pub stage: u16,
    pub unq: u16,
    pub arg: PriceArg,
}

const fn stand(name: &'static str, course: Course, stage: u16, unq: u16, arg: PriceArg) -> Stand {
    Stand { name, course, stage, unq, arg }
}

/// Every stand whose price can change. The Golden Bees, at 9999 Rupees, keep their price.
pub const STANDS: &[Stand] = &[
    stand("Ravio's Shop (1)", IndoorLight, 1, 13, PriceArg::Fourth),
    stand("Ravio's Shop (2)", IndoorLight, 1, 14, PriceArg::Fourth),
    stand("Ravio's Shop (3)", IndoorLight, 1, 15, PriceArg::Fourth),
    stand("Ravio's Shop (4)", IndoorLight, 1, 16, PriceArg::Fourth),
    stand("Ravio's Shop (5)", IndoorLight, 1, 17, PriceArg::Fourth),
    stand("Ravio's Shop (6)", IndoorLight, 1, 18, PriceArg::Fourth),
    stand("Ravio's Shop (7)", IndoorLight, 1, 19, PriceArg::Fourth),
    stand("Ravio's Shop (8)", IndoorLight, 1, 20, PriceArg::Fourth),
    stand("Ravio's Shop (9)", IndoorLight, 1, 21, PriceArg::Fourth),
    stand("Street Merchant (Left)", FieldLight, 16, 198, PriceArg::Third),
    stand("Street Merchant (Right)", FieldLight, 16, 199, PriceArg::Third),
    stand("Kakariko Item Shop (1)", IndoorLight, 8, 6, PriceArg::Third),
    stand("Kakariko Item Shop (2)", IndoorLight, 8, 7, PriceArg::Third),
    stand("Kakariko Item Shop (3)", IndoorLight, 8, 11, PriceArg::Third),
    stand("Lakeside Item Shop (1)", IndoorLight, 6, 6, PriceArg::Third),
    stand("Lakeside Item Shop (2)", IndoorLight, 6, 7, PriceArg::Third),
    stand("Lakeside Item Shop (3)", IndoorLight, 6, 11, PriceArg::Third),
    stand("Thieves' Town Item Shop (1)", IndoorDark, 13, 8, PriceArg::Third),
    stand("Thieves' Town Item Shop (3)", IndoorDark, 13, 10, PriceArg::Third),
    stand("Thieves' Town Item Shop (4)", IndoorDark, 13, 9, PriceArg::Third),
    stand("Lorule Lakeside Item Shop (1)", IndoorDark, 9, 6, PriceArg::Third),
    stand("Lorule Lakeside Item Shop (3)", IndoorDark, 9, 8, PriceArg::Third),
    stand("Lorule Lakeside Item Shop (4)", IndoorDark, 9, 5, PriceArg::Third),
];

/// The lowest and highest random price, and the step between prices.
pub const LOWEST: u16 = 10;
pub const HIGHEST: u16 = 300;
const STEP: u16 = 5;

// Logic does not count Rupees for these prices: Rupee items are placed after the logic is
// settled, and 300 Rupees is a short grind.

/// Picks a price for every stand. Uses its own random numbers, so the rest of the seed comes out
/// the same with or without random prices.
pub fn choose(seed: u32, setting: ShopPrices) -> BTreeMap<String, u16> {
    if setting.is_vanilla() {
        return BTreeMap::new();
    }
    // "SHOP" in ASCII.
    let rng = &mut StdRng::seed_from_u64(seed as u64 ^ 0x5348_4F50_0000_0000);
    STANDS
        .iter()
        .map(|stand| (stand.name.to_owned(), rng.gen_range(LOWEST / STEP..=HIGHEST / STEP) * STEP))
        .collect()
}
