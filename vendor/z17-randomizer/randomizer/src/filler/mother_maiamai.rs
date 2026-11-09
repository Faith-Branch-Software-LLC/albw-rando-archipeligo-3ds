use modinfo::Settings;
use rand::rngs::StdRng;

pub fn choose_mother_maiamai_costs(settings: &Settings, rng: &mut StdRng) -> Vec<u8> {
    (0..9).map(|_| crate::filler::util::index(rng, settings.maiamai_limit + 1) as u8).collect()
}