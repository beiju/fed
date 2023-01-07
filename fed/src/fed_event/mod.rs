mod fed_event_impl;

pub use fed_event_impl::*;

use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Write};
use chrono::{DateTime, Utc};
use enum_access::EnumDisplay;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use eventually_api::{EventMetadata, Weather};
use num_enum::{IntoPrimitive, TryFromPrimitive, TryFromPrimitiveError};
use derive_builder::Builder;
use schemars::JsonSchema;
use strum_macros::AsRefStr;
use with_structure::WithStructure;
use with_structure_derive::WithStructure;
use enum_flatten_derive::{EnumFlatten, EnumFlattenable};

use crate::FeedParseError;
use crate::parse::builder::possessive;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, IntoPrimitive, TryFromPrimitive)]
#[repr(i32)]
pub enum Being {
    EmergencyAlert = -1,
    TheShelledOne = 0,
    TheMonitor = 1,
    TheCoin = 2,
    TheReader = 3,
    TheMicrophone = 4,
    Lootcrates = 5,
    Namerifeht = 6,
}
/// Game data. Every game event has one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    /// Game uuid
    pub game_id: Uuid,

    /// Home team's uuid
    pub home_team: Uuid,

    /// Away team's uuid
    pub away_team: Uuid,

    /// The play that this event came from. This number is always one lower than the playCount
    /// field in the corresponding game update.
    pub play: i64,

    /// If a player got unscattered this tick, contains information about their unscattering.
    pub unscatter: Option<ModChangeSubEventWithNamedPlayer>,

    /// If an Attractor entered the Secret Base on this tick, contains information about this player
    pub attractor_secret_base: Option<PlayerNameId>,
}

/// Pitch data. The normal-baseball game events all have one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GamePitch {
    /// If a Double Strike was fired, the name of the pitcher who fired it. Otherwise null.
    pub double_strike: Option<String>,

    /// If an Acidic pitch was thrown, the name of the pitcher who threw it. Otherwise null.
    pub acidic_pitch: Option<String>,
}

// This contains only the event properties that will differ from the parent, including id, created,
// and nuts; but not properties that will be the same, like day, season, and tournament.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct SubEvent {
    /// Uuid of sub-event
    pub id: Uuid,

    /// Date the sub-event was created. This should be very close to the date the parent event was
    /// created, but will typically not be exactly the same.
    pub created: DateTime<Utc>,

    /// Number of upshells this event has received
    pub nuts: i32,
}

impl SubEvent {
    // For use when you are generating Fed events and don't care about the SubEvent data
    pub fn nil() -> Self {
        Self {
            id: Uuid::nil(),
            created: DateTime::default(),
            nuts: 0,
        }
    }
}

// I am doing this crime because i want to compare measured events to generated events and i don't
// care about the non-generatable data. i am sure this will bite me in the ass eventually
impl PartialEq for SubEvent {
    fn eq(&self, other: &Self) -> bool {
        true
    }
}

// TODO Consolidate with ModChangeSubEventWithNamedPlayer
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FreeRefill {
    /// Metadata for the sub-event associated with losing the Free Refill mod
    pub sub_event: SubEvent,

    /// Name of the player who used their Free Refill. This may be the batter, a scoring runner, or
    /// in rare cases, the pitcher.
    pub player_name: String,

    /// Uuid of the player who used their Free Refill
    pub player_id: Uuid,

    /// Uuid of the team of the player who used their Free Refill. This is usually populated, but
    /// when a ghost who died before player objects stored team ids uses their free refill it's null
    pub team_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoringPlayer {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,

    /// Item damaged by player scoring, if any
    pub item_damage: Option<ItemDamaged>,

    /// Info about the player attracted by this score, if any
    pub attraction: Option<Attraction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    /// Info for all the scores that happened on this event
    pub scores: Vec<ScoringPlayer>,

    /// List of free refills used on this event, if any. This should always be empty if `scores` is
    /// empty, but if `scores` is non-empty it may be larger than `scores`.
    ///
    /// It's almost possible to attribute each one to the specific score that caused it, but not
    /// quite because FlyOut events don't have pitcher and batter uuids.
    pub free_refills: Vec<FreeRefill>,
}

impl Scores {
    #[deprecated = "This is part of the old event builder"]
    pub fn to_description_with_text_between(&self, score_text: &str, text_between: &str, extra_space: bool) -> String {
        let mut output = String::new();
        for score in &self.scores {
            if let Some(damage) = &score.item_damage {
                write!(output, "\n{}{} {} {}", if extra_space { " " } else { "" },
                       possessive(score.player_name.clone()), damage.item_name,
                       if damage.health == 0 { "broke!" } else { "was damaged." }).unwrap();
            }

            write!(output, "\n{}{}", score.player_name, score_text).unwrap();

            if let Some(attraction) = &score.attraction {
                write!(output, "\nThe {} Attract {}!", attraction.team_nickname, score.player_name).unwrap();
            }
        }

        write!(output, "{}", text_between).unwrap();

        for refill in &self.free_refills {
            write!(output, "\n{} used their Free Refill.\n{} Refills the In!", refill.player_name, refill.player_name).unwrap();
        }

        output
    }

    pub fn scorer_ids(&self) -> Vec<Uuid> {
        self.scores.iter()
            .map(|p| p.player_id)
            .collect()
    }

    pub fn used_refill(&self) -> bool {
        !self.free_refills.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    /// Info for the score that happened on this event, if any, otherwise null
    pub score: Option<ScoringPlayer>,

    /// List of free refills used on this event, if any. This should always be empty if `score` is
    /// null, but if `scores` is non-null it may contain more than one element.
    pub free_refills: Vec<FreeRefill>,
}

impl Score {
    #[deprecated = "This is part of the old event builder"]
    pub fn to_description_with_text_between(&self, score_text: &str, text_between: &str) -> String {
        let mut output = String::new();
        if let Some(score) = &self.score {
            write!(output, "\n{}{}", score.player_name, score_text).unwrap();
        }

        write!(output, "{}", text_between).unwrap();

        for refill in &self.free_refills {
            write!(output, "\n{} used their Free Refill.\n{} Refills the In!", refill.player_name, refill.player_name).unwrap();
        }

        output
    }

    pub fn scorer_ids(&self) -> Vec<Uuid> {
        self.score.iter()
            .map(|p| p.player_id)
            .collect()
    }

    pub fn used_refill(&self) -> bool {
        !self.free_refills.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Inhabiting {
    /// Metadata for the sub-event associated with adding the Inhabiting modifier. If the player
    /// already has the Inhabiting modifier, this will be null. (That only happens 14 times in all
    /// of Expansion.)
    pub sub_event: Option<SubEvent>,

    /// The name of the player who's being inhabited
    pub inhabited_player_name: String,

    /// The uuid of the player who's being inhabited
    pub inhabited_player_id: Uuid,

    /// The uuid of the player who's inhabiting
    pub inhabiting_player_id: Uuid,

    /// The last known team uuid of the player who's inhabiting, if known.
    ///
    /// The game didn't start saving last known team ids until somewhere around the Coffee Cup
    pub inhabiting_player_team_id: Option<Uuid>,
}

// TODO: Have a variant of this where the player name and id are inferred from the batter's
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoppedInhabiting {
    /// Sub-event associated with losing the Inhabiting mod
    pub sub_event: SubEvent,

    /// Name of inhabiting player
    pub inhabiting_player_name: String,

    /// Uuid of inhabiting player
    pub inhabiting_player_id: Uuid,

    /// The last known team uuid of the player who's inhabiting, if known.
    ///
    /// The game didn't start saving last known team ids until somewhere around the Coffee Cup
    pub inhabiting_player_team_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum CoffeeBeanMod {
    Wired,
    Tired,
}

impl CoffeeBeanMod {
    fn to_str(&self) -> &'static str {
        match self {
            CoffeeBeanMod::Wired => { "WIRED" }
            CoffeeBeanMod::Tired => { "TIRED" }
        }
    }
}

impl TryFrom<&str> for CoffeeBeanMod {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "WIRED" => Ok(Self::Wired),
            "TIRED" => Ok(Self::Tired),
            _ => Err(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, IntoPrimitive, TryFromPrimitive)]
#[serde(rename_all = "camelCase")]
#[repr(i64)]
pub enum AttrCategory {
    Batting = 0,
    Pitching = 1,
    Defense = 2,
    Baserunning = 3,
}

impl Display for AttrCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AttrCategory::Batting => { write!(f, "hitting") }
            AttrCategory::Pitching => { write!(f, "pitching") }
            AttrCategory::Defense => { write!(f, "defensive") }
            AttrCategory::Baserunning => { write!(f, "baserunning") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "action", content = "strikeoutBatterName")]
pub enum BlooddrainAction {
    AddBall,
    RemoveBall,
    AddStrike(Option<String>),
    // if there's a strikeout looking, there's a name here
    RemoveStrike,
    AddOut,
    RemoveOut,
}

impl Display for BlooddrainAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlooddrainAction::AddBall => { write!(f, "adds a Ball!") }
            BlooddrainAction::RemoveBall => { write!(f, "removes a Ball!") }
            BlooddrainAction::AddStrike(None) => { write!(f, "adds a Strike!") }
            BlooddrainAction::AddStrike(Some(player_struck_out_name)) => {
                write!(f, "adds a Strike!\n{player_struck_out_name} strikes out looking.")
            }
            BlooddrainAction::RemoveStrike => { write!(f, "removes a Strike!") }
            BlooddrainAction::AddOut => { write!(f, "adds a Out!") }
            BlooddrainAction::RemoveOut => { write!(f, "removes a Out!") }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
#[serde(rename_all = "camelCase")]
pub enum ModDuration {
    Permanent = 0,
    Seasonal = 1,
    Weekly = 2,
    Game = 3,
}

impl Display for ModDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ModDuration::Permanent => { write!(f, "permanent") }
            ModDuration::Seasonal => { write!(f, "seasonal") }
            ModDuration::Weekly => { write!(f, "weekly") }
            ModDuration::Game => { write!(f, "game") }
        }
    }
}

// Struct that bundles metadata necessary to reconstruct a ModAdded/ModChanged/ModRemoved event.
// Which of those it is will come from context. If the od of the player is not present in the
// containing event, use ModChangeSubEventWithPlayer or ModChangeSubEventWithNamedPlayer instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModChangeSubEvent {
    /// Metadata for the sub-event associated with the mod change
    pub sub_event: SubEvent,

    /// Uuid of the team whose player's mod changed
    pub team_id: Uuid,
}

// Struct that bundles metadata necessary to reconstruct a ModAdded/ModChanged/ModRemoved event.
// Which of those it is will come from context. If the name of the player is not present in the
// containing event, use ModChangeSubEventWithNamedPlayer instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModChangeSubEventWithPlayer {
    /// Metadata for the sub-event associated with the mod change
    pub sub_event: SubEvent,

    /// Uuid of the team whose player's mod changed
    pub team_id: Uuid,

    /// Uuid of the player whose mod changed
    pub player_id: Uuid,
}

// Struct that bundles metadata necessary to reconstruct a ModAdded/ModChanged/ModRemoved event.
// Which of those it is will come from context. If the name of the player is present in the
// containing event, use ModChangeSubEventWithPlayer instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModChangeSubEventWithNamedPlayer {
    /// Metadata for the sub-event associated with the mod change
    pub sub_event: SubEvent,

    /// Uuid of the team whose player's mod changed
    pub team_id: Uuid,

    /// Uuid of the player whose mod changed
    pub player_id: Uuid,

    /// Name of the player whose mod changed
    pub player_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SpicyStatus {
    /// Nothing Spicy-related is happening
    None,

    /// The batter is Heating Up
    HeatingUp,

    /// The batter is Red Hot. Sometimes this has a sub-event with metadata about the mod change.
    /// I haven't determined what causes the difference. If anyone else knows, I would appreciate an
    /// explanation (ideally with evidence), in the github issues or to beiju#9630 in SIBR.
    RedHot(Option<ModChangeSubEvent>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamPerformingChanged {
    /// Nickname of the team who gained or lost Over or Underperforming
    pub team_nickname: String,

    /// Uuid of the team who gained or lost Over or Underperforming
    pub team_id: Uuid,

    /// Internal ID of the mod which caused the addition or removal. Which mod was added or removed
    /// is not stored, but is inferred from this ID.
    // TODO: Make this an enum?
    pub source_mod_id: String,

    /// Name of the mod which caused the addition or removal
    pub source_mod_name: String,

    /// True if the mod was added, false if it was removed
    pub was_added: bool,

    /// Metadata for the sub-event associated with the mod change
    pub sub_event: SubEvent,
}

impl SpicyStatus {
    pub fn is_none(&self) -> bool {
        match self {
            SpicyStatus::None => true,
            _ => false
        }
    }
    pub fn is_special(&self) -> bool {
        match self {
            SpicyStatus::RedHot { .. } => true,
            _ => false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatChange {
    /// Team uuid of player whose stats changed
    pub team_id: Uuid,

    /// Uuid of player whose stats changed
    pub player_id: Uuid,

    /// Name of player whose stats changed
    pub player_name: String,

    /// Player's rating before the stats changed. The rating category is stored externally. Rating
    /// is equivalent to stars but is on an 0-1 scale rather than an 0-5 scale.
    pub rating_before: f64,

    /// Player's rating after the stats changed
    pub rating_after: f64,

    /// Metadata for the sub-event associated with the player stat change event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
#[serde(rename_all = "camelCase")]
pub enum ActivePositionType {
    Lineup = 0,
    Rotation = 1,
}

impl ActivePositionType {
    pub fn location(&self) -> &'static str {
        match self {
            ActivePositionType::Lineup => "lineup",
            ActivePositionType::Rotation => "rotation",
        }
    }

    pub fn role(&self) -> &'static str {
        match self {
            ActivePositionType::Lineup => "batting",
            ActivePositionType::Rotation => "pitching",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
#[serde(rename_all = "camelCase")]
pub enum ShadowPositionType {
    Bench = 2,
    Bullpen = 3,
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive, WithStructure)]
#[repr(i64)]
#[serde(rename_all = "camelCase")]
pub enum PositionType {
    Lineup = 0,
    Rotation = 1,
    Bench = 2,
    Bullpen = 3,
}

impl From<TryFromPrimitiveError<ActivePositionType>> for FeedParseError {
    fn from(value: TryFromPrimitiveError<ActivePositionType>) -> Self {
        FeedParseError::InvalidLocation {
            expected: &[1, 2],
            actual: value.number,
        }
    }
}

// TODO doc comments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPlayerData {
    pub team_id: Uuid,
    pub team_nickname: String,
    pub player_id: Uuid,
    pub player_name: String,
    pub location: ActivePositionType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlayerReverb {
    /// There is a repeated Uuid in playerTags at this position. This is the only indication that,
    /// presumably, the sim rolled to swap a player with themselves.
    RepeatId(Uuid),

    /// Normal reverb effect, two players are swapped
    Swap {
        /// Uuid of the first player involved in this reverb
        first_player_id: Uuid,

        /// Name of the first player involved in this reverb
        first_player_name: String,

        /// New location (lineup or rotation) of the first player involved in this reverb. Also the 
        /// previous location of the second player in the reverb.
        first_player_new_location: ActivePositionType,

        /// Uuid of the second player involved in this reverb
        second_player_id: Uuid,

        /// Name of the second player involved in this reverb
        second_player_name: String,

        /// New location (lineup or rotation) of the second player involved in this reverb. Also the 
        /// previous location of the second player in the reverb.
        second_player_new_location: ActivePositionType,

        /// Metadata associated with the player swap sub-event
        sub_event: SubEvent,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
// This uses a combo of flatten and adjacent tagging
#[serde(rename_all = "camelCase", tag = "type", content = "subEvent")]
pub enum ReverbType {
    Rotation(SubEvent),
    Lineup(SubEvent),
    Full(SubEvent),
    SeveralPlayers(Vec<PlayerReverb>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BatterSkippedReason {
    /// Batter is Shelled
    Shelled,

    /// Batter is Elsewhere
    ///
    /// For whatever reason, this has a player_id while the Shelled variant does not
    Elsewhere(Uuid),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(i64)]
pub enum StatChangeCategory {
    Batting = 0,
    Pitching = 1,
    Baserunning = 2,
    Defense = 3,
    All = 4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerNameId {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,
}

// This is identical to PlayerInfo except for field names. It's used for JSON schema reasons
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PitcherNameId {
    /// Pitcher uuid
    pub pitcher_id: Uuid,

    /// Pitcher name
    pub pitcher_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Scattered {
    /// Name of player after being Scattered
    pub scattered_name: String,

    /// Sub-event associated with adding the Scattered mod
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FloodingSweptEffect {
    Elsewhere(ModChangeSubEventWithNamedPlayer),
    Flippers(PlayerNameId),
    Ego(PlayerNameId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, rename_all = "camelCase")]
pub enum RenovationVotes {
    Normal(i64),
    Manual(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MultipleModsAddedOrRemoved {
    /// Vector of mods that were added/removed. Each mod is represented by its internal ID.
    pub mod_ids: Vec<String>,

    /// Metadata for the event associated with adding or removing these mods
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Echo {
    /// Team Uuid of player who received the Echo.
    pub receiver_team_id: Uuid,

    /// Uuid of player who received the Echo
    pub receiver_id: Uuid,

    /// Name of player who received the Echo
    pub receiver_name: String,

    /// Mods that Faded as a result of this Echo, if any
    pub mods_removed: Option<MultipleModsAddedOrRemoved>,

    /// Mods that were added as a result of this Echo
    pub mods_added: MultipleModsAddedOrRemoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EchoIntoStatic {
    /// Team Uuid of player who echoed into static
    pub team_id: Uuid,

    /// Team nickname of player who echoed into static
    pub team_nickname: String,

    /// Uuid of player who echoed into static
    pub player_id: Uuid,

    /// Name of player who echoed into static
    pub player_name: String,

    /// Metadata for the event associated with removing the player from the team
    pub removed_from_team_sub_event: SubEvent,

    /// Metadata for the event associated with changing the Echo mod to the Static mod
    pub mod_changed_sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr)]
#[serde(tag = "time_elsewhere_type", content = "time_elsewhere", rename_all = "camelCase")]
pub enum TimeElsewhere {
    Days(i32),
    Seasons(i32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr)]
#[serde(tag = "flavor", rename_all = "camelCase")]
pub enum ReturnFromElsewhereFlavor {
    /// The normal one
    #[serde(rename_all = "camelCase")]
    Full {
        /// Team uuid of player who returned from Elsewhere
        team_id: Uuid,

        /// Uuid of player who returned from Elsewhere
        player_id: Uuid,

        /// True if the player is trapped in a giant peanut shell, false otherwise
        // TODO: Move this outside the enum?
        is_peanut: bool,

        /// Metadata for sub-event associated with removing the Elsewhere mod
        sub_event: SubEvent,

        /// Number of days or seasons the player was Elsewhere
        time_elsewhere: TimeElsewhere,

        /// Scattered sub-event, if the player was scattered, or null otherwise
        scattered: Option<Scattered>,

        /// "Re-congealed differently" sub-event, if player re-congealed differently, or null
        /// otherwise
        recongealed_differently: Option<PlayerStatChange>,
    },
    /// The short one that happens when the player went Elsewhere via salmon cannons or fleeing a
    /// failed heist. Players can't get Scattered on this one.
    #[serde(rename_all = "camelCase")]
    Short {
        /// Team uuid of player who returned from Elsewhere
        team_id: Uuid,

        /// Uuid of player who returned from Elsewhere
        player_id: Uuid,

        /// True if the player is trapped in a giant peanut shell, false otherwise
        is_peanut: bool,

        /// Metadata for sub-event associated with removing the Elsewhere mod
        sub_event: SubEvent,
    },
    /// Fake returns from elsewhere. As far as I know this only happens when a Receiver returns from
    /// Elsewhere after being sent there by Receiving Elsewhere from an Echo. There's no metadata
    /// on a false return from elsewhere.
    False {
        /// True if the player is trapped in a giant peanut shell, false otherwise
        is_peanut: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamRunsLost {
    /// Number of runs lost
    pub runs_lost: f32,

    /// Name of team who lost the runs
    pub team_name: String,
}

impl Display for TeamRunsLost {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} of the {}'s Runs are lost!", self.runs_lost, self.team_name)
    }
}

// TODO: Make this into a static vec with max size 2 (third-party crate)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr)]
#[serde(into = "SerdeRunLossesFromSalmon", try_from = "SerdeRunLossesFromSalmon")]
pub enum RunLossesFromSalmon {
    None,
    OneTeam(TeamRunsLost),
    BothTeams((TeamRunsLost, TeamRunsLost)),
}

#[derive(Serialize, Deserialize)]
struct SerdeRunLossesFromSalmon(Vec<TeamRunsLost>);

impl TryFrom<SerdeRunLossesFromSalmon> for RunLossesFromSalmon {
    type Error = String;

    fn try_from(value: SerdeRunLossesFromSalmon) -> Result<Self, Self::Error> {
        Ok(match value.0.len() {
            0 => { Self::None }
            1 => { Self::OneTeam(value.0.into_iter().next().unwrap()) }
            2 => { Self::BothTeams(value.0.into_iter().collect_tuple().unwrap()) }
            n => { return Err(format!("RunLossesFromSalmon must have 0, 1, or 2 elements but got {} elements", n)); }
        })
    }
}

impl Into<SerdeRunLossesFromSalmon> for RunLossesFromSalmon {
    fn into(self) -> SerdeRunLossesFromSalmon {
        match self {
            RunLossesFromSalmon::None => { SerdeRunLossesFromSalmon(vec![]) }
            RunLossesFromSalmon::OneTeam(one) => { SerdeRunLossesFromSalmon(vec![one]) }
            RunLossesFromSalmon::BothTeams((a, b)) => { SerdeRunLossesFromSalmon(vec![a, b]) }
        }
    }
}


impl Display for RunLossesFromSalmon {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RunLossesFromSalmon::None => { write!(f, "No Runs are lost.") }
            RunLossesFromSalmon::OneTeam(runs) => { write!(f, "{runs}") }
            RunLossesFromSalmon::BothTeams((a, b)) => { write!(f, "{a}\n{b}") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetectiveActivity {
    /// Uuid of the detective
    pub detective_id: Uuid,

    /// Name of the detective
    pub detective_name: String,

    /// Metadata for the sub-event associated with the detective activity
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatterDebt {
    /// Batter Uuid. For some reason this is only added to the event when Debt procs, even though
    /// the batter and fielder are always part of the event.
    pub batter_id: Uuid,

    /// Fielder Uuid. For some reason this is only added to the event when Debt procs, even though
    /// the batter and fielder are always part of the event.
    pub fielder_id: Uuid,

    /// Metadata for the sub-event associated with adding the Observed/Unstable/etc. mod. If the
    /// player already had the mod, this will be null.
    pub sub_event: Option<ModChangeSubEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TogglePerforming {
    /// Uuid of the player whose Overperforming/Underperforming was toggled
    pub player_id: Uuid,

    /// Team uuid of the player whose Overperforming/Underperforming was toggled
    pub team_id: Uuid,

    /// Name of the player whose Overperforming/Underperforming was toggled
    pub player_name: String,

    /// Whether player is now Overperforming (true) or Underperforming (false)
    pub is_overperforming: bool,

    /// Whether this is the first this toggle has procced. This is necessary for accurate
    /// reconstruction of the game event.
    pub is_first_proc: bool,

    /// Metadata for the event that adds or replaces the Overperforming or Underperforming mod
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct GrindRailTrick {
    /// Name of this Grind Rail trick
    pub trick_name: String,

    /// Point value of this grind rail trick
    pub points: i32,
}

impl Display for GrindRailTrick {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.trick_name, self.points)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
#[serde(tag = "success")]
pub enum GrindRailSuccess {
    /// The player was Safe, and secondTrick was successful
    Safe(GrindRailTrick),

    /// The player was Safe, and secondTrick failed
    TaggedOut(GrindRailTrick),

    /// The player lost their balance and bailed, and secondTrick is null
    Bailed,
}

impl Display for GrindRailSuccess {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GrindRailSuccess::Safe(trick) => {
                write!(f, "They land a {trick}!\nSafe!")
            }
            GrindRailSuccess::TaggedOut(trick) => {
                write!(f, "They're tagged out doing a {trick}!")
            }
            GrindRailSuccess::Bailed => {
                write!(f, "... but lose their balance and bail!\nOut!")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
pub enum EchoChamberModAdded {
    Repeating,
    Reverberating,
}

impl Display for EchoChamberModAdded {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EchoChamberModAdded::Repeating => { write!(f, "Repeating") }
            EchoChamberModAdded::Reverberating => { write!(f, "Reverberating") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
#[serde(tag = "type")]
pub enum ConsumerAttackEffect {
    Chomp {
        /// Player's rating before the attack
        rating_before: f64,

        /// Player's rating after the attack
        rating_after: f64,

        /// Metadata for sub-event associated with player stat change
        sub_event: SubEvent,
    },

    DefendedWithItem(ItemDamaged),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ItemDamaged {
    /// Uuid of item that was damaged
    pub item_id: Uuid,

    /// Name of item that was damaged
    pub item_name: String,

    /// Whether the item name is plural, if known. This is extracted from the message text and not
    /// all messages are phrased in a way that indicate the item's plurality.
    pub item_name_plural: Option<bool>,

    /// Mods bestowed by item that was damaged
    pub item_mods: Vec<String>,

    /// Durability of item. This is its max health.
    pub durability: i64,

    /// Current health of item
    pub health: i64,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before being damaged (TODO Clarify damage vs. breaking)
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's remaining items cause to their star
    /// rating.
    pub player_item_rating_after: f64,

    /// The player's star rating. TODO: Is this with or without items?
    pub player_rating: f64,

    /// Team Uuid of team whose item broke
    pub team_id: Uuid,

    /// Uuid of player whose item broke
    pub player_id: Uuid,

    /// Metadata for the event associated with the item being damaged
    pub sub_event: SubEvent,
}

impl Display for ItemDamaged {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.health == 0 {
            write!(f, "{} broke!", self.item_name)
        } else if self.item_name_plural.unwrap() {
            write!(f, "{} were damaged.", self.item_name)
        } else {
            write!(f, "{} was damaged.", self.item_name)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ItemGained {
    /// Uuid of item that was gained
    pub item_id: Uuid,

    /// Name of item that was gained
    pub item_name: String,

    /// Mods bestowed by item that was gained
    pub item_mods: Vec<String>,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before gaining this item
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's items now cause to their star rating
    pub player_item_rating_after: f64,

    /// The player's star rating. TODO: Is this with or without items?
    pub player_rating: f64,

    /// Team Uuid of team who gained the item
    pub team_id: Uuid,

    /// Uuid of player who gained the item
    pub player_id: Uuid,

    /// Metadata for the event associated with gaining/losing the item
    pub sub_event: SubEvent,

    /// If the player dropped an item as a result of gaining this item, contains information about
    /// the dropped item. Otherwise null.
    pub dropped_item: Option<ItemDroppedForNewItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ItemRepaired {
    /// Uuid of item that was repaired
    pub item_id: Uuid,

    /// Name of item that was repaired
    pub item_name: String,

    /// Mods bestowed by item that was repaired
    pub item_mods: Vec<String>,

    /// Durability of item. This is its max health.
    pub durability: i64,

    /// Current health of item
    pub health: i64,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before being repaired (TODO Clarify damage vs. breaking)
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's items now cause to their star
    /// rating.
    pub player_item_rating_after: f64,

    /// The player's star rating. TODO: Is this with or without items?
    pub player_rating: f64,

    /// Team Uuid of team whose item broke
    pub team_id: Uuid,

    /// Uuid of player whose item broke
    pub player_id: Uuid,

    // TODO: Move this out if it turns out there are other restoring events with the name stored
    //   outside the ItemRepaired struct
    /// Name of player whose item broke
    pub player_name: String,

    /// Metadata for the event associated with the item being damaged
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ItemDroppedForNewItem {
    /// Uuid of item that was dropped
    pub item_id: Uuid,

    /// Name of item that was dropped
    pub item_name: String,

    /// Mods bestowed by item that was dropped
    pub item_mods: Vec<String>,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before dropping this item
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's items now cause to their star rating
    pub player_item_rating_after: f64,

    /// True if the item was broken, otherwise false
    pub item_was_broken: bool,

    /// Metadata for the event associated with dropping the item
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PlayerMovedTeams {
    /// Uuid of player who moved teams
    pub player_id: Uuid,

    /// Name of player who moved teams
    pub player_name: String,

    /// Location of player within the teams
    pub location: PositionType,

    /// Uuid of player's previous team
    pub previous_team_id: Uuid,

    /// Nickname of player's previous team
    pub previous_team_nickname: String,

    /// Uuid of player's new team
    pub new_team_id: Uuid,

    /// Nickname of player's new team
    pub new_team_nickname: String,

    /// Sub-event associated with the player moving
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Carcinization {
    #[serde(flatten)]
    pub mv: PlayerMovedTeams,

    /// Full name of player's new team
    pub new_team_name: String,

    /// Metadata for sub-event associated with adding the TEMP_STOLEN mod
    pub mod_added_sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct AttractionWithPlayer {
    /// Nickname of team who attracted this player
    pub team_nickname: String,

    /// Uuid of team who attracted this player
    pub team_id: Uuid,

    /// Name of player who was attracted
    pub player_name: String,

    /// Uuid of player who was attracted
    pub player_id: Uuid,

    /// Metadata about the player being added to the team
    pub sub_event: SubEvent,

    /// After season 17, players started getting (visible) shadow boosts when being Attracted. This
    /// contains that information.
    pub boost: Option<PlayerBoostSubEvent>,
}

// Use this in contexts where the player name and ID are stored outside
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Attraction {
    /// Nickname of team who attracted this player
    pub team_nickname: String,

    /// Uuid of team who attracted this player
    pub team_id: Uuid,

    /// Metadata about the player being added to the team
    pub sub_event: SubEvent,

    /// After season 17, players started getting (visible) shadow boosts when being Attracted. This
    /// contains that information.
    pub boost: Option<PlayerBoostSubEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModDesc {
    /// Internal name of the mod
    pub mod_id: String,

    /// Duration of the mod
    pub mod_duration: ModDuration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(tag = "hitType", content = "chargeBlood")]
pub enum HitType {
    Single,
    Double(Option<ModChangeSubEvent>),
    Triple(Option<ModChangeSubEvent>),
    Quadruple,
}

impl Display for HitType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HitType::Single => { write!(f, "Single") }
            HitType::Double(_) => { write!(f, "Double") }
            HitType::Triple(_) => { write!(f, "Triple") }
            HitType::Quadruple => { write!(f, "Quadruple") }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub enum Base {
    First,
    Second,
    Third,
    Fourth,
    Fifth,
}

impl Display for Base {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Base::First => { write!(f, "first") }
            Base::Second => { write!(f, "second") }
            Base::Third => { write!(f, "third") }
            Base::Fourth => { write!(f, "fourth") }
            Base::Fifth => { write!(f, "fifth") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(tag = "hitType", content = "chargeBlood")]
pub enum HomeRunType {
    Solo,
    TwoRun,
    ThreeRun,
    GrandSlam,
}

impl Display for HomeRunType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HomeRunType::Solo => { write!(f, "solo home run") }
            HomeRunType::TwoRun => { write!(f, "2-run home run") }
            HomeRunType::ThreeRun => { write!(f, "3-run home run") }
            HomeRunType::GrandSlam => { write!(f, "grand slam") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub enum StrikeoutType {
    Looking,
    Swinging,
}

impl Display for StrikeoutType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StrikeoutType::Looking => { write!(f, "looking") }
            StrikeoutType::Swinging => { write!(f, "swinging") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Parasite {
    /// Team uuid of the batter who was parasitically drained
    pub batter_team_id: Uuid,

    /// Uuid of the batter who was parasitically drained
    pub batter_id: Uuid,

    /// Name of the batter who was parasitically drained
    pub batter_name: String,

    /// Team uuid of the batter who was parasitically drained
    pub pitcher_team_id: Uuid,

    /// Uuid of the Parasite pitcher
    pub pitcher_id: Uuid,

    /// Name of the Parasite pitcher
    pub pitcher_name: String,

    /// Drained attribute name. Should agree with attribute_ids.
    ///
    /// TODO: Should this be an enum? Then I wouldn't need both name and id
    pub attribute_name: String,

    /// Drained attribute numeric ID. Should agree with attribute_name.
    pub attribute_id: i64,

    /// Sipped player's rating before the stats changed
    pub batter_rating_before: f64,

    /// Sipped player's rating after the stats changed
    pub batter_rating_after: f64,

    /// Metadata for the sub-event about the sipper gaining stars
    pub batter_sub_event: SubEvent,

    /// Sipper player's rating before the stats changed
    pub pitcher_rating_before: f64,

    /// Sipper player's rating after the stats changed
    pub pitcher_rating_after: f64,

    /// Metadata for the sub-event about the sipper gaining stars
    pub pitcher_sub_event: SubEvent,
}

// TODO A bunch of places this is inlined should be replaced with PlayerBoostSubEvent and  #[serde(flatten)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PlayerBoostSubEvent {
    /// Player's rating before the boost
    pub rating_before: f64,

    /// Player's rating after the boost
    pub rating_after: f64,

    /// Metadata for the boost sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
#[serde(tag = "type")]
pub enum FedEventData {
    /// When a being (a god, Binky, or a similar entity) speaks
    #[serde(rename_all = "camelCase")]
    BeingSpeech {
        /// Which being is speaking
        being: Being,
        /// The text of the being's message
        message: String,
    },

    /// This is always the first event of every game
    #[serde(rename_all = "camelCase")]
    LetsGo {
        #[serde(flatten)]
        game: GameEvent,

        /// Weather for this game
        weather: Weather,

        /// Uuid of the stadium this game is being played in, if any
        stadium_id: Option<Uuid>,
    },

    /// This is always the second of event of every game
    #[serde(rename_all = "camelCase")]
    PlayBall {
        #[serde(flatten)]
        game: GameEvent,
    },

    /// Marks the start of a half-inning
    #[serde(rename_all = "camelCase")]
    HalfInningStart {
        #[serde(flatten)]
        game: GameEvent,

        /// Whether this is the top of the inning (true) or bottom of the inning (false)
        top_of_inning: bool,

        /// Zero-indexed inning number
        inning: i32,

        /// Full name of the team at bat
        batting_team_name: String,

        /// List of subseasonal mods that came into effect on this HalfInning. Currently, all of
        /// these mods add either Overperforming or Underperforming for the subseason.
        ///
        /// This array is only populated on the first HalfInning event of a game on the first game a
        /// team plays in a given subseason (Earlseason, Midseason, Lateseason, or Postseason). Most
        /// of the time this is the first day of the subseason, but the wildcard rounds in the
        /// Postseason mean that some teams don't have their first game on the first day.
        ///
        /// Subseasonal mods only started working like this in season 16. Prior to season 16 there
        /// was a separate event for these effects.
        subseasonal_mod_effects: Vec<TeamPerformingChanged>,
    },

    /// Marks a new batter stepping up to the plate
    #[serde(rename_all = "camelCase")]
    BatterUp {
        #[serde(flatten)]
        game: GameEvent,

        /// Batter's name
        batter_name: String,

        /// Batter's team's nickname
        team_nickname: String,

        /// The name of the player's legacy (pre-s15 election) item, if any, otherwise null. This
        /// should always be null from season 16 onward.
        wielding_item: Option<String>,

        /// Details of the inhabiting player, if any, otherwise null
        inhabiting: Option<Inhabiting>,

        /// True if the player is Repeating
        is_repeating: bool,
    },

    /// The event that announces when a Superyummy player loves or misses peanuts at the beginning
    /// of the game
    #[serde(rename_all = "camelCase")]
    SuperyummyGameStart {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        toggle: TogglePerforming,
    },

    /// The event that announces when a Superyummy player loves or misses peanuts at the beginning
    /// of the game. This event has different metadata when Superyummy is Echoed.
    #[serde(rename_all = "camelCase")]
    EchoedSuperyummyGameStart {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the Superyummy player
        player_name: String,

        /// Whether peanuts are present. Determines whether the player "loves" (true) or "misses"
        /// (false) peanuts.
        peanuts_present: bool,
    },

    /// Ball
    #[serde(rename_all = "camelCase")]
    Ball {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,

        /// Meta about the batter's item breaking, if it broke, otherwise null.
        batter_item_damage: Option<(String, ItemDamaged)>,
    },

    /// Foul Ball
    #[serde(rename_all = "camelCase")]
    FoulBall {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,

        /// Meta about the batter's item breaking, if it broke, otherwise null.
        batter_item_damage: Option<(String, ItemDamaged)>,

        /// If a new bird found a birdhouse, the total number of birds. Otherwise null.
        birds: Option<i32>,
    },

    /// Strike, swinging
    #[serde(rename_all = "camelCase")]
    StrikeSwinging {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,

        /// If the pitcher's item was damaged, information about the damage. Otherwise null
        pitcher_item_damage: Option<(String, ItemDamaged)>,
    },

    /// Strike, looking
    #[serde(rename_all = "camelCase")]
    StrikeLooking {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,

        /// If the pitcher's item was damaged, information about the damage. Otherwise null
        pitcher_item_damage: Option<(String, ItemDamaged)>,
    },

    /// Strike, flinching
    #[serde(rename_all = "camelCase")]
    StrikeFlinching {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count. Should always be 0, but still present in the data for
        /// forward-compatibility and convenience.
        strikes: i32,

        /// If the pitcher's item was damaged, information about the damage. Otherwise null
        pitcher_item_damage: Option<(String, ItemDamaged)>,
    },

    /// Flyout
    #[serde(rename_all = "camelCase")]
    Flyout {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of the batter that hit the flyout
        batter_name: String,

        /// Name of the batter that caught the out
        fielder_name: String,

        #[serde(flatten)]
        scores: Scores,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null. Note that scoring players losing Inhabiting is inside `scores`.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// If the batter has Debt and hit the fielder with the ball, this contains the information
        /// about adding Unstable/Observed/whatever. Otherwise it will be null.
        batter_debt: Option<BatterDebt>,

        /// Damage that the batter's item took, if any
        batter_item_damage: Option<ItemDamaged>,

        /// Damage that the fielder's item took, if any
        fielder_item_damage: Option<ItemDamaged>,

        /// Damage that any non-batter and non-fielder player's item took, if any. It's not possible
        /// to know the role of the other player (pitcher, runner?) from the event alone.
        other_player_item_damage: Option<(String, ItemDamaged)>,

        /// If there was a parasite blooddrain on this strikeout, contains information about it.
        /// Otherwise null.
        parasite: Option<Parasite>,
    },

    /// A simple ground out. This includes sacrifices but does not include fielder's choices or
    /// double plays.
    #[serde(rename_all = "camelCase")]
    GroundOut {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of player who hit the ground out
        batter_name: String,

        /// Name of fielder who caught the ground out
        fielder_name: String,

        #[serde(flatten)]
        scores: Scores,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null. Scoring players losing the Inhabiting mod is included in `scores`.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// If the batter has Debt and hit the fielder with the ball, this contains the information
        /// about adding Unstable/Observed/whatever. Otherwise it will be null.
        batter_debt: Option<BatterDebt>,

        /// Damage that the batter's item took, if any
        batter_item_damage: Option<ItemDamaged>,

        /// Damage that the pitcher's item took, if any
        pitcher_item_damage: Option<(String, ItemDamaged)>,

        /// Damage that the fielder's item took, if any
        fielder_item_damage: Option<ItemDamaged>,
    },

    /// Fielders choice event
    #[serde(rename_all = "camelCase")]
    FieldersChoice {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of batter who hit into the fielder's choice
        batter_name: String,

        /// Name of the runner who got out as a result of the fielder's choice
        runner_out_name: String,

        /// Which base the runner was tagged out on
        out_at_base: Base,

        #[serde(flatten)]
        scores: Scores,

        /// If the runner was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null. Scoring players losing the Inhabiting mod is included in `scores`.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// Items that were damaged, if any. Like home runs there isn't enough information to 
        /// properly attribute the damage to pitchers, batters, fielders, and runners.
        damaged_items: Vec<(String, ItemDamaged)>,
    },

    /// Double play event
    #[serde(rename_all = "camelCase")]
    DoublePlay {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of batter who hit into the double play
        batter_name: String,

        #[serde(flatten)]
        scores: Scores,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,
    },

    /// Hit event (Single, Double, Triple, or Quadruple)
    #[serde(rename_all = "camelCase")]
    Hit {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of the player who hit the ball
        batter_name: String,

        /// Uuid of the player who hit the ball
        batter_id: Uuid,

        /// Type of hit: Single, Double, etc.
        hit_type: HitType,

        #[serde(flatten)]
        scores: Scores,

        /// The Spicy status of the batter
        spicy_status: SpicyStatus,

        /// If the batter was Haunting, this contains metadata about removing the Inhabiting mod.
        /// Otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// Damage that the pitcher's item took, if any
        pitcher_item_damage: Option<(String, ItemDamaged)>,

        /// Damage that the batter's item took, if any
        batter_item_damage: Option<ItemDamaged>,

        /// Damage that any non-batter player's item took, if any. It's not possible to know the
        /// role of the other player (pitcher, fielder, runner?) from the event alone.
        other_player_item_damage: Option<(String, ItemDamaged)>,
    },

    /// Home run, including Grand Slam
    #[serde(rename_all = "camelCase")]
    HomeRun {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// If this is a Magmatic home run, the metadata for the event where the batter loses the
        /// Magmatic mod, otherwise null
        magmatic: Option<ModChangeSubEvent>,

        /// Name of the batter who hit the home run
        batter_name: String,

        /// Uuid of the batter who hit the home run
        batter_id: Uuid,

        /// Type of home run
        home_run_type: HomeRunType,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// List of players who used a Free Refill
        free_refills: Vec<FreeRefill>,

        /// The Spicy status of the batter
        spicy_status: SpicyStatus,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// True if the ball landed in a Big Bucket and scored an extra Run, false otherwise
        big_bucket: bool,

        /// Info about an Attractor being Attracted, if any. Otherwise null.
        attraction: Option<AttractionWithPlayer>,

        /// Info about player items that were damaged, if any.
        ///
        /// Home Run events don't really give enough information to attribute these damages to
        /// anybody. We could compare the name to the batter name, but since batters can also be
        /// on base that doesn't really give us any certain information.
        damaged_items: Vec<(String, ItemDamaged)>,
    },

    /// Stolen base
    #[serde(rename_all = "camelCase")]
    StolenBase {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the runner who stole the base
        runner_name: String,

        /// Uuid of the runner who stole the base
        runner_id: Uuid,

        /// Which base they stole
        base_stolen: Base,

        /// Whether this player scored with Blaserunning
        blaserunning: bool,

        /// Free Refill data if one was used, otherwise null
        free_refill: Option<FreeRefill>,

        /// Baserunner item damage if any, otherwise null
        runner_item_damage: Option<ItemDamaged>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,
    },

    /// Caught stealing
    #[serde(rename_all = "camelCase")]
    CaughtStealing {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the runner who tried to steal the base
        runner_name: String,

        /// Which base they tried to steal
        base_stolen: Base,
    },

    /// Strikeout swinging
    #[serde(rename_all = "camelCase")]
    StrikeoutSwinging {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of batter who struck out swinging
        batter_name: String,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// Information about the pitcher's item being damaged, if any
        pitcher_item_damage: Option<(String, ItemDamaged)>,

        /// Free Refill data if one was used, otherwise null. Free refills can happen on strikeouts
        /// thanks to Triple Threat.
        free_refill: Option<FreeRefill>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players got Unrun strikeouts the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// If there was a parasite blooddrain on this strikeout, contains information about it.
        /// Otherwise null.
        parasite: Option<Parasite>,
    },

    /// Strikeout looking
    #[serde(rename_all = "camelCase")]
    StrikeoutLooking {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of batter who struck out looking
        batter_name: String,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// Information about the pitcher's item being damaged, if any
        pitcher_item_damage: Option<(String, ItemDamaged)>,

        /// Free Refill data if one was used, otherwise null. Free refills can happen on strikeouts
        /// thanks to Triple Threat.
        free_refill: Option<FreeRefill>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players got Unrun strikeouts the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,

        /// If there was a parasite blooddrain on this strikeout, contains information about it.
        /// Otherwise null.
        parasite: Option<Parasite>,
    },

    /// Player drew a walk
    #[serde(rename_all = "camelCase")]
    Walk {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the batter who drew the walk
        batter_name: String,

        /// Uuid of the batter who drew the walk
        batter_id: Uuid,

        #[serde(flatten)]
        scores: Scores,

        /// If the batter went to a later base with Base Instincts, this is the base they went to.
        /// Otherwise null.
        base_instincts: Option<Base>,

        /// Damage that the batter's item took, if any
        batter_item_damage: Option<ItemDamaged>,

        /// If the batter was Haunting, this contains metadata about removing the Inhabiting mod.
        /// Otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,
    },

    /// Marks the end of the half-inning
    #[serde(rename_all = "camelCase")]
    InningEnd {
        #[serde(flatten)]
        game: GameEvent,

        /// Which inning just ended (one-indexed)
        inning_num: i32,

        /// List of pitchers who lost Triple Threat. Should be at most two players.
        lost_triple_threat: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Player struck out by charming the batter
    #[serde(rename_all = "camelCase")]
    CharmStrikeout {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who did the charming
        charmer_id: Uuid,

        /// Name of player who did the charming
        charmer_name: String,

        /// Uuid of player who was charmed
        charmed_id: Uuid,

        /// Name of the player who was charmed
        charmed_name: String,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// Number of swings the player was charmed into making. Should be 3 ordinarily and 4 for
        /// players with The Fourth Strike.
        num_swings: i32,
    },

    /// Zapped a strike
    #[serde(rename_all = "camelCase")]
    StrikeZapped {
        #[serde(flatten)]
        game: GameEvent,
    },

    /// Peanut flavor text messages
    #[serde(rename_all = "camelCase")]
    PeanutFlavorText {
        #[serde(flatten)]
        game: GameEvent,

        /// The text of the message
        message: String,
    },

    #[serde(rename_all = "camelCase")]
    GameEnd {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the team who won
        winner_id: Uuid,

        /// Name of the team who won
        winning_team_name: String,

        /// Score of the team who won
        winning_team_score: f32,

        /// Name of the team who lost
        losing_team_name: String,

        /// Score of the team who lost
        losing_team_score: f32,

        /// Information about a temp stolen player being returned at the end of the game, if
        /// applicable. Otherwise null.
        temp_stolen_player_returned: Option<PlayerMovedTeams>,
    },

    /// Mild pitch that does not result in a walk
    #[serde(rename_all = "camelCase")]
    MildPitch {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the pitcher who threw the mild pitch
        pitcher_id: Uuid,

        /// Name of the player who threw the mild pitch
        pitcher_name: String,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,

        /// Whether runners advance on the pathetic play (I believe runners always advance if there
        /// are any runners at all)
        runners_advance: bool,

        #[serde(flatten)]
        scores: Scores,
    },

    /// Mild pitch that results in a walk
    #[serde(rename_all = "camelCase")]
    MildPitchWalk {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the pitcher who threw the mild pitch
        pitcher_id: Uuid,

        /// Name of the pitcher who threw the mild pitch
        pitcher_name: String,

        /// Uuid of the batter who drew the walk
        batter_id: Uuid,

        /// Name of the batter who drew the walk
        batter_name: String,

        #[serde(flatten)]
        scores: Scores,
    },

    /// Player is Beaned with a Tired or Wired
    #[serde(rename_all = "camelCase")]
    CoffeeBean {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who was Beaned
        player_id: Uuid,

        /// Name of player who was Beaned
        player_name: String,

        /// Type of roast of the coffee that Beaned
        roast: String,

        /// Notes of the coffee that Beaned
        notes: String,

        /// Which mod the player was Beaned by
        which_mod: CoffeeBeanMod,

        /// True if the player gained the mod, false if they lost it
        gained_mod: bool,

        /// Metadata of the sub-event associated with adding or removing the Tired/Wired mod
        sub_event: SubEvent,

        /// Uuid for the team whose player was Beaned. Sometimes this is null and I don't know why
        team_id: Option<Uuid>,

        /// The mod this player previously had, if any. This isn't visible in the text of the event
        /// but it is in the metadata.
        previous: Option<CoffeeBeanMod>,
    },

    /// Player became magmatic
    #[serde(rename_all = "camelCase")]
    BecameMagmatic {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who became magmatic
        player_id: Uuid,

        /// Name of player who became magmatic
        player_name: String,

        /// True if the player is Unstable, false otherwise
        is_unstable: bool,

        /// Information about the player getting the Magmatic mod, if applicable. If the player was
        /// already Magmatic, this will be null
        magmatic_mod_added: Option<ModChangeSubEvent>,
    },

    /// Blooddrain event that results in player gaining the stolen blood (as opposed to using it to
    /// add/remove an out, strike. etc.), whether siphon or not
    #[serde(rename_all = "camelCase")]
    Blooddrain {
        #[serde(flatten)]
        game: GameEvent,

        /// Whether this was the result of a Siphon
        is_siphon: bool,

        /// Attribute category that was sipped
        sipped_category: AttrCategory,

        /// Player who did the sippy
        sipper: PlayerStatChange,

        /// Player who was sipped
        sipped: PlayerStatChange,
    },

    /// Blooddrain event that results in a special action (add/remove an out, strike, etc.)
    #[serde(rename_all = "camelCase")]
    SpecialBlooddrain {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who did the sippy
        sipper_id: Uuid,

        /// Name of player who did the sippy
        sipper_name: String,

        /// Uuid of player who was sipped
        sipped_id: Uuid,

        /// Team uuid of player who was sipped
        sipped_team_id: Uuid,

        /// Name of player who was sipped
        sipped_name: String,

        /// Attribute category that was sipped
        sipped_category: AttrCategory,

        /// What the drained blood was used for
        #[serde(flatten)]
        action: BlooddrainAction,

        /// Metadata for the sub-event associated with the player stat change event
        sipped_event: SubEvent,

        /// Player's rating before the stats changed. The rating category is stored externally. Rating
        /// is equivalent to stars but is on an 0-1 scale rather than an 0-5 scale.
        rating_before: f64,

        /// Player's rating after the stats changed
        rating_after: f64,
    },

    /// Mod expired after set time period (game, week, or season)
    #[serde(rename_all = "camelCase")]
    PlayerModExpires {
        /// Uuid of the team for the player whose mod(s) expired
        team_id: Uuid,

        /// Uuid of the player whose mod(s) expired
        player_id: Uuid,

        /// Name of the player whose mod(s) expired
        player_name: String,

        /// The mod(s) that were removed
        mods: Vec<String>,

        /// Duration after which the mod(s) were removed (game, week, or season)
        mod_duration: ModDuration,
    },

    /// Mod expired after set time period (game, week, or season)
    #[serde(rename_all = "camelCase")]
    TeamModExpires {
        /// Uuid of the team whose mod(s) expired
        team_id: Uuid,

        /// Nickname the team whose mod(s) expired
        team_nickname: String,

        /// The mod(s) that were removed
        mods: Vec<String>,

        /// Duration after which the mod(s) were removed (game, week, or season)
        mod_duration: ModDuration,
    },

    /// Birds Circle event. This event always has the same text ("The Birds circle ... but they
    /// don't find what they're looking for") and almost no metadata
    #[serde(rename_all = "camelCase")]
    BirdsCircle {
        #[serde(flatten)]
        game: GameEvent,
    },

    /// Batter is ambushed by crows, leading to an out. This can happen randomly or as a result of
    /// the Friend of Crows mod
    #[serde(rename_all = "camelCase")]
    AmbushedByCrows {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of batter who was ambushed
        batter_id: Uuid,

        /// Name of batter who was ambushed
        batter_name: String,

        /// If this is a Friends of Crows proc, the uuid and name of the pitcher who called upon
        /// their friends
        friend_of_crows: Option<PitcherNameId>,
    },

    /// Sun2 set a Win. This version of the event shows up in the Outcomes section and is separate
    /// from the version that shows up in the game log.
    #[serde(rename_all = "camelCase")]
    Sun2SetWin {
        /// Uuid of team who earned the Win
        team_id: Uuid,

        /// Nickname of team who earned the Win
        team_nickname: String,
    },

    /// Black hole swallowed a win. This version of the event shows up in the Outcomes section and
    /// is separate from the version that shows up in the game log.
    #[serde(rename_all = "camelCase")]
    BlackHoleSwallowedWin {
        /// Uuid of team whose Win was swallowed
        team_id: Uuid,

        /// Nickname of team whose Win was swallowed
        team_nickname: String,
    },

    /// Sun2 set a Win. This version of the event shows up in the game log and is separate from the
    /// version that shows up in the Outcomes section.
    #[serde(rename_all = "camelCase")]
    Sun2 {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who earned the Win
        team_nickname: String,

        /// If a player caught some rays, info about the player's attribute increase, otherwise null
        caught_some_rays: Option<PlayerStatChange>,
    },

    /// Black hole swallowed a win. This version of the event shows up in the game log and is
    /// separate from the version that shows up in the Outcomes section.
    #[serde(rename_all = "camelCase")]
    BlackHole {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of the team that caused the event
        scoring_team_nickname: String,

        /// Nickname of the team whose Win was swallowed
        victim_team_nickname: String,

        /// If a player was Carcinized on this event, contains details about the carcinization.
        /// Otherwise null.
        carcinization: Option<Carcinization>,

        /// If a player was compressed by gamma on this event, contains details about the stat
        /// change. Otherwise null.
        compressed_by_gamma: Option<PlayerStatChange>,
    },

    /// Team shamed another team
    #[serde(rename_all = "camelCase")]
    TeamDidShame {
        /// Uuid of the team that did the shaming
        shaming_team_id: Uuid,

        /// Nickname of the team that did the shaming
        shaming_team_nickname: String,

        /// Nickname of the team that was shamed
        shamed_team_nickname: String,

        /// Number of shames that the shaming team has performed
        total_shames: i64,

        /// Number of shames that the shaming team has received
        total_shamings: i64,
    },

    /// Team was shamed
    #[serde(rename_all = "camelCase")]
    TeamWasShamed {
        /// Uuid of the team that was shamed
        shamed_team_id: Uuid,

        /// Nickname of the team that was shamed
        shamed_team_nickname: String,

        /// Nickname of the team that did the shaming
        shaming_team_nickname: String,

        /// Number of shames that the shamed team has performed
        total_shames: i64,

        /// Number of shames that the shamed team has received
        total_shamings: i64,
    },

    /// Walk as a result of Charm
    #[serde(rename_all = "camelCase")]
    CharmWalk {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the batter that did the charming
        batter_id: Uuid,

        /// Name of the batter that did the charming
        batter_name: String,

        /// Name of the pitcher that was charmed
        pitcher_name: String,

        /// Meta about the pitcher's item breaking, if it broke, otherwise null.
        pitcher_item_damage: Option<ItemDamaged>,

        /// Meta about the batter's item breaking, if it broke, otherwise null.
        batter_item_damage: Option<ItemDamaged>,

        #[serde(flatten)]
        scores: Scores,
    },

    /// Player gained a Free Refill
    #[serde(rename_all = "camelCase")]
    GainFreeRefill {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the team of the player who gained the Free Refill. This will be null if the
        /// player is Inhabiting a Haunted player and they died before team ids were stored in the
        /// player object (i.e. during Discipline)
        team_id: Option<Uuid>,

        /// Uuid of player who gained the Free Refill
        player_id: Uuid,

        /// Name of player who gained the Free Refill
        player_name: String,

        /// Roast of the coffee that bestowed the Free Refill
        roast: String,

        /// First ingredient of the coffee that bestowed the Free Refill
        ingredient1: String,

        /// Second ingredient of the coffee that bestowed the Free Refill
        ingredient2: String,

        /// Metadata for the sub-event associated with the Free Refill mod-added event
        sub_event: SubEvent,
    },

    /// Player suffered an allergic reaction (note: yummy reactions and the Feed never coexisted,
    /// so all peanut reactions in the Feed were allergic)
    #[serde(rename_all = "camelCase")]
    AllergicReaction {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the team of the player who suffered the allergic reaction
        team_id: Uuid,

        /// Uuid of the player who suffered the allergic reaction
        player_id: Uuid,

        /// Name of the player who suffered the allergic reaction
        player_name: String,

        /// Metadata for the sub-event associated with the player stat change event
        sub_event: SubEvent,

        /// Player rating before the stat change
        rating_before: f64,

        /// Player rating after the stat change
        rating_after: f64,
    },

    /// Player perked up at start of game
    #[serde(rename_all = "camelCase")]
    PerkUp {
        #[serde(flatten)]
        game: GameEvent,

        /// Players who gained Overperforming as a result of Perk
        players: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Feedback
    #[serde(rename_all = "camelCase")]
    Feedback {
        #[serde(flatten)]
        game: GameEvent,

        /// The two players involved in the feedback. I believe the first is always the initiator,
        /// as indicated by Flickering, but I'm not sure.
        players: (FeedbackPlayerData, FeedbackPlayerData),

        /// If LCD soundsystem was in effect, the boost events for the players. This is in the same
        /// order as `players`.
        lcd_soundsystem: Option<(PlayerBoostSubEvent, PlayerBoostSubEvent)>,

        /// The position of the players that were swapped
        position_type: ActivePositionType,

        /// Metadata for the `PlayerTraded` sub-event
        sub_event: SubEvent,
    },

    /// Reverb bestows the Reverberating mod
    #[serde(rename_all = "camelCase")]
    BestowReverberating {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team of player who was given Reverberating
        team_id: Uuid,

        /// Uuid of player who was given Reverberating
        player_id: Uuid,

        /// Name of player who was given Reverberating
        player_name: String,

        /// Sub-event associated with the `AddedMod` event
        sub_event: SubEvent,
    },

    /// Reverb swap
    #[serde(rename_all = "camelCase")]
    Reverb {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team who got reverbed
        team_id: Uuid,

        /// Nickname of team who got reverbed
        team_nickname: String,

        /// Type of reverb that happened, with metadata for the associated `ReverbRosterShuffle`
        /// sub-event
        #[serde(flatten)]
        reverb_type: ReverbType,

        /// Players who were kept in place with Gravity
        gravity_players: Vec<PlayerNameId>,
    },

    /// Tarot readings
    #[serde(rename_all = "camelCase")]
    TarotReading {
        /// Tarot reading description
        description: String,

        /// Metadata associated with the tarot reading. This is vague on purpose to be generic.
        metadata: serde_json::Value,

        /// Uuids of players involved in this tarot reading. This is vague on purpose to be generic.
        player_tags: Vec<Uuid>,

        /// Uuids of teams involved in this tarot reading. This is vague on purpose to be generic.
        team_tags: Vec<Uuid>,
    },

    /// Added or removed a mod as a result of a Tarot reading
    #[serde(rename_all = "camelCase")]
    TarotReadingAddedOrRemovedMod {
        /// Uuid of team who gained/lost the mod or team of player who gained/lost the mod
        team_id: Uuid,

        /// Uuid of player who gained/lost the mod, if it was a player. Null if it was a team.
        player_id: Option<Uuid>,

        /// Description of the event that added/removed the mod
        description: String,

        /// Internal ID of the mod that was gained/lost
        r#mod: String,

        /// Duration of the mod that was gained/lost
        mod_duration: ModDuration,

        /// True if the mod was lost, false if it was gained
        mod_removed: bool,
    },

    /// Team entered Party Time!
    #[serde(rename_all = "camelCase")]
    TeamEnteredPartyTime {
        /// Uuid of team who just entered Party Time
        team_id: Uuid,

        /// Nickname of team who just entered Party Time
        team_nickname: String,
    },

    /// Player becomes Triple Threat at start of game
    #[serde(rename_all = "camelCase")]
    BecomeTripleThreat {
        #[serde(flatten)]
        game: GameEvent,

        /// Add mod events for the players who became Triple Threat. This array will be either 1 or
        /// 2 entries.
        pitchers: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Under Over procced
    #[serde(rename_all = "camelCase")]
    UnderOver {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of player whose Under Over procced
        team_id: Uuid,

        /// Uuid of player whose Under Over procced
        player_id: Uuid,

        /// Name of player whose Under Over procced
        player_name: String,

        /// Whether Over Under turned on or off
        on: bool,

        /// Metadata for the sub-event associated with adding or removing Overperforming
        sub_event: SubEvent,
    },

    /// Over Under procced
    #[serde(rename_all = "camelCase")]
    OverUnder {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of player whose Over Under procced
        team_id: Uuid,

        /// Uuid of player whose Over Under procced
        player_id: Uuid,

        /// Name of player whose Over Under procced
        player_name: String,

        /// Whether Over Under turned on or off
        on: bool,

        /// Metadata for the sub-event associated with adding or removing Underperforming
        sub_event: SubEvent,
    },

    /// Player tastes the infinite and Shells another player
    #[serde(rename_all = "camelCase")]
    TasteTheInfinite {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who shelled the other player
        sheller_id: Uuid,

        /// Name of player who shelled the other player
        sheller_name: String,

        /// Team uuid of player who was shelled
        shellee_team_id: Uuid,

        /// Uuid of player who was shelled
        shellee_id: Uuid,

        /// Name of player who was shelled
        shellee_name: String,

        /// Metadata for the sub-event associated with adding the Shelled mod
        sub_event: SubEvent,
    },

    /// Batter skipped event
    #[serde(rename_all = "camelCase")]
    BatterSkipped {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of batter who got skipped
        batter_name: String,

        /// Reason the batter was skipped
        reason: BatterSkippedReason,
    },

    /// Feedback failed and initiator was tangled in the feedback
    #[serde(rename_all = "camelCase")]
    FeedbackBlocked {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who resisted feedback
        resisted_id: Uuid,

        /// Name of player who resisted feedback
        resisted_name: String,

        /// Uuid of player who attempted feedback, failed, and was tangled
        tangled_id: Uuid,

        /// Team uuid of player who attempted feedback, failed, and was tangled
        tangled_team_id: Uuid,

        /// Name of player who attempted feedback, failed, and was tangled
        tangled_name: String,

        /// Rating of player who attempted feedback before the event
        tangled_rating_before: f64,

        /// Rating of player who attempted feedback after the event
        tangled_rating_after: f64,

        /// Metadata for sub-event associated with player stat change event
        sub_event: SubEvent,
    },

    /// Team breaks ground on ballpark and ground is broken
    #[serde(rename_all = "camelCase")]
    FlagPlanted {
        /// Uuid of team who broke ground
        team_id: Uuid,

        /// Nickname of team who broke ground
        team_nickname: String,

        /// Name of newly created ballpark
        ballpark_name: String,

        /// Name of prefab used for newly created ballpark
        prefab_name: String,

        /// Internal renovation ID. TODO: Does this correspond to the prefab?
        renovation_id: String,

        /// Number of votes team spent on the ballpark
        votes: i64,

        /// Whether this was the first ballpark. There was a slightly different message for the
        /// first one.
        is_first: bool,
    },

    /// Emergency Alerty
    #[serde(rename_all = "camelCase")]
    EmergencyAlert {
        /// Message of emergency alert
        message: String,

        /// Teams involved in emergency alert
        team_tags: Vec<Uuid>,
    },

    /// Team was added to ILB
    #[serde(rename_all = "camelCase")]
    TeamJoinedILB {
        /// Uuid of newly added team
        team_id: Uuid,

        /// Nickname of newly added team
        team_nickname: String,

        /// Uuid of division to which team was added
        division_id: Uuid,

        /// Name of division to which team was added
        division_name: String,
    },

    /// Players swept off base by Flooding
    #[serde(rename_all = "camelCase")]
    FloodingSwept {
        #[serde(flatten)]
        game: GameEvent,

        /// List of effects in the order in which they occurred
        effects: Vec<FloodingSweptEffect>,

        /// List of players who used a Free Refill
        free_refills: Vec<FreeRefill>,

        /// Whether the Flood Pumps activated
        flood_pumps: bool,
    },

    /// Player returned from Elsewhere
    #[serde(rename_all = "camelCase")]
    ReturnFromElsewhere {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of player who returned from Elsewhere
        player_name: String,

        /// Which flavor of return from elsewhere this is
        #[serde(flatten)]
        flavor: ReturnFromElsewhereFlavor,
    },

    /// Player was incinerated
    #[serde(rename_all = "camelCase")]
    Incineration {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team whose player who was incinerated
        team_id: Uuid,

        /// Nickname of team whose player was incinerated
        team_nickname: String,

        /// Uuid of player who was incinerated
        victim_id: Uuid,

        /// Name of player who was incinerated
        victim_name: String,

        /// Uuid of replacement player
        replacement_id: Uuid,

        /// Name of replacement player
        replacement_name: String,

        /// Location of incinerated and replacement player
        location: ActivePositionType,

        /// If the player was unstable, the player that the instability chained to. Otherwise null.
        /// Use the null-ness of this property to tell whether this was an Unstable incineration.
        unstable_chain: Option<ModChangeSubEventWithNamedPlayer>,

        /// Metadata for the incineration sub-event, the enters-hall sub-event, the hatch sub-event,
        /// and the replacement sub-event, in that order
        sub_events: (SubEvent, SubEvent, SubEvent, SubEvent),
    },

    /// Pitcher change event. This happens automatically when something incapacitates the active
    /// pitcher (e.g. the player is shelled by Taste the Infinite)
    #[serde(rename_all = "camelCase")]
    PitcherChange {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team whose pitcher changed
        team_nickname: String,

        /// Uuid of new pitcher
        pitcher_id: Uuid,

        /// Name of new pitcher
        pitcher_name: String,
    },

    /// Team partied
    #[serde(rename_all = "camelCase")]
    Party {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team who partied
        team_id: Uuid,

        /// Uuid of player who partied
        player_id: Uuid,

        /// Name of player who partied
        player_name: String,

        /// Metadata for sub-event associated with player stat change
        sub_event: SubEvent,

        /// Player's rating before the party
        ///
        /// TODO I think SIBR figured out how this rating works. Look that up
        rating_before: f64,

        /// Player's rating after the party
        rating_after: f64,
    },

    /// Player was hatched from the Field of Eggs
    #[serde(rename_all = "camelCase")]
    PlayerHatched {
        /// Uuid of newly hatched player
        player_id: Uuid,

        /// Name of newly hatched player
        player_name: String,
    },

    /// Team received a postseason birth. I believe this is always preceded by a PlayerHatched event
    #[serde(rename_all = "camelCase")]
    PostseasonBirth {
        /// Uuid of team who received the birth
        team_id: Uuid,

        /// Nickname of team who received the birth
        team_nickname: String,

        /// Player who was birthed onto the team
        player_id: Uuid,

        /// Name of player who was birthed onto the team
        player_name: String,

        /// Position of the new birth within the shadows
        location: ShadowPositionType,
    },

    /// Place of team in the final standings
    #[serde(rename_all = "camelCase")]
    FinalStandings {
        /// Uuid of team
        team_id: Uuid,

        /// Nickname of team
        team_nickname: String,

        /// Place of team within the division
        place: i32,

        /// Name of division
        division_name: String,
    },

    /// Event indicating when a team leaves Party Time because it's been drafted into the postseason
    #[serde(rename_all = "camelCase")]
    TeamLeftPartyTimeForPostseason {
        /// Uuid of team who left Party Time
        team_id: Uuid,

        /// Name of team who left Party Time
        team_nickname: String,
    },

    /// Team earned a slot in the postseason
    #[serde(rename_all = "camelCase")]
    EarnedPostseasonSlot {
        /// Uuid of team who earned a slot in the postseason
        team_id: Uuid,

        /// Nickname of team who earned a slot in the postseason
        team_nickname: String,
    },

    /// Team advanced to next round of the postseason
    #[serde(rename_all = "camelCase")]
    PostseasonAdvance {
        /// Uuid of team who advanced in the postseason
        team_id: Uuid,

        /// Nickname of team who advanced in the postseason
        team_nickname: String,

        /// Round to which the team advanced, or null for the Internet Series
        round: Option<i32>,

        /// One-indexed season number
        displayed_season: i32,
    },

    /// Team was eliminated from the postseason
    #[serde(rename_all = "camelCase")]
    PostseasonEliminated {
        /// Uuid of team who was eliminated from the postseason
        team_id: Uuid,

        /// Nickname of team who was eliminated from the postseason
        team_nickname: String,

        /// One-indexed season number
        displayed_season: i32,
    },

    /// Player was boosted during election
    #[serde(rename_all = "camelCase")]
    PlayerBoosted {
        /// Uuid of team whose player was boosted
        team_id: Uuid,

        /// Uuid of player who was boosted
        player_id: Uuid,

        /// Name of player who was boosted
        player_name: String,

        /// Player rating before being boosted
        rating_before: f64,

        /// Player rating after being boosted
        rating_after: f64,
    },

    /// Team won the Internet Series
    #[serde(rename_all = "camelCase")]
    TeamWonInternetSeries {
        /// Uuid of team who won the series
        team_id: Uuid,

        /// Name of team who won the series
        team_nickname: String,

        /// Number of championships the team now has
        championships: i64,
    },

    /// Bottom Dwellers team mod procs
    #[serde(rename_all = "camelCase")]
    BottomDwellers {
        /// Uuid of team whose bottom dwellers procced
        team_id: Uuid,

        /// Nickname of team whose bottom dwellers procced
        team_nickname: String,

        /// Team rating before Bottom Dwellers
        rating_before: f64,

        /// Team rating after Bottom Dwellers
        rating_after: f64,
    },

    /// Team received a Will. This event is currently minimally parsed, with metadata simply
    /// included as-is. If you have a use-case where thoroughly parsing this event type would be
    /// useful please let us know in the SIBR discord.
    #[serde(rename_all = "camelCase")]
    WillReceived {
        /// Uuid of team who received the Will
        team_id: Uuid,

        /// Title of Will that was earned. This may be redundant with the title in `metadata`
        will_title: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,
    },

    /// Team won a Blessing. This event is currently minimally parsed, with metadata simply
    /// included as-is. If you have a use-case where thoroughly parsing this event type would be
    /// useful please let us know in the SIBR discord.
    #[serde(rename_all = "camelCase")]
    BlessingWon {
        /// Team tags of the Blessing event. This is often the Uuid of the team who won the
        /// blessing, but not always. For example, the Pitching Flotation Bubble has the Uuids of
        /// all affected teams.
        team_tags: Vec<Uuid>,

        /// Title of Blessing that was won. This may be redundant with the title in `metadata`
        blessing_title: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,
    },

    /// Earlbirds mod procs at the beginning of Earlseason
    #[serde(rename_all = "camelCase")]
    EarlbirdsAddedToTeam {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of Earlbird team
        team_id: Uuid,

        /// Name of Earlbird team
        team_nickname: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        sub_event: SubEvent,
    },

    /// Decree passed. This event is currently minimally parsed, with metadata simply included
    /// as-is. If you have a use-case where thoroughly parsing this event type would be useful
    /// please let us know in the SIBR discord.
    #[serde(rename_all = "camelCase")]
    DecreePassed {
        /// Title of Decree that passesd. This may be redundant with the title in `metadata`
        decree_title: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,
    },

    /// Player was added to ILB
    #[serde(rename_all = "camelCase")]
    PlayerJoinedILB {
        /// Uuid of newly added player
        player_id: Uuid,

        /// Name of newly added player
        player_name: String,
    },

    /// A Returned player was permitted to stay (not called back to the Hall at the end of the
    /// season)
    #[serde(rename_all = "camelCase")]
    PlayerPermittedToStay {
        /// Uuid of player who was permitted to stay
        player_id: Uuid,

        /// Name of player who was permitted to stay
        player_name: String,
    },

    /// Umpire tried to incinerate the player, but the player was Fireproof
    #[serde(rename_all = "camelCase")]
    FireproofIncineration {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of fireproof player
        player_id: Uuid,

        /// Name of fireproof player
        player_name: String,
    },

    /// Team's lineup was sorted as a result of gaining Base Dealing
    #[serde(rename_all = "camelCase")]
    LineupSorted {
        /// Uuid of team whose lineup was just sorted
        team_id: Uuid,

        /// Nickname of team whose lineup was just sorted
        team_nickname: String,
    },

    /// Earlbirds mod is removed at the end of Earlseason
    #[serde(rename_all = "camelCase")]
    EarlbirdsRemovedFromTeam {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of Earlbird team
        team_id: Uuid,

        /// Metadata for the sub-event that removes the Overperforming mod
        sub_event: SubEvent,
    },

    /// Team went Undersea
    #[serde(rename_all = "camelCase")]
    Undersea {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team who went Undersea
        team_id: Uuid,

        /// Uuid of team who went Undersea
        team_name: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        sub_event: SubEvent,
    },

    /// Renovation was built at a Ballpark
    #[serde(rename_all = "camelCase")]
    RenovationBuilt {
        /// Uuid of team who owns the Ballpark
        team_id: Uuid,

        /// Flavor text for building the renovation
        description: String,

        /// Internal ID for the renovation
        renovation_id: String,

        /// User-visible name of the renovation
        renovation_title: String,

        /// Number of votes cast for this renovation
        ///
        /// This is ordinarily an int, but for the three renovations that were added manually to
        /// undo the reno fraud of season 14 it is a string.
        // TODO Verify that this serializes without any intermediate structure
        votes: RenovationVotes,
    },

    /// Late to the Party mod procs at the beginning of Lateseason
    #[serde(rename_all = "camelCase")]
    LateToThePartyAdded {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of Late to the Party team
        ///
        /// It seems that there's one event that has the sub-event and team Uuid and then another
        /// that doesn't. Shrug emoji.
        team_id: Option<Uuid>,

        /// Name of Late to the Party team
        team_nickname: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        ///
        /// It seems that there's one event that has the sub-event and team Uuid and then another
        /// that doesn't. Shrug emoji.
        sub_event: Option<SubEvent>,
    },

    /// The peanut mister activates and cures a player's peanut allergy
    #[serde(rename_all = "camelCase")]
    PeanutMister {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who got Misted
        player_id: Uuid,

        /// Name of player who got Misted
        player_name: String,

        /// If the mister cured a Superallergy, this will be metadata about the event associated
        /// with losing the Superallergic mod. For a normal allergy this will be null.
        superallergy: Option<ModChangeSubEvent>,
    },

    /// Player was named an MVP
    #[serde(rename_all = "camelCase")]
    PlayerNamedMvp {
        /// Uuid of team of player who was named an MVP
        team_id: Uuid,

        /// Uuid of player who was named an MVP
        player_id: Uuid,

        /// Name of player who was named an MVP
        player_name: String,

        /// Which level of MVP this player attained. The associated ego mod will be EGO{level}. This
        /// is 1-indexed.
        level: i32,
    },

    /// Late to the Party wore off for the team
    #[serde(rename_all = "camelCase")]
    LateToThePartyRemoved {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team whose Late to the Party wore off
        team_nickname: String,
    },

    /// The birds circle and peck a Shelled player free
    #[serde(rename_all = "camelCase")]
    BirdsUnshell {
        #[serde(flatten)]
        game: GameEvent,

        /// Team Uuid of player who got pecked free
        team_id: Uuid,

        /// Uuid of player who got pecked free
        player_id: Uuid,

        /// Name of player who got pecked free
        player_name: String,

        /// Metadata for the sub-event about being pecked free
        pecked_free_event: SubEvent,

        /// Metadata for the sub-event about gaining a Superallergy
        superallergy_event: SubEvent,
    },

    /// A Returned player on this Team was called back to the Hall and replaced by a newly-promoted
    /// player from the Shadows
    #[serde(rename_all = "camelCase")]
    ReplaceReturnedPlayerFromShadows {
        /// Uuid of team whose players were moved around
        team_id: Uuid,

        /// Nickname of team whose players were moved around
        team_nickname: String,

        /// Uuid of player who was promoted
        promoted_player_id: Uuid,

        /// Name of player who was promoted
        promoted_player_name: String,

        /// Previous location of the player who was promoted
        promoted_location: ShadowPositionType,

        /// Uuid of player who was removed
        removed_player_id: Uuid,

        /// Name of player who was removed
        removed_player_name: String,

        /// Previous location of the player who was removed
        removed_location: ActivePositionType,
    },

    /// Player was called back to the Hall at the end of the Season
    #[serde(rename_all = "camelCase")]
    PlayerCalledBackToHall {
        /// Uuid of player who was called back to the Hall
        player_id: Uuid,

        /// Name of player who was called back to the Hall
        player_name: String,
    },

    /// Team used their Free Will
    #[serde(rename_all = "camelCase")]
    TeamUsedFreeWill {
        /// Uuid of team who used their Free Will
        team_id: Uuid,

        /// Name of team who used their Free Will
        team_nickname: String,
    },

    /// Player lost a mod
    #[serde(rename_all = "camelCase")]
    PlayerLostMod {
        /// Team uuid of player who lost the mod
        team_id: Uuid,

        /// Uuid of player who lost the mod
        player_id: Uuid,

        /// Name of player who lost the mod
        player_name: String,

        /// Internal ID of the mod that was lost
        r#mod: String,

        /// User-facing name of the mod that was lost
        mod_name: String,
    },

    /// Investigation progress. This could be parsed further, contributions welcome.
    #[serde(rename_all = "camelCase")]
    InvestigationMessage {
        /// Uuid of player doing the investigating
        player_id: Uuid,

        /// Investigation progress message (event description)
        message: String,
    },

    /// High Pressure status messages from Season 14. They were removed in the following season,
    /// presumably for occurring too often and cluttering up the Feed.
    #[serde(rename_all = "camelCase")]
    HighPressure {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team with High Pressure
        team_id: Uuid,

        /// Nickname of team with High Pressure
        team_nickname: String,

        /// Whether High Pressure just turned on (true) or off (false)
        is_on: bool,

        /// Metadata for sub-event for adding or removing Overperforming
        sub_event: SubEvent,
    },

    /// Player was "pulled through the Rift". This was used in the Second Wyatt Masoning and nowhere
    /// else.
    #[serde(rename_all = "camelCase")]
    PlayerPulledThroughRift {
        /// Uuid of newly added player
        player_id: Uuid,

        /// Name of newly added player
        player_name: String,
    },

    /// Player Localized on to a team. This occurred as part of the Second Wyatt Masoning.
    #[serde(rename_all = "camelCase")]
    PlayerLocalized {
        /// Uuid of team the player localized onto
        team_id: Uuid,

        /// Nickname of team the player localized onto
        team_nickname: String,

        /// Uuid of player who localized onto the team
        player_id: Uuid,

        /// Name of player who localized onto the team
        player_name: String,

        /// Position of the new player within the team
        location: ActivePositionType,
    },

    /// Player Echoed another player
    #[serde(rename_all = "camelCase")]
    Echo {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of player who was echoed (info for the echoer is in main_echo)
        echoee_name: String,

        /// Information about the effect on the echoer
        primary_echo: Echo,

        /// Information about the effects on any receivers that were affected
        receiver_echos: Vec<Echo>,
    },

    /// The Solar Panels await at the beginning of a game
    #[serde(rename_all = "camelCase")]
    SolarPanelsAwait {
        #[serde(flatten)]
        game: GameEvent,
    },

    /// Players Echoed into Static
    #[serde(rename_all = "camelCase")]
    EchoIntoStatic {
        #[serde(flatten)]
        game: GameEvent,

        /// Metadata for the (presumed) initiator of the Echo
        echoer: EchoIntoStatic,

        /// Metadata for the (presumed) victim of the Echo
        echoee: EchoIntoStatic,
    },

    /// Psychoacoustics echoed a mod
    #[serde(rename_all = "camelCase")]
    Psychoacoustics {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of stadium with Psychoacoustics
        stadium_name: String,

        /// Uuid of team who Echoed the mod
        team_id: Uuid,

        /// Nickname of team who Echoed the mod
        team_nickname: String,

        /// Name of mod that was Echoed
        mod_name: String,

        /// Internal ID of mod that was echoed
        mod_id: String,

        /// Metadata for the sub-event associated with adding the mod
        sub_event: SubEvent,
    },

    /// An Echo Echoed a Receiver and turned them into an Echo
    #[serde(rename_all = "camelCase")]
    EchoReceiver {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of Echo who Echoed the Receiver
        echoer_name: String,

        /// Name of Receiver who was Echoed
        echoee_name: String,

        /// Uuid of Receiver who was Echoed
        echoee_id: Uuid,

        /// Team uuid of Receiver who was Echoed
        echoee_team_id: Uuid,

        /// Metadata for the sub-event associated with changing the Receiver mod to Echo
        sub_event: SubEvent,
    },

    /// Player was attacked by a Consumer
    #[serde(rename_all = "camelCase")]
    ConsumerAttack {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of player who was attacked by the Consumer
        team_id: Uuid,

        /// Uuid of player who was attacked by the Consumer
        player_id: Uuid,

        /// Name of player who was attacked by the Consumer. It's in all caps because it was parsed
        /// from the event description, where it appears in all caps.
        player_name_all_caps: String,

        /// Effect of the attack
        effect: ConsumerAttackEffect,

        /// Detective activity, if any
        sensed_something_fishy: Option<DetectiveActivity>,

        /// Whether the player who was attacked was Scattered
        scattered: bool,
    },

    /// Team gained a Free Will
    #[serde(rename_all = "camelCase")]
    TeamGainedFreeWill {
        /// Uuid of team who gained the Free Will
        team_id: Uuid,

        /// Nickname of team who gained the Free Will
        team_nickname: String,
    },

    /// Tidings section of Election results. This event is currently minimally parsed, with metadata
    /// simply included as-is. If you have a use-case where thoroughly parsing this event type would
    /// be useful please let us know in the SIBR discord.
    Tidings {
        /// Tidings message
        message: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,

        /// Player tags exactly as it appears in the Feed event
        player_tags: Vec<Uuid>,
    },

    /// The event that announces when a Homebody is happy to be home or misses home at the beginning
    /// of the game
    #[serde(rename_all = "camelCase")]
    HomebodyGameStart {
        #[serde(flatten)]
        game: GameEvent,

        /// List of data for all players with Homebody in this game
        homebodies: Vec<TogglePerforming>,
    },

    /// The Salmon swim upstream
    #[serde(rename_all = "camelCase")]
    SalmonSwim {
        #[serde(flatten)]
        game: GameEvent,

        /// The inning number according to the event description. 1-indexed.
        inning_num: i32,

        /// Runs lost to the Salmon
        run_losses: RunLossesFromSalmon,

        /// Item restored by the salmon, if any
        item_restored: Option<ItemRepaired>,

        /// Player caught in the bind, if any
        player_expelled: Option<ModChangeSubEventWithNamedPlayer>,
    },

    /// Pitcher hit batter with a pitch, batter is now Observed (will add Unstable support later)
    #[serde(rename_all = "camelCase")]
    HitByPitch {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who threw the HBP
        pitcher_id: Uuid,

        /// Name of player who threw the HBP
        pitcher_name: String,

        /// Team uuid of player who was hit by the HBP
        batter_team_id: Uuid,

        /// Uuid of player who was hit by the HBP
        batter_id: Uuid,

        /// Name of player who was hit by the HBP
        batter_name: String,

        /// Metadata for the event associated with adding the Observed mod
        sub_event: SubEvent,

        #[serde(flatten)]
        scores: Scores,
    },

    /// Solar Panels activate, stop Sun 2 from swallowing the runs, and save them for the activating
    /// team's next game
    #[serde(rename_all = "camelCase")]
    SolarPanelsActivate {
        #[serde(flatten)]
        game: GameEvent,

        /// Number of runs saved for the team's next game
        num_runs: f32,

        /// Nickname of the team who activted Solar Panels
        team_nickname: String,
    },

    /// (Un)runs are Overflowing from a previous Solar Panels or Event Horizon activation
    #[serde(rename_all = "camelCase")]
    RunsOverflowing {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who gained the (Un)runs
        team_nickname: String,

        /// Number of Runs (positive) or Unruns (negative) gained
        num_runs: f32,
    },

    // TODO: Earlseason is separate Fed events for add and remove, but Middling is the same event.
    //   Choose one and stick with it.
    /// Team gains or loses Middling
    #[serde(rename_all = "camelCase")]
    TeamMiddling {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team became or un-became Middling
        team_nickname: String,

        /// Whether this team just became Middling (true) or un-became Middling (false)
        is_middling: bool,

        #[serde(flatten)]
        change_event: ModChangeSubEvent,
    },

    /// Detective enters a Crime Scene
    #[serde(rename_all = "camelCase")]
    EnterCrimeScene {
        #[serde(flatten)]
        game: GameEvent,

        // TODO Document these
        player_id: Uuid,
        player_name: String,
        previous_team_id: Uuid,
        previous_team_name: String,
        previous_location: PositionType,
        new_team_id: Uuid,
        new_team_name: String,
        stadium_name: String,
        rating_before: f64,
        rating_after: f64,

        enter_crime_scene_sub_event: SubEvent,
        enter_shadows_sub_event: SubEvent,
    },

    /// Detective returns from an Investigation
    #[serde(rename_all = "camelCase")]
    ReturnFromInvestigation {
        // TODO Document these
        player_id: Uuid,
        player_name: String,
        previous_team_id: Uuid,
        previous_team_name: String,
        new_location: PositionType,
        new_team_id: Uuid,
        new_team_name: String,
        emptyhanded: bool,
    },

    /// Investigation at stadium concluded
    #[serde(rename_all = "camelCase")]
    InvestigationConcluded {
        /// Uuid of the team at whose stadium the investigation was concluded
        team_id: Uuid,

        /// Name of the stadium at which the investigation has concluded
        stadium_name: String,
    },

    /// Player hopped on the Grind Rail
    #[serde(rename_all = "camelCase")]
    GrindRail {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who hopped on the Grind Rail
        player_id: Uuid,

        /// Name of the player who hopped on the Grind Rail
        player_name: String,

        /// First trick this player attempted. This trick always succeeds.
        first_trick: GrindRailTrick,

        /// Second trick this player attempted. This trick does not always succeed
        #[serde(rename = "secondTrick")] // this makes sense given the external tag
        success: GrindRailSuccess,
    },

    /// Player entered the Secret Base
    #[serde(rename_all = "camelCase")]
    EnterSecretBase {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who entered the Secret Base
        player_id: Uuid,

        /// Name of the player who entered the Secret Base
        player_name: String,
    },

    /// Player exits the Secret Base
    #[serde(rename_all = "camelCase")]
    ExitSecretBase {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who exited the Secret Base
        player_id: Uuid,

        /// Name of the player who exited the Secret Base
        player_name: String,
    },

    /// Echo Chamber makes a player temporarily Repeating
    #[serde(rename_all = "camelCase")]
    EchoChamber {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of the player who was made Repeating. If the player was a ghost who died
        /// before team ids were stored in the player object, this will be null.
        team_id: Option<Uuid>,

        /// Uuid of the player who was made Repeating
        player_id: Uuid,

        /// Name of the player who was made Repeating
        player_name: String,

        /// Whether the player was made Repeating or Reverberating
        which_mod: EchoChamberModAdded,

        /// Metadata for the event associated with adding the Repeating or Reverberating mod
        sub_event: SubEvent,
    },

    /// Player Roamed at the end of the Season
    #[serde(rename_all = "camelCase")]
    Roam {
        /// Uuid of player who roamed
        player_id: Uuid,

        /// Name of player who roamed
        player_name: String,

        /// Location of player within the teams
        location: PositionType,

        /// Uuid of player's previous team
        previous_team_id: Uuid,

        /// Nickname of player's previous team
        previous_team_nickname: String,

        /// Uuid of player's new team
        new_team_id: Uuid,

        /// Nickname of player's new team
        new_team_nickname: String,
    },

    /// A shimmering Crate descends during Glitter weather
    #[serde(rename_all = "camelCase")]
    GlitterCrate {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the player who received the item from the crate
        player_name: String,

        /// Info about the item that was received form the crate
        #[serde(flatten)]
        gained_item: ItemGained,
    },

    /// A player's mods created from another mod were removed
    #[serde(rename_all = "camelCase")]
    ModsFromAnotherModRemoved {
        /// Uuid of the team who lost the mod(s)
        team_id: Uuid,

        /// Uuid of the player who lost the mod(s)
        player_id: Uuid,

        /// Name of the player who lost the mod(s)
        player_name: String,

        /// List of mods that were removed
        mods_removed: Vec<ModDesc>,

        /// Name of the mod that had originally added the removed mods. It's implied that this mod
        /// was just removed, which caused these others to be removed as well.
        source_mod_name: String,

        /// Internal name of the mod that had originally added the removed mods
        source_mod_id: String,
    },

    /// A Consumer was expelled by Salmon Cannons
    #[serde(rename_all = "camelCase")]
    ConsumerExpelled {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who was targeted by the Consumer
        player_id: Uuid,
    },

    /// Earlbirds mod procs at the beginning of Earlseason
    #[serde(rename_all = "camelCase")]
    EarlbirdsAddedToPlayer {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of Earlbird player
        team_id: Uuid,

        /// Uuid of Earlbird player
        player_id: Uuid,

        /// Name of Earlbird player
        player_name: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        sub_event: SubEvent,
    },

    /// Walk as a result of a Mind Trick
    #[serde(rename_all = "camelCase")]
    MindTrickWalk {
        #[serde(flatten)]
        game: GameEvent,

        /// The type of strikeout this originally was (swinging or looking)
        strikeout_type: StrikeoutType,

        /// Uuid of the batter that did the mind trick
        batter_id: Uuid,

        /// Name of the batter that did the mind trick
        batter_name: String,

        /// If the batter went to a later base with Base Instincts, this is the base they went to.
        /// Otherwise null.
        base_instincts: Option<Base>,

        // /// Meta about the pitcher's item breaking, if it broke, otherwise null.
        // pitcher_item_damage: Option<ItemDamaged>,
        //
        // /// Meta about the batter's item breaking, if it broke, otherwise null.
        // batter_item_damage: Option<ItemDamaged>,

        #[serde(flatten)]
        scores: Scores,
    },

    /// Strikeout as a result of a Mind Trick ("strikes out thinking")
    #[serde(rename_all = "camelCase")]
    MindTrickStrikeout {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the batter that was mind tricked
        batter_id: Uuid,

        /// Name of the batter that was mind tricked
        batter_name: String,

        /// Name of the pitcher that did the mind trick
        pitcher_name: String,
    },

    /// Blooddrain blocked due to Sealant
    #[serde(rename_all = "camelCase")]
    BlooddrainBlocked {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player that attempted to blooddrain the Sealed player
        sipper_id: Uuid,

        /// Name of the player that attempted to blooddrain the Sealed player
        sipper_name: String,

        /// Uuid of the Sealed player
        sippee_id: Uuid,

        /// Name of the Sealed player
        sippee_name: String,
    },

    /// Earlbirds is removed at the beginning of Midseason
    #[serde(rename_all = "camelCase")]
    EarlbirdsRemovedFromPlayer {
        #[serde(flatten)]
        game: GameEvent,

        /// Team uuid of Earlbird player
        team_id: Uuid,

        /// Uuid of Earlbird player
        player_id: Uuid,

        /// Name of Earlbird player
        player_name: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        sub_event: SubEvent,
    },

    /// Added or removed an item as a result of a Tarot reading
    #[serde(rename_all = "camelCase")]
    TarotReadingAddedOrRemovedItem {
        /// Description of event
        description: String,

        /// Uuid of item that was gained/lost
        item_id: Uuid,

        /// Name of item that was gained/lost
        item_name: String,

        /// Mods bestowed by item that was gained/lost
        item_mods: Vec<String>,

        /// The increase/decrease that all the wielding player's items caused to their star rating
        /// before gaining/losing this item
        player_item_rating_before: f64,

        /// The increase/decrease that all the wielding player's items now cause to their star rating
        player_item_rating_after: f64,

        /// The player's star rating. TODO: Is this with or without items?
        player_rating: f64,

        /// Team Uuid of team who gained/lost the item
        team_id: Uuid,

        /// Uuid of player who gained/lost the item
        player_id: Uuid,

        /// True if the player gained the item, false otherwise
        item_gained: bool,
    },

    // TODO: Earlseason is separate Fed events for add and remove, but Middling is the same event.
    //   Choose one and stick with it.
    /// Player gains or loses Middling
    #[serde(rename_all = "camelCase")]
    PlayerMiddling {
        #[serde(flatten)]
        game: GameEvent,

        /// Whether this team just became Middling (true) or un-became Middling (false)
        is_middling: bool,

        #[serde(flatten)]
        change_event: ModChangeSubEventWithNamedPlayer,
    },

    /// Player gets an item from the Community Chest
    #[serde(rename_all = "camelCase")]
    CommunityChestOpens {
        /// Uuid of item that was gained
        item_id: Uuid,

        /// Name of item that was gained
        item_name: String,

        /// Mods bestowed by item that was gained
        item_mods: Vec<String>,

        /// The increase or decrease that all the wielding player's items caused to their star rating
        /// before gaining this item. Sometimes this is null for no reason I can discern.
        player_item_rating_before: Option<f64>,

        /// The increase or decrease that all the wielding player's items now cause to their star
        /// rating. Sometimes this is null for no reason I can discern.
        player_item_rating_after: Option<f64>,

        /// The player's star rating. TODO: Is this with or without items?
        player_rating: f64,

        /// Team Uuid of team who gained the item
        team_id: Uuid,

        /// Name of player who gained the item
        player_name: String,

        /// Uuid of player who gained the item
        player_id: Uuid,
    },

    /// Top-level "player lost item" event. I'm only aware of this happening as a result of the
    /// player getting a new item from the Community Chest, but it may happen from other sources.
    #[serde(rename_all = "camelCase")]
    PlayerDropsItem {
        /// Uuid of item that was gained
        item_id: Uuid,

        /// Name of item that was gained
        item_name: String,

        /// Mods bestowed by item that was gained
        item_mods: Vec<String>,

        /// The increase or decrease that all the wielding player's items caused to their star rating
        /// before gaining this item
        player_item_rating_before: f64,

        /// The increase or decrease that all the wielding player's items now cause to their star rating
        player_item_rating_after: f64,

        /// The player's star rating. TODO: Is this with or without items?
        player_rating: f64,

        /// Team Uuid of team who gained the item
        team_id: Uuid,

        /// Name of player who gained the item
        player_name: String,

        /// Uuid of player who gained the item
        player_id: Uuid,
    },

    /// The community chest announcement that appears during the game. Because community chests can 
    /// open when some teams aren't playing a game, and the players must still receive their items, 
    /// the events for receiving an item are separate from the event that appears in game.
    ///
    /// This event has very minimal data. If you want to process community chests you probably want
    /// to look for CommunityChestOpens events.
    #[serde(rename_all = "camelCase")]
    CommunityChestGameMessage {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the player who's listed first in the event. TODO: Is this in consistent order
        /// w/r/t home and away team?
        first_player_name: String,

        /// Name of the item that the first player received
        first_player_item_name: String,

        /// Name of the item that the first player dropped, if any. Otherwise null.
        first_player_dropped_item: Option<String>,

        /// Name of the player who's listed second in the event
        second_player_name: String,

        /// Name of the item that the second player received
        second_player_item_name: String,

        /// Name of the item that the second player dropped, if any. Otherwise null.
        second_player_dropped_item: Option<String>,
    },

    /// Late to the Party mod procs at the beginning of Lateseason
    #[serde(rename_all = "camelCase")]
    LateToThePartyAddedToPlayer {
        #[serde(flatten)]
        game: GameEvent,

        /// Team Uuid of Late to to the Party player
        team_id: Uuid,

        /// Uuid of Late to to the Party player
        player_id: Uuid,

        /// Name of Late to the Party player
        player_name: String,

        /// Metadata for the sub-event that adds the Overperforming mod
        sub_event: SubEvent,
    },

    /// Fax Machine activates
    #[serde(rename_all = "camelCase")]
    Fax {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of team whose pitchers faxed
        team_id: Uuid,

        /// Nickname of team whose pitchers faxed
        team_nickname: String,

        /// Uuid of pitcher who faxed out
        exiting_pitcher_id: Uuid,

        /// Name of pitcher who faxed out
        exiting_pitcher_name: String,

        /// Uuid of pitcher who faxed in
        entering_pitcher_id: Uuid,

        /// Name of pitcher who faxed in
        entering_pitcher_name: String,

        /// Before [todo: whichever season merged the shadows], which section of the shadows the
        /// player exited/entered.
        // TODO: Make None after shadows were merged?
        shadows_location: ShadowPositionType,

        /// Exiting pitcher's rating before the shadow boost
        rating_before: f64,

        /// Exiting pitcher's rating after the shadow boost
        rating_after: f64,

        /// Metadata for the sub-event associated with swapping the players
        player_swap_sub_event: SubEvent,

        /// Metadata for the sub-event associated with the shadow boost
        enter_shadows_sub_event: SubEvent,
    },

    /// A Redacted event
    #[serde(rename_all = "camelCase")]
    Redacted {
        /// Event description, which seems to contain only "|" characters and spaces
        description: String,

        /// Number of upscales. This is like nuts but for Redacted events
        scales: i64,
    },

    /// A Redacted event
    #[serde(rename_all = "camelCase")]
    Ambitious {
        #[serde(flatten)]
        game: GameEvent,

        /// True if the mod was added, false otherwise
        was_added: bool,

        #[serde(flatten)]
        mod_change: ModChangeSubEventWithNamedPlayer,
    },

    /// Late to the Party mod removed at the beginning of Postseason
    #[serde(rename_all = "camelCase")]
    LateToThePartyRemovedFromPlayer {
        #[serde(flatten)]
        game: GameEvent,

        /// Team Uuid of Late to to the Party player
        team_id: Uuid,

        /// Uuid of Late to to the Party player
        player_id: Uuid,

        /// Name of Late to the Party player
        player_name: String,

        /// Metadata for the sub-event that removes the Overperforming mod
        sub_event: SubEvent,
    },

    /// Smithy procs and repairs a player's item
    #[serde(rename_all = "camelCase")]
    Smithy {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        repair: ItemRepaired,
    },

    /// Holiday Inning is announced
    #[serde(rename_all = "camelCase")]
    HolidayInning {
        #[serde(flatten)]
        game: GameEvent,

        /// One-indexed inning number
        inning_number: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, WithStructure, IntoPrimitive, TryFromPrimitive)]
#[repr(i32)]
pub enum SimPhase {
    GodsDay = 0,
    Preseason = 1,
    Earlseason = 2,
    Earlsiesta = 3,
    Midseason = 4,
    Latesiesta = 5,
    Lateseason = 6,
    Endseason = 7,
    PrePostseason = 8,
    Earlpostseason = 9,
    EarlpostseasonEnd = 10,
    Latepostseason = 11,
    PostseasonEnd = 12,
    Election = 13,
    SpecialEvent = 14,
}

/// Represents the parsed data for any Feed event
#[derive(Clone, Debug, Builder, JsonSchema, Serialize, Deserialize, WithStructure, EnumFlatten)]
#[serde(rename_all = "camelCase")]
#[enum_flatten(data)]
pub struct FedEvent {
    /// Uuid of the event itself
    pub id: Uuid,

    /// Date the event occurred
    pub created: DateTime<Utc>,

    /// Which sim (or universe of Blaseball) this event came from
    ///
    /// Notable values are:
    ///
    /// - thisidisstaticyo: All of Beta, during which the ID was indeed static yo
    ///
    /// - gammaN: Any of the Short Circuits universes, including many that were generated by mistake
    ///   and never visible on the site. Non-empty gammas are gamma5 and gamma7, which just include
    ///   the event "SIM_GAMMA_LEAGUE became Non-Physical Law.",  and gamma8-gamma10, which were the
    ///   visible Short Circuits universes.
    ///
    /// Unfortunately, it seems that many events in Short Circuits were incorrectly assigned to the
    /// thisidisstaticyo sim.
    pub sim: String,

    /// In gamma10 in a Title Belt match, tournament indicates which match this is. Otherwise it is
    /// always -1.
    ///
    /// Previously, before the feed, tournament=0 was used in other API responses to indicate the
    /// Coffee Cup. It's unclear what, if anything, it will be used for in future.
    pub tournament: i32,

    /// Zero-indexed season
    pub season: i32,

    /// Zero-indexed day
    pub day: i32,

    /// Phase of the sim. Corresponds to the schedule section on the Blaseball homepage, with a few
    /// extra entries.
    pub phase: SimPhase,

    /// The number of times this event has been upshelled
    pub nuts: i32,

    /// The event type and specific event-specific data
    #[serde(flatten)]
    #[serde(with = "FedEventData")]
    pub data: FedEventData,
}

impl FedEventData {
    pub fn game(&self) -> Option<&GameEvent> {
        match self {
            FedEventData::BeingSpeech { .. } => { None }
            FedEventData::LetsGo { game, .. } => { Some(game) }
            FedEventData::PlayBall { game, .. } => { Some(game) }
            FedEventData::HalfInningStart { game, .. } => { Some(game) }
            FedEventData::BatterUp { game, .. } => { Some(game) }
            FedEventData::SuperyummyGameStart { game, .. } => { Some(game) }
            FedEventData::EchoedSuperyummyGameStart { game, .. } => { Some(game) }
            FedEventData::Ball { game, .. } => { Some(game) }
            FedEventData::FoulBall { game, .. } => { Some(game) }
            FedEventData::StrikeSwinging { game, .. } => { Some(game) }
            FedEventData::StrikeLooking { game, .. } => { Some(game) }
            FedEventData::StrikeFlinching { game, .. } => { Some(game) }
            FedEventData::Flyout { game, .. } => { Some(game) }
            FedEventData::GroundOut { game, .. } => { Some(game) }
            FedEventData::FieldersChoice { game, .. } => { Some(game) }
            FedEventData::DoublePlay { game, .. } => { Some(game) }
            FedEventData::Hit { game, .. } => { Some(game) }
            FedEventData::HomeRun { game, .. } => { Some(game) }
            FedEventData::StolenBase { game, .. } => { Some(game) }
            FedEventData::CaughtStealing { game, .. } => { Some(game) }
            FedEventData::StrikeoutSwinging { game, .. } => { Some(game) }
            FedEventData::StrikeoutLooking { game, .. } => { Some(game) }
            FedEventData::Walk { game, .. } => { Some(game) }
            FedEventData::InningEnd { game, .. } => { Some(game) }
            FedEventData::CharmStrikeout { game, .. } => { Some(game) }
            FedEventData::StrikeZapped { game, .. } => { Some(game) }
            FedEventData::PeanutFlavorText { game, .. } => { Some(game) }
            FedEventData::GameEnd { game, .. } => { Some(game) }
            FedEventData::MildPitch { game, .. } => { Some(game) }
            FedEventData::MildPitchWalk { game, .. } => { Some(game) }
            FedEventData::CoffeeBean { game, .. } => { Some(game) }
            FedEventData::BecameMagmatic { game, .. } => { Some(game) }
            FedEventData::Blooddrain { game, .. } => { Some(game) }
            FedEventData::SpecialBlooddrain { game, .. } => { Some(game) }
            FedEventData::PlayerModExpires { .. } => { None }
            FedEventData::TeamModExpires { .. } => { None }
            FedEventData::BirdsCircle { game, .. } => { Some(game) }
            FedEventData::AmbushedByCrows { game, .. } => { Some(game) }
            FedEventData::Sun2SetWin { .. } => { None }
            FedEventData::BlackHoleSwallowedWin { .. } => { None }
            FedEventData::Sun2 { game, .. } => { Some(game) }
            FedEventData::BlackHole { game, .. } => { Some(game) }
            FedEventData::TeamDidShame { .. } => { None }
            FedEventData::TeamWasShamed { .. } => { None }
            FedEventData::CharmWalk { game, .. } => { Some(game) }
            FedEventData::GainFreeRefill { game, .. } => { Some(game) }
            FedEventData::AllergicReaction { game, .. } => { Some(game) }
            FedEventData::PerkUp { game, .. } => { Some(game) }
            FedEventData::Feedback { game, .. } => { Some(game) }
            FedEventData::BestowReverberating { game, .. } => { Some(game) }
            FedEventData::Reverb { game, .. } => { Some(game) }
            FedEventData::TarotReading { .. } => { None }
            FedEventData::TarotReadingAddedOrRemovedMod { .. } => { None }
            FedEventData::TeamEnteredPartyTime { .. } => { None }
            FedEventData::BecomeTripleThreat { game, .. } => { Some(game) }
            FedEventData::UnderOver { game, .. } => { Some(game) }
            FedEventData::OverUnder { game, .. } => { Some(game) }
            FedEventData::TasteTheInfinite { game, .. } => { Some(game) }
            FedEventData::BatterSkipped { game, .. } => { Some(game) }
            FedEventData::FeedbackBlocked { game, .. } => { Some(game) }
            FedEventData::FlagPlanted { .. } => { None }
            FedEventData::EmergencyAlert { .. } => { None }
            FedEventData::TeamJoinedILB { .. } => { None }
            FedEventData::FloodingSwept { game, .. } => { Some(game) }
            FedEventData::ReturnFromElsewhere { game, .. } => { Some(game) }
            FedEventData::Incineration { game, .. } => { Some(game) }
            FedEventData::PitcherChange { game, .. } => { Some(game) }
            FedEventData::Party { game, .. } => { Some(game) }
            FedEventData::PlayerHatched { .. } => { None }
            FedEventData::PostseasonBirth { .. } => { None }
            FedEventData::FinalStandings { .. } => { None }
            FedEventData::TeamLeftPartyTimeForPostseason { .. } => { None }
            FedEventData::EarnedPostseasonSlot { .. } => { None }
            FedEventData::PostseasonAdvance { .. } => { None }
            FedEventData::PostseasonEliminated { .. } => { None }
            FedEventData::PlayerBoosted { .. } => { None }
            FedEventData::TeamWonInternetSeries { .. } => { None }
            FedEventData::BottomDwellers { .. } => { None }
            FedEventData::WillReceived { .. } => { None }
            FedEventData::BlessingWon { .. } => { None }
            FedEventData::EarlbirdsAddedToTeam { game, .. } => { Some(game) }
            FedEventData::DecreePassed { .. } => { None }
            FedEventData::PlayerJoinedILB { .. } => { None }
            FedEventData::PlayerPermittedToStay { .. } => { None }
            FedEventData::FireproofIncineration { game, .. } => { Some(game) }
            FedEventData::LineupSorted { .. } => { None }
            FedEventData::EarlbirdsRemovedFromTeam { game, .. } => { Some(game) }
            FedEventData::Undersea { game, .. } => { Some(game) }
            FedEventData::RenovationBuilt { .. } => { None }
            FedEventData::LateToThePartyAdded { game, .. } => { Some(game) }
            FedEventData::PeanutMister { game, .. } => { Some(game) }
            FedEventData::PlayerNamedMvp { .. } => { None }
            FedEventData::LateToThePartyRemoved { game, .. } => { Some(game) }
            FedEventData::BirdsUnshell { game, .. } => { Some(game) }
            FedEventData::ReplaceReturnedPlayerFromShadows { .. } => { None }
            FedEventData::PlayerCalledBackToHall { .. } => { None }
            FedEventData::TeamUsedFreeWill { .. } => { None }
            FedEventData::PlayerLostMod { .. } => { None }
            FedEventData::InvestigationMessage { .. } => { None }
            FedEventData::HighPressure { game, .. } => { Some(game) }
            FedEventData::PlayerPulledThroughRift { .. } => { None }
            FedEventData::PlayerLocalized { .. } => { None }
            FedEventData::Echo { game, .. } => { Some(game) }
            FedEventData::SolarPanelsAwait { game, .. } => { Some(game) }
            FedEventData::EchoIntoStatic { game, .. } => { Some(game) }
            FedEventData::Psychoacoustics { game, .. } => { Some(game) }
            FedEventData::EchoReceiver { game, .. } => { Some(game) }
            FedEventData::ConsumerAttack { game, .. } => { Some(game) }
            FedEventData::TeamGainedFreeWill { .. } => { None }
            FedEventData::Tidings { .. } => { None }
            FedEventData::HomebodyGameStart { game, .. } => { Some(game) }
            FedEventData::SalmonSwim { game, .. } => { Some(game) }
            FedEventData::HitByPitch { game, .. } => { Some(game) }
            FedEventData::SolarPanelsActivate { game, .. } => { Some(game) }
            FedEventData::RunsOverflowing { game, .. } => { Some(game) }
            FedEventData::TeamMiddling { game, .. } => { Some(game) }
            FedEventData::EnterCrimeScene { game, .. } => { Some(game) }
            FedEventData::ReturnFromInvestigation { .. } => { None }
            FedEventData::InvestigationConcluded { .. } => { None }
            FedEventData::GrindRail { game, .. } => { Some(game) }
            FedEventData::EnterSecretBase { game, .. } => { Some(game) }
            FedEventData::ExitSecretBase { game, .. } => { Some(game) }
            FedEventData::EchoChamber { game, .. } => { Some(game) }
            FedEventData::Roam { .. } => { None }
            FedEventData::GlitterCrate { game, .. } => { Some(game) }
            FedEventData::ModsFromAnotherModRemoved { .. } => { None }
            FedEventData::ConsumerExpelled { game, .. } => { Some(game) }
            FedEventData::EarlbirdsAddedToPlayer { game, .. } => { Some(game) }
            FedEventData::MindTrickWalk { game, .. } => { Some(game) }
            FedEventData::MindTrickStrikeout { game, .. } => { Some(game) }
            FedEventData::BlooddrainBlocked { game, .. } => { Some(game) }
            FedEventData::EarlbirdsRemovedFromPlayer { game, .. } => { Some(game) }
            FedEventData::TarotReadingAddedOrRemovedItem { .. } => { None }
            FedEventData::PlayerMiddling { game, .. } => { Some(game) }
            FedEventData::CommunityChestOpens { .. } => { None }
            FedEventData::PlayerDropsItem { .. } => { None }
            FedEventData::CommunityChestGameMessage { game, .. } => { Some(game) }
        }
    }
}

impl Eq for FedEvent {}

impl PartialEq<Self> for FedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.created.eq(&other.created)
    }
}

impl PartialOrd<Self> for FedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.created.partial_cmp(&other.created)
    }
}

impl Ord for FedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.created.cmp(&other.created)
    }
}