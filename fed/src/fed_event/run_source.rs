use with_structure::{MonostateStructure, WithStructure};
use schemars::JsonSchema;

pub trait RunSource {
    fn label() -> &'static str;
}

macro_rules! run_source {
    ($name:ident, $label:expr) => {
        #[derive(Debug, Clone, Copy, JsonSchema)]
        pub struct $name;
        impl RunSource for $name {
            fn label() -> &'static str {
                $label
            }
        }
        impl WithStructure for $name {
            type Structure = MonostateStructure;
            fn structure(&self) -> Self::Structure { MonostateStructure }
        }
    };
    ($name:ident) => {
        run_source!($name, stringify!($name));
    }
}

run_source!(Flyout, "Sacrifice");
run_source!(GroundOut, "Sacrifice");
run_source!(FieldersChoice, "Base Hit"); // this doesn't seem like the right label but ok
run_source!(Hit, "Base Hit");
run_source!(DoublePlay, "Double Play");
run_source!(StolenBase, "Steal Home"); // Probably needs to become non-Simple because of Blaserunning
run_source!(StrikeoutSwinging, "Strikeout Swinging");
run_source!(StrikeoutLooking, "Strikeout Looking");
run_source!(Walk, "Walk");
run_source!(MildPitch, "Mild Pitch");
run_source!(MildPitchWalk, "Mild Pitch Walk");
run_source!(CharmWalk, "Charm Walk");
run_source!(HomeRun, "Home Run");
run_source!(HomeRunSlamDunk, "Slam Dunk");
run_source!(MildPitchCharmWalk, "Mild Pitch Charm Walk");
run_source!(Flippers, "Flippers");
run_source!(HitByPitch, "Hit By Pitch");
run_source!(RunsOverflowing, "Runs Overflowing");
run_source!(MindTrickWalk, "Mind Trick Walk");
run_source!(CharmedMindTrickWalk, "Charmed Mind Trick Walk");
run_source!(DonatedShame, "Donated Shame");
run_source!(Moderation, "Moderation");
