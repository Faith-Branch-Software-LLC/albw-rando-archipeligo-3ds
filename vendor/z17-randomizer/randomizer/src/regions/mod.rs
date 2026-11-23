use crate::{hints::hint_color::HintColor, patch::Patcher, SeedInfo};
use log::info;
use serde::Serialize;
use std::{
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
};

#[derive(Serialize)]
pub struct Subregion {
    name: &'static str,
    color: HintColor,
    world: World,
    id: &'static str,
}

impl Subregion {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn name_colorized(&self) -> String {
        self.color.format(self.name)
    }

    pub fn world(&self) -> World {
        self.world
    }
}

impl Debug for Subregion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subregion").field("name", &self.name).field("world", &self.world).field("id", &self.id).finish()
    }
}

impl Eq for Subregion {}

impl PartialEq for Subregion {
    fn eq(&self, other: &Self) -> bool {
        self.world == other.world && self.name == other.name && self.id == other.id
    }
}

impl Hash for Subregion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.world.hash(state);
        self.name.hash(state);
        self.id.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
pub enum World {
    Hyrule,
    Lorule,
    Dungeons,
}

pub mod dungeons {
    pub const WORLD: super::World = super::World::Dungeons;
    pub mod dark;
    pub mod desert;
    pub mod eastern;
    pub mod house;
    pub mod hyrule;
    pub mod ice;
    pub mod lorule;
    pub mod skull;
    pub mod swamp;
    pub mod thieves;
    pub mod tower;
    pub mod turtle;
}
pub mod hyrule {
    pub const WORLD: super::World = super::World::Hyrule;
    pub mod death;
    pub mod desert;
    pub mod eastern;
    pub mod field;
    pub mod irene;
    pub mod kakariko;
    pub mod lake;
    pub mod lost;
    pub mod ravio;
    pub mod river;
    pub mod southern;
}
pub mod lorule {
    pub const WORLD: super::World = super::World::Lorule;
    pub mod dark;
    pub mod death;
    pub mod field;
    pub mod graveyard;
    pub mod lake;
    pub mod misery;
    pub mod skull;
}

/// A location as its region file lists it: subregion, the region's course, name, and patch.
pub type RegionLocation = (&'static Subregion, ::game::Course, &'static str, &'static str);

/// Every location in every region file.
pub fn locations() -> Vec<RegionLocation> {
    let lists: [fn() -> Vec<RegionLocation>; 30] = [
        dungeons::dark::locations,
        dungeons::desert::locations,
        dungeons::eastern::locations,
        dungeons::house::locations,
        dungeons::hyrule::locations,
        dungeons::ice::locations,
        dungeons::lorule::locations,
        dungeons::skull::locations,
        dungeons::swamp::locations,
        dungeons::thieves::locations,
        dungeons::tower::locations,
        dungeons::turtle::locations,
        hyrule::death::locations,
        hyrule::desert::locations,
        hyrule::eastern::locations,
        hyrule::field::locations,
        hyrule::irene::locations,
        hyrule::kakariko::locations,
        hyrule::lake::locations,
        hyrule::lost::locations,
        hyrule::ravio::locations,
        hyrule::river::locations,
        hyrule::southern::locations,
        lorule::dark::locations,
        lorule::death::locations,
        lorule::field::locations,
        lorule::graveyard::locations,
        lorule::lake::locations,
        lorule::misery::locations,
        lorule::skull::locations,
    ];
    lists.iter().flat_map(|list| list()).collect()
}

impl Subregion {
    /// The region's short id within its file, such as "main".
    pub fn id(&self) -> &'static str {
        self.id
    }
}

pub(crate) fn patch(patcher: &mut Patcher, seed_info: &SeedInfo) -> crate::Result<()> {
    info!("Patching Randomized Checks...");

    // todo unravel this
    let steps: [fn(&mut Patcher, &SeedInfo) -> crate::Result<()>; 30] = [
        dungeons::dark::patch,
        dungeons::desert::patch,
        dungeons::eastern::patch,
        dungeons::house::patch,
        dungeons::hyrule::patch,
        dungeons::ice::patch,
        dungeons::lorule::patch,
        dungeons::skull::patch,
        dungeons::swamp::patch,
        dungeons::thieves::patch,
        dungeons::tower::patch,
        dungeons::turtle::patch,
        hyrule::death::patch,
        hyrule::desert::patch,
        hyrule::eastern::patch,
        hyrule::field::patch,
        hyrule::irene::patch,
        hyrule::kakariko::patch,
        hyrule::lake::patch,
        hyrule::lost::patch,
        hyrule::ravio::patch,
        hyrule::river::patch,
        hyrule::southern::patch,
        lorule::dark::patch,
        lorule::death::patch,
        lorule::field::patch,
        lorule::graveyard::patch,
        lorule::lake::patch,
        lorule::misery::patch,
        lorule::skull::patch,
    ];
    for (done, step) in steps.into_iter().enumerate() {
        crate::progress::report("Patching regions", done as u32, steps.len() as u32);
        step(patcher, seed_info)?;
    }
    Ok(())
}

#[doc(hidden)]
#[macro_export]
macro_rules! region {
    (
        course: $course:ident,
        name: $name:literal,
        color: $color:ident,
        $start:ident $start_props:tt,
        $($id:ident $props:tt,)*
    ) => {

        #[inline]
        pub fn patch(patcher: &mut $crate::patch::Patcher, seed_info: &$crate::SeedInfo) -> $crate::Result<()> {
            $start::patch(patcher, seed_info)?;
            $($id::patch(patcher, seed_info)?;)*
            Ok(())
        }

        $crate::subregion!($start $start_props);
        $($crate::subregion!($id $props);)*

        /// Every location in this region, with its subregion and how it is patched.
        #[allow(unused)]
        pub fn locations() -> Vec<$crate::regions::RegionLocation> {
            let mut all: Vec<$crate::regions::RegionLocation> = Vec::new();
            all.extend($start::LOCATIONS.iter().map(|&(name, patch)| ($start::SUBREGION, COURSE, name, patch)));
            $(all.extend($id::LOCATIONS.iter().map(|&(name, patch)| ($id::SUBREGION, COURSE, name, patch)));)*
            all
        }

        pub const NAME: &str = $name;
        pub const COLOR: $crate::hints::hint_color::HintColor = $crate::hints::hint_color::HintColor::$color;
        #[allow(unused)]
        pub const COURSE: ::game::Course = ::game::Course::$course;
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! subregion {
    ($id:ident {
        $(locations: [
            $($key:literal: $item:ident @$variant:ident $props:tt $(:- $condition:tt)?
                $(where $settings:ident: $where:expr)?,)*
        ],)?
        $(paths: [
            $($path:ident$(::$path_rest:ident)* $(:- $pcondition:tt)?,)*
        ],)?
        $(quest: $kind:ident$(::$qvariant:ident)?,)?
    }) => {
        pub mod $id {

            use $crate::{patch::Patcher, regions::Subregion};

            pub use super::COURSE;

            /// Each location's name and its patch, as written in the region file.
            #[allow(unused)]
            pub const LOCATIONS: &[(&str, &str)] = &[$($(($key, stringify!($variant $props)),)*)?];

            pub const SUBREGION: &Subregion = &Subregion {
                name: super::NAME,
                color: super::COLOR,
                world: super::super::WORLD,
                id: stringify!($id),
            };

            #[allow(unused)]
            #[inline]
            pub fn patch(patcher: &mut Patcher, seed_info: &$crate::SeedInfo) -> $crate::Result<()> {
                $(use $crate::patch::Patch;
                $($crate::patch!($variant $props).apply(
                    patcher,
                    seed_info,
                    seed_info.layout
                        .get($key, SUBREGION),
                    $key,
                )?;)*)?
                Ok(())
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! patch {
    (Chest($course:ident $stage:literal[$unq:literal])) => {
        Patch::Chest { course: ::game::Course::$course, stage: $stage - 1, unq: $unq }
    };
    (Chest($stage:literal[$unq:literal])) => {
        Patch::Chest { course: COURSE, stage: $stage - 1, unq: $unq }
    };
    (Chest[$($stage:literal[$unq:literal],)+]) => {
        Patch::Multi(vec![
            $(
                Patch::Chest { course: COURSE, stage: $stage - 1, unq: $unq },
            )+
        ])
    };
    (BigChest($course:ident $stage:literal[$unq:literal])) => {
        Patch::BigChest { course: ::game::Course::$course, stage: $stage - 1, unq: $unq }
    };
    (BigChest($stage:literal[$unq:literal])) => {
        Patch::BigChest { course: COURSE, stage: $stage - 1, unq: $unq }
    };
    (Event($name:ident[$index:literal])) => {
        Patch::Event { course: Some(COURSE), name: stringify!($name), index: $index }
    };
    (Event(Boot/$name:ident[$index:literal])) => {
        Patch::Event { course: None, name: stringify!($name), index: $index }
    };
    (Event($course:ident/$name:ident[$index:literal])) => {
        Patch::Event { course: Some(::game::Course::$course), name: stringify!($name), index: $index }
    };
    (Event[$($name:ident[$index:literal],)+]) => {
        Patch::Multi(vec![
            $(
                Patch::Event { course: Some(COURSE), name: stringify!($name), index: $index },
            )+
        ])
    };
    (Heart($course:ident $scene:literal[$unq:literal])) => {
        Patch::Heart { course: ::game::Course::$course, scene: $scene - 1, unq: $unq }
    };
    (Heart($scene:literal[$unq:literal])) => {
        Patch::Heart { course: COURSE, scene: $scene - 1, unq: $unq }
    };
    (Key($course:ident $scene:literal[$unq:literal])) => {
        Patch::Key { course: ::game::Course::$course, scene: $scene - 1, unq: $unq }
    };
    (Key($scene:literal[$unq:literal])) => {
        Patch::Key { course: COURSE, scene: $scene - 1, unq: $unq }
    };
    (Maiamai($course:ident $scene:literal[$unq:literal])) => {
        Patch::Maiamai { course: ::game::Course::$course, scene: $scene - 1, unq: $unq }
    };
    (Maiamai($scene:literal[$unq:literal])) => {
        Patch::Maiamai { course: COURSE, scene: $scene - 1, unq: $unq }
    };
    (SilverRupee($course:ident $scene:literal[$unq:literal])) => {
        Patch::SilverRupee { course: ::game::Course::$course, scene: $scene - 1, unq: $unq }
    };
    (SilverRupee($scene:literal[$unq:literal])) => {
        Patch::SilverRupee { course: COURSE, scene: $scene - 1, unq: $unq }
    };
    (GoldRupee($course:ident $scene:literal[$unq:literal])) => {
        Patch::GoldRupee { course: ::game::Course::$course, scene: $scene - 1, unq: $unq }
    };
    (GoldRupee($scene:literal[$unq:literal])) => {
        Patch::GoldRupee { course: COURSE, scene: $scene - 1, unq: $unq }
    };
    (Crack($course:ident $scene:literal[$unq:literal] $crack:ident)) => {
        Patch::Crack { course: ::game::Course::$course, scene: $scene - 1, unq: $unq, crack: $crate::Crack::$crack }
    };
    (Crack($scene:literal[$unq:literal] $crack:ident)) => {
        Patch::Crack { course: COURSE, scene: $scene - 1, unq: $unq, crack: $crate::Crack::$crack }
    };
    (WeatherVane($course:ident $scene:literal[$unq:literal] $vane:ident)) => {
        Patch::WeatherVane { course: ::game::Course::$course, scene: $scene - 1, unq: $unq, vane: $crate::filler::filler_item::Vane::$vane }
    };
    (WeatherVane($scene:literal[$unq:literal] $vane:ident)) => {
        Patch::WeatherVane { course: COURSE, scene: $scene - 1, unq: $unq, vane: $crate::filler::filler_item::Vane::$vane }
    };
    (Shop($variant:ident$($args:tt)?)) => {
        Patch::Shop($crate::patch::Shop::$variant $($args)?)
    };
    (None()) => {
        Patch::None
    }
}
