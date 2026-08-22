use serde::Serialize;
use schemars::JsonSchema;
use with_structure::{MonostateStructure, WithStructure};

pub trait RunSource {
    fn label() -> &'static str;
}

macro_rules! run_source {
    ($name:ident, $label:expr_2021) => {
        #[derive(Debug, Clone, Copy, JsonSchema, Serialize)]
        pub struct $name;
        impl RunSource for $name {
            fn label() -> &'static str {
                $label
            }
        }
        impl WithStructure for $name {
            type Structure = MonostateStructure;
            fn structure(&self) -> Self::Structure {
                MonostateStructure
            }
        }
    };
    ($name:ident) => {
        run_source!($name, stringify!($name));
    };
}

run_source!(Flyout, "Sacrifice");
run_source!(GroundOut, "Sacrifice");
run_source!(FieldersChoice, "Base Hit");
run_source!(Hit, "Base Hit");
run_source!(DoublePlay, "Base Hit");
run_source!(Walk, "Walk");
run_source!(MildPitch, "Base Hit");
run_source!(MildPitchWalk, "Mild Pitch Walk");
run_source!(CharmWalk, "Walk");
run_source!(HomeRun, "Home Run");
run_source!(HomeRunSlamDunk, "Slam Dunk");
run_source!(HomeRunBigBucket, "Big Bucket");
run_source!(Flippers, "Flippers");
run_source!(HitByPitch, "Hit By Pitch");
run_source!(CharmedMindTrickWalk, "Charmed Mind Trick Walk");
run_source!(DonatedShame, "Donated Shame");
