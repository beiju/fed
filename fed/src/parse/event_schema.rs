use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Write};
use std::iter;
use chrono::{DateTime, Utc};
use enum_access::EnumDisplay;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use eventually_api::{EventMetadata, EventType, EventCategory, EventuallyEvent, Weather};
use num_enum::{IntoPrimitive, TryFromPrimitive, TryFromPrimitiveError};
use derive_builder::Builder;
use schemars::JsonSchema;
use strum_macros::AsRefStr;
use with_structure::WithStructure;
use with_structure_derive::WithStructure;
use enum_flatten_derive::{EnumFlatten, EnumFlattenable};

use crate::parse::error::FeedParseError;
use crate::parse::builder::*;
use crate::parse::event_builder_new::{EventBuilder, Possessive};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, IntoPrimitive, TryFromPrimitive)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Unscatter {
    pub sub_event: SubEvent,
    pub team_id: Uuid,
    pub player_id: Uuid,
    pub player_name: String,
}

/// Game data. Every game event has one of these.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    pub unscatter: Option<Unscatter>,

    /// If an Attractor entered the Secret Base on this tick, contains information about this player
    pub attractor_secret_base: Option<PlayerInfo>,
}

/// Pitch data. The normal-baseball game events all have one of these.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GamePitch {
    /// If a Double Strike was fired, the name of the pitcher who fired it. Otherwise null.
    pub double_strike: Option<String>,
}

// impl GameEvent {
//     pub fn try_from_event(event: &EventuallyEvent, unscatter: Option<Unscatter>, attractor_secret_base: Option<PlayerInfo>) -> Result<Self, FeedParseError> {
//         let (&game_id, ) = event.game_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "game" })?;
//
//         // Order is very important here
//         let (&away_team, &home_team) = event.team_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "team" })?;
//
//         Self::try_from_event_with_teams(event, unscatter, attractor_secret_base, game_id, away_team, home_team)
//     }
//
//     pub fn try_from_event_extra_teams(event: &EventuallyEvent, unscatter: Option<Unscatter>, attractor_secret_base: Option<PlayerInfo>) -> Result<Self, FeedParseError> {
//         let (&game_id, ) = event.game_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "game" })?;
//
//         // Order is very important here. Apparently game end events have extra teams?
//         let (&away_team, &home_team, &home_team2, &away_team2) = event.team_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "team" })?;
//
//         assert_eq!(away_team, away_team2);
//         assert_eq!(home_team, home_team2);
//
//         Self::try_from_event_with_teams(event, unscatter, attractor_secret_base, game_id, away_team, home_team)
//     }
//
//     fn try_from_event_with_teams(event: &EventuallyEvent, unscatter: Option<Unscatter>, attractor_secret_base: Option<PlayerInfo>, game_id: Uuid, away_team: Uuid, home_team: Uuid) -> Result<Self, FeedParseError> {
//         Ok(Self {
//             game_id,
//             home_team,
//             away_team,
//             play: event.metadata.play
//                 .ok_or_else(|| {
//                     FeedParseError::MissingMetadata {
//                         event_type: event.r#type,
//                         field: "play",
//                     }
//                 })?,
//             unscatter,
//             attractor_secret_base,
//         })
//     }
// }

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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    /// Info for the score that happened on this event, if any, otherwise null
    pub score: Option<ScoringPlayer>,

    /// List of free refills used on this event, if any. This should always be empty if `score` is
    /// null, but if `scores` is non-null it may contain more than one element.
    pub free_refills: Vec<FreeRefill>,
}

impl Score {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AttrCategory {
    Batting,
    Pitching,
    Defense,
    Baserunning,
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

impl AttrCategory {
    pub fn metadata_type(&self) -> i32 {
        match self {
            AttrCategory::Batting => { 0 }
            AttrCategory::Pitching => { 1 }
            AttrCategory::Defense => { 2 }
            AttrCategory::Baserunning => { 3 }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, WithStructure, TryFromPrimitive, IntoPrimitive)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
#[serde(rename_all = "camelCase")]
pub enum ShadowPositionType {
    Bench = 2,
    Bullpen = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive, WithStructure)]
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


#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPlayerData {
    pub team_id: Uuid,
    pub team_nickname: String,
    pub player_id: Uuid,
    pub player_name: String,
    pub location: ActivePositionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerReverb {
    /// Uuid of the first player involved in this reverb
    pub first_player_id: Uuid,

    /// Name of the first player involved in this reverb
    pub first_player_name: String,

    /// New location (lineup or rotation) of the first player involved in this reverb. Also the 
    /// previous location of the second player in the reverb.
    pub first_player_new_location: ActivePositionType,

    /// Uuid of the second player involved in this reverb
    pub second_player_id: Uuid,

    /// Name of the second player involved in this reverb
    pub second_player_name: String,

    /// New location (lineup or rotation) of the second player involved in this reverb. Also the 
    /// previous location of the second player in the reverb.
    pub second_player_new_location: ActivePositionType,

    /// Metadata associated with the player swap sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
// This uses a combo of flatten and adjacent tagging
#[serde(rename_all = "camelCase", tag = "type", content = "subEvent")]
pub enum ReverbType {
    Rotation(SubEvent),
    Lineup(SubEvent),
    Full(SubEvent),
    SeveralPlayers(Vec<PlayerReverb>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BatterSkippedReason {
    /// Batter is Shelled
    Shelled,

    /// Batter is Elsewhere
    ///
    /// For whatever reason, this has a player_id while the Shelled variant does not
    Elsewhere(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[repr(i64)]
pub enum StatChangeCategory {
    Batting = 0,
    Pitching = 1,
    Baserunning = 2,
    Defense = 3,
    All = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,
}

// This is identical to PlayerInfo except for field names. It's used for JSON schema reasons
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PitcherInfo {
    /// Pitcher uuid
    pub pitcher_id: Uuid,

    /// Pitcher name
    pub pitcher_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Scattered {
    /// Name of player after being Scattered
    pub scattered_name: String,

    /// Sub-event associated with adding the Scattered mod
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FloodingSweptEffect {
    Elsewhere(ModChangeSubEventWithNamedPlayer),
    Flippers(PlayerInfo),
    Ego(PlayerInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, rename_all = "camelCase")]
pub enum RenovationVotes {
    Normal(i64),
    Manual(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MultipleModsAddedOrRemoved {
    /// Vector of mods that were added/removed. Each mod is represented by its internal ID.
    pub mod_ids: Vec<String>,

    /// Metadata for the event associated with adding or removing these mods
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr)]
#[serde(tag = "time_elsewhere_type", content = "time_elsewhere", rename_all = "camelCase")]
pub enum TimeElsewhere {
    Days(i32),
    Seasons(i32),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr)]
#[serde(tag = "flavor", rename_all = "camelCase")]
pub enum ReturnFromElsewhereFlavor {
    /// The normal one
    #[serde(rename_all = "camelCase")]
    Full {
        /// Team uuid of player who returned from Elsewhere
        team_id: Uuid,

        /// Uuid of player who returned from Elsewhere
        player_id: Uuid,

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

        /// Metadata for sub-event associated with removing the Elsewhere mod
        sub_event: SubEvent,
    },
    /// Fake returns from elsewhere. As far as I know this only happens when a Receiver returns from
    /// Elsewhere after being sent there by Receiving Elsewhere from an Echo. There's no extra data
    /// on a false return from elsewhere.
    False,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetectiveActivity {
    /// Uuid of the detective
    pub detective_id: Uuid,

    /// Name of the detective
    pub detective_name: String,

    /// Metadata for the sub-event associated with the detective activity
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

// This event is intended for situations where the player's id and team are already encoded in a
// ItemGained struct. If a player
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Carcinization {
    #[serde(flatten)]
    pub mv: PlayerMovedTeams,

    /// Full name of player's new team
    pub new_team_name: String,

    /// Metadata for sub-event associated with adding the TEMP_STOLEN mod
    pub mod_added_sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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
}

// Use this in contexts where the player name and ID are stored outside
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Attraction {
    /// Nickname of team who attracted this player
    pub team_nickname: String,

    /// Uuid of team who attracted this player
    pub team_id: Uuid,

    /// Metadata about the player being added to the team
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModDesc {
    /// Internal name of the mod
    pub mod_id: String,

    /// Duration of the mod
    pub mod_duration: ModDuration,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(tag = "hitType", content = "chargeBlood")]
pub enum HitType {
    Single,
    Double,
    Triple(Option<ModChangeSubEvent>),
    Quadruple,
}

impl Display for HitType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HitType::Single => { write!(f, "Single") }
            HitType::Double => { write!(f, "Double") }
            HitType::Triple(_) => { write!(f, "Triple") }
            HitType::Quadruple => { write!(f, "Quadruple") }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
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

        /// Batter's team's name
        team_name: String,

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

        /// Which base the runner was tagged out on. First base is `1`, second is `2`, etc.
        out_at_base: i32,

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

        /// If this is a Magmatic home run, the metadata for the event where the batter loses the
        /// Magmatic mod, otherwise null
        magmatic: Option<ModChangeSubEvent>,

        /// Name of the batter who hit the home run
        batter_name: String,

        /// Uuid of the batter who hit the home run
        batter_id: Uuid,

        /// Number of players who made it home during this home run (minimum 1)
        num_runs: i32,

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
        base_stolen: i32,

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
        base_stolen: i32,
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

        /// If the batter went to a later base with Base Instincts, this is the base number.
        /// Otherwise null.
        base_instincts: Option<i32>,

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

        /// Team uuid of player who became magmatic
        team_id: Uuid,

        /// Metadata of sub-event associated with player gaining the Magmatic mod
        mod_add_event: SubEvent,
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
        friend_of_crows: Option<PitcherInfo>,
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
        gravity_players: Vec<PlayerInfo>,
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

    /// Team gains or loses Middling
    #[serde(rename_all = "camelCase")]
    Middling {
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

        /// Team uuid of the player who was made Repeating
        team_id: Uuid,

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

        // /// Meta about the pitcher's item breaking, if it broke, otherwise null.
        // pitcher_item_damage: Option<ItemDamaged>,
        //
        // /// Meta about the batter's item breaking, if it broke, otherwise null.
        // batter_item_damage: Option<ItemDamaged>,
        //
        // #[serde(flatten)]
        // scores: Scores,
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
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, WithStructure, IntoPrimitive, TryFromPrimitive)]
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

trait GameEventForBuilder {
    fn for_game(self, game: &GameEvent) -> Self;
    fn for_sub_event(self, sub: &SubEvent) -> Self;
}

fn make_switch_performing_child(toggle: &TogglePerforming, description: &str, mod_source: &str) -> EventBuilderChildFull {
    let mod_name = if toggle.is_overperforming { "OVERPERFORMING" } else { "UNDERPERFORMING" };
    let opposite_mod_name = if toggle.is_overperforming { "UNDERPERFORMING" } else { "OVERPERFORMING" };
    if toggle.is_first_proc {
        EventBuilderChild::new(&toggle.sub_event)
            .update(EventBuilderUpdate {
                category: EventCategory::Changes,
                r#type: EventType::AddedModFromOtherMod,
                description: description.to_string(),
                team_tags: vec![toggle.team_id],
                player_tags: vec![toggle.player_id],
                ..Default::default()
            })
            .metadata(json!({
                "mod": mod_name,
                "source": mod_source,
                "type": 0, // ?
            }))
    } else {
        EventBuilderChild::new(&toggle.sub_event)
            .update(EventBuilderUpdate {
                r#type: EventType::ChangedModFromOtherMod,
                category: EventCategory::Changes,
                description: description.to_string(),
                team_tags: vec![toggle.team_id],
                player_tags: vec![toggle.player_id],
                ..Default::default()
            })
            .metadata(json!({
                "from": opposite_mod_name,
                "source": mod_source,
                "to": mod_name,
                "type": 0, // ?
            }))
    }
}

fn apply_batter_debt(batter_debt: &Option<BatterDebt>, batter_name: &str, fielder_name: &str) -> (String, Option<EventBuilderChildFull>, Vec<Uuid>) {
    let suffix = if batter_debt.is_some() {
        format!("\n{batter_name} hit a ball at {fielder_name}...\n{fielder_name} is now being Observed.")
    } else {
        String::new()
    };

    let observed_child = batter_debt.as_ref().and_then(|debt| {
        debt.sub_event.as_ref().map(|sub_event| {
            EventBuilderChild::new(&sub_event.sub_event)
                .update(EventBuilderUpdate {
                    r#type: EventType::AddedMod,
                    category: EventCategory::Changes,
                    description: format!("{fielder_name} is now being Observed."),
                    player_tags: vec![debt.fielder_id],
                    team_tags: vec![sub_event.team_id],
                    ..Default::default()
                })
                .metadata(json!({
                                "mod": "COFFEE_PERIL",
                                "type": 2, // ?
                            }))
        })
    });

    let player_tags = if let Some(debt) = batter_debt {
        vec![debt.batter_id, debt.fielder_id]
    } else {
        vec![]
    };

    (suffix, observed_child, player_tags)
}


impl FedEvent {
    pub fn into_feed_event(self) -> EventuallyEvent {
        let event_builder = EventBuilderCommon {
            id: self.id,
            created: self.created,
            sim: self.sim.clone(),
            day: self.day,
            phase: self.phase.into(),
            season: self.season,
            tournament: self.tournament,
            nuts: self.nuts,
        };

        let mut eb = EventBuilder::new(
            self.id,
            self.created,
            self.sim.clone(),
            self.day,
            self.season,
            self.tournament,
            self.phase.into(),
            self.nuts,
        );

        match self.data {
            FedEventData::BeingSpeech { being, message } => {
                let being_id: i32 = being.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BigDeal,
                        category: EventCategory::Narrative,
                        description: message,
                        ..Default::default()
                    })
                    .metadata(json!({ "being": being_id }))
                    .build()
            }
            FedEventData::LetsGo { game, weather, stadium_id } => {
                let weather_id: i32 = weather.into();
                let mut metadata = json!({
                    "home": game.home_team,
                    "away": game.away_team,
                    "weather": weather_id,
                });
                if let Some(id) = stadium_id {
                    metadata["stadium"] = json!(id);
                }
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::LetsGo,
                        description: "Let's Go!".to_string(),
                        ..Default::default()
                    })
                    .metadata(metadata)
                    .build()
            }
            FedEventData::PlayBall { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayBall,
                        description: "Play ball!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name, subseasonal_mod_effects } => {
                eb.set_game(game);
                for effect in subseasonal_mod_effects {
                    let description = if effect.was_added {
                        format!("The {} are {}.", effect.team_nickname, effect.source_mod_name)
                    } else {
                        format!("{} are no longer {}.", effect.team_nickname, effect.source_mod_name)
                    };
                    eb.push_description(&description);
                    eb.push_child(effect.sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_team_tag(effect.team_id);
                        child.push_metadata_str("mod", match effect.source_mod_id.as_str() {
                            "MIDDLING" | "LATE_TO_PARTY" => "OVERPERFORMING",
                            "EARLY_TO_PARTY" => "UNDERPERFORMING",
                            other => panic!("Unexpected mod in TeamPerformingChanged: '{other}'")
                        });
                        child.push_metadata_str("source", effect.source_mod_id);
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(if effect.was_added {
                            EventType::AddedModFromOtherMod
                        } else {
                            EventType::RemovedModFromOtherMod
                        })
                    })
                }
                eb.push_description(&format!("{} of {inning}, {batting_team_name} batting.",
                                             if top_of_inning { "Top" } else { "Bottom" }));
                eb.build(EventType::HalfInning)
            }
            FedEventData::BatterUp { ref game, ref batter_name, ref team_name, ref wielding_item, ref inhabiting, is_repeating } => {
                let item_suffix = if let Some(item_name) = wielding_item {
                    format!(", wielding {}", item_name)
                } else {
                    String::default()
                };

                let prefix = if is_repeating {
                    format!("{batter_name} is Repeating!\n")
                } else {
                    String::default()
                };

                let inhabiting_child = inhabiting.as_ref()
                    .and_then(|inhabiting| {
                        inhabiting.sub_event.as_ref().map(|sub_event|
                            EventBuilderChild::new(sub_event)
                                .update(EventBuilderUpdate {
                                    r#type: EventType::AddedMod,
                                    category: EventCategory::Changes,
                                    description: format!("{} is Inhabiting {}!",
                                                         batter_name, inhabiting.inhabited_player_name),
                                    player_tags: vec![inhabiting.inhabiting_player_id],
                                    team_tags: inhabiting.inhabiting_player_team_id.iter().cloned().collect(),
                                    ..Default::default()
                                })
                                .metadata(json!({
                                    "mod": "INHABITING",
                                    "type": 0, // ?
                                }))
                        )
                    });

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BatterUp,
                        category: EventCategory::special_if(inhabiting.is_some() || is_repeating),
                        description: if let Some(inhabiting) = &inhabiting {
                            format!("{prefix}{batter_name} is Inhabiting {}!\n{batter_name} batting for the {team_name}{item_suffix}.",
                                    inhabiting.inhabited_player_name)
                        } else {
                            format!("{prefix}{batter_name} batting for the {team_name}{item_suffix}.")
                        },
                        player_tags: if let Some(inhabiting) = inhabiting {
                            vec![inhabiting.inhabiting_player_id, inhabiting.inhabited_player_id]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    })
                    .children(inhabiting_child)
                    .build()
            }
            FedEventData::SuperyummyGameStart { ref game, ref toggle } => {
                let description = format!("{} {} Peanuts.", toggle.player_name,
                                          if toggle.is_overperforming { "loves" } else { "misses" });
                let change_event = make_switch_performing_child(toggle, &description, "SUPERYUMMY");
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Superyummy,
                        description,
                        ..Default::default()
                    })
                    .child(change_event)
                    .build()
            }
            FedEventData::EchoedSuperyummyGameStart { ref game, ref player_name, peanuts_present: peanuts } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Superyummy,
                        description: format!("{} {} Peanuts.", player_name,
                                             if peanuts { "loves" } else { "misses" }),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Ball { game, balls, strikes, batter_item_damage } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Ball,
                        description: format!("Ball. {}-{}", balls, strikes),
                        ..Default::default()
                    })
                    .named_item_damage_before_score(batter_item_damage.as_ref())
                    .build()
            }
            FedEventData::StrikeSwinging { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike, swinging. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeLooking { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike, looking. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeFlinching { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike, flinching. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.build(EventType::Strike)
            }
            FedEventData::FoulBall { game, pitch, balls, strikes, batter_item_damage, birds } => {
                eb.set_game(game);
                let foul_ball_text = if pitch.double_strike.is_some() {
                    eb.set_category(EventCategory::Special);
                    "Foul Balls"
                } else {
                    "Foul Ball"
                };
                eb.push_pitch(pitch);
                eb.push_description(&format!("{foul_ball_text}. {balls}-{strikes}"));
                eb.push_named_item_damage(batter_item_damage);
                eb.push_birds(birds);
                eb.build(EventType::FoulBall)
            }
            FedEventData::Flyout { game, pitch, batter_name, fielder_name, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, fielder_item_damage, other_player_item_damage, parasite, } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} hit a flyout to {fielder_name}."));
                eb.push_item_damage(batter_item_damage, &batter_name);
                eb.push_item_damage(fielder_item_damage, &fielder_name);
                eb.push_named_item_damage(other_player_item_damage);
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                eb.push_scores(scores, "tags up and scores!");
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_parasite(parasite);
                eb.build(EventType::FlyOut)
                // let (suffix, observed_child, player_tags) = apply_batter_debt(&batter_debt, &batter_name, &fielder_name);
                //
                // event_builder.for_game(&game)
                //     .fill(EventBuilderUpdate {
                //         r#type: ,
                //         category: EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special),
                //         description: format!("{batter_name} hit a flyout to {fielder_name}.{suffix}"),
                //         player_tags,
                //         ..Default::default()
                //     })
                //     .scores(&scores, " tags up and scores!")
                //     .stopped_inhabiting(&stopped_inhabiting)
                //     .cooled_off(&cooled_off, &batter_name)
                //     .children(observed_child) // slight abuse of IntoIter
                //     .item_damage_before_score(&batter_item_damage, &batter_name)
                //     .item_damage_before_score(&fielder_item_damage, &fielder_name)
                //     .named_item_damage_before_score(&other_player_item_damage)
                //     .build()
            }
            FedEventData::Hit { game, pitch, batter_name, batter_id, hit_type, scores, spicy_status, stopped_inhabiting, is_special, pitcher_item_damage, batter_item_damage, other_player_item_damage } => {
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.push_item_damage(batter_item_damage, &batter_name);
                eb.push_description(&format!("{batter_name} hits a {hit_type}!"));
                eb.push_player_tag(batter_id);
                match hit_type {
                    HitType::Triple(power_charge) => {
                        eb.push_charge_blood(power_charge, &batter_name, batter_id, "aaa");
                    }
                    _ => {}
                }
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_scores(scores, "scores!");
                eb.push_spicy(spicy_status, &batter_name, batter_id);
                eb.push_named_item_damage(other_player_item_damage);

                eb.build(EventType::Hit)
            }
            FedEventData::HomeRun { ref game, ref magmatic, ref batter_name, batter_id, num_runs, ref free_refills, ref spicy_status, ref stopped_inhabiting, is_special, big_bucket, attraction, damaged_items } => {
                let mut suffix = String::new();
                let mut player_tags = vec![batter_id];
                if big_bucket {
                    write!(suffix, "\nThe ball lands in a Big Bucket. An extra Run scores!").unwrap();
                }
                for free_refill in free_refills {
                    write!(suffix, "\n{} used their Free Refill.\n{} Refills the In!",
                           free_refill.player_name, free_refill.player_name).unwrap();
                }

                let attraction_child = attraction.map(|attraction| {
                    write!(suffix, "\nThe {} Attract {}!", attraction.team_nickname, attraction.player_name).unwrap();
                    player_tags.push(attraction.player_id);
                    EventBuilderChild::new(&attraction.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::PlayerAddedToTeam,
                            category: EventCategory::Changes,
                            description: format!("The {} Attracted {}!", attraction.team_nickname, attraction.player_name),
                            player_tags: vec![attraction.player_id],
                            team_tags: vec![attraction.team_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "location": 2,
                            "playerId": attraction.player_id,
                            "playerName": attraction.player_name,
                            "teamId": attraction.team_id,
                            "teamName": attraction.team_nickname,
                        }))
                });

                let free_refill_children: Vec<_> = free_refills.iter()
                    .map(make_free_refill_child)
                    .collect();

                let magmagic_child = if let Some(ModChangeSubEvent { sub_event, team_id }) = magmatic {
                    Some(EventBuilderChild::new(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::RemovedMod,
                            category: EventCategory::Changes,
                            description: format!("{batter_name} hit a Magmatic home run!"),
                            player_tags: vec![batter_id],
                            team_tags: vec![*team_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": "MAGMATIC",
                            "type": 0, // ?
                        })))
                } else {
                    None
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::HomeRun,
                        category: EventCategory::special_if(magmatic.is_some() || !free_refills.is_empty() || spicy_status.is_special() || is_special),
                        description: format!("{}{batter_name} hits a {}!{suffix}",
                                             if magmatic.is_some() { format!("{batter_name} is Magmatic!\n") } else { String::new() },
                                             match num_runs {
                                                 1 => "solo home run",
                                                 2 => "2-run home run",
                                                 3 => "3-run home run",
                                                 4 => "grand slam",
                                                 // TODO Turn this into a Result error
                                                 _ => panic!("Unknown num runs in home run")
                                             }),
                        player_tags,
                        ..Default::default()
                    })
                    .stopped_inhabiting(stopped_inhabiting)
                    .spicy(spicy_status, batter_id, batter_name)
                    .children(attraction_child)
                    .children(free_refill_children)
                    .children(magmagic_child)
                    .named_item_damage_before_event(&damaged_items)
                    .build()
            }
            FedEventData::GroundOut { game, pitch, batter_name, fielder_name, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, pitcher_item_damage, fielder_item_damage } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} hit a ground out to {fielder_name}."));
                eb.push_scores(scores, "advances on the sacrifice.");
                // Per resim, it's definitely pitcher-batter-fielder in that order. It's also
                // definitely somewhere after scores. Rest of the order is not yet known
                eb.push_named_item_damage(pitcher_item_damage);
                eb.push_item_damage(batter_item_damage, &batter_name);
                eb.push_item_damage(fielder_item_damage, &fielder_name);
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                eb.build(EventType::GroundOut)
            }
            FedEventData::StolenBase { ref game, ref runner_name, runner_id, base_stolen, blaserunning, ref free_refill, ref runner_item_damage, is_special } => {
                let blaserunning_str = if blaserunning {
                    format!("\n{} scores with Blaserunning!", runner_name)
                } else {
                    String::new()
                };

                let free_refill_str = if let Some(free_refill) = free_refill {
                    format!("\n{} used their Free Refill.\n{} Refills the In!",
                            free_refill.player_name, free_refill.player_name)
                } else {
                    String::new()
                };
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::StolenBase,
                        category: EventCategory::special_if(blaserunning || free_refill.is_some() || is_special),
                        description: format!("{} steals {} base!{}{}", runner_name, base_name(base_stolen), blaserunning_str, free_refill_str),
                        player_tags: if blaserunning { vec![runner_id, runner_id] } else { vec![runner_id] },
                        ..Default::default()
                    })
                    .children(
                        free_refill.as_ref()
                            .map(|free_refill| make_free_refill_child(free_refill))
                            .into_iter()
                    )
                    .item_damage_after_score(runner_item_damage, runner_name)
                    .build()
            }
            FedEventData::StrikeoutSwinging { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{} strikes out swinging.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.build(EventType::Strikeout)
            }
            FedEventData::StrikeoutLooking { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{} strikes out looking.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage);
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.build(EventType::Strikeout)
            }
            FedEventData::Walk { game, batter_name, batter_id, scores, base_instincts, batter_item_damage, stopped_inhabiting, is_special } => {
                let base_instincts_str = if let Some(base) = base_instincts {
                    format!("\nBase Instincts take them directly to {} base!", base_name(base))
                } else {
                    String::new()
                };

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Walk,
                        category: EventCategory::special_if(scores.used_refill() || base_instincts.is_some() || is_special),
                        description: format!("{batter_name} draws a walk.{base_instincts_str}"),
                        player_tags: vec![batter_id],
                        ..Default::default()
                    })
                    .scores(&scores, " scores!")
                    .stopped_inhabiting(&stopped_inhabiting)
                    .item_damage_before_score(&batter_item_damage, &batter_name)
                    .build()
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::StolenBase,
                        description: format!("{} gets caught stealing {} base.", runner_name, base_name(base_stolen)),
                        player_tags: vec![],
                        team_tags: vec![],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::InningEnd { ref game, inning_num, ref lost_triple_threat } => {
                let (children, suffix) = self.make_mod_change_sub_events(lost_triple_threat, EventType::RemovedMod, "is no longer a Triple Threat.", "TRIPLE_THREAT");

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::InningEnd,
                        description: format!("Inning {inning_num} is now an Outing.{suffix}"),
                        player_tags: lost_triple_threat.iter().map(|e| e.player_id).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::CharmStrikeout { game, charmer_id, charmer_name, charmed_id, charmed_name, num_swings } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Strikeout,
                        category: EventCategory::Special,
                        description: format!("{charmer_name} charmed {charmed_name}!\n{charmed_name} swings {num_swings} times to strike out willingly!"),
                        // I do not know why the charmer appears twice, but that seems to be accurate
                        player_tags: vec![charmer_id, charmer_id, charmed_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FieldersChoice { ref game, ref pitch, ref batter_name, ref runner_out_name, out_at_base, ref scores, ref stopped_inhabiting, ref cooled_off, is_special } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special),
                        description: format!("{runner_out_name} out at {} base.",
                                             base_name(out_at_base)),
                        description_after_score: format!("\n{batter_name} reaches on fielder's choice."),
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .stopped_inhabiting(stopped_inhabiting)
                    .cooled_off(cooled_off, batter_name)
                    .build()
            }
            FedEventData::StrikeZapped { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::StrikeZapped,
                        category: EventCategory::Special,
                        description: "The Electricity zaps a strike away!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PeanutFlavorText { game, message } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PeanutFlavorText,
                        category: EventCategory::Special,
                        description: message,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::DoublePlay { ref game, ref pitch, ref batter_name, ref scores, ref stopped_inhabiting, ref cooled_off } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::special_if(scores.used_refill() || cooled_off.is_some()),
                        description: format!("{batter_name} hit into a double play!"),
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .stopped_inhabiting(stopped_inhabiting)
                    .cooled_off(cooled_off, batter_name)
                    .build()
            }
            FedEventData::GameEnd { game, winner_id, winning_team_name, winning_team_score, losing_team_name, losing_team_score, temp_stolen_player_returned } => {
                let child = temp_stolen_player_returned
                    .map(|ret| {
                        EventBuilderChild::new(&ret.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::PlayerMoved,
                                category: EventCategory::Changes,
                                description: format!("{} is returned to the {}.",
                                                     ret.player_name, ret.new_team_nickname),
                                player_tags: vec![ret.player_id],
                                team_tags: vec![ret.previous_team_id, ret.new_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "location": ret.location as i64,
                                "playerId": ret.player_id,
                                "playerName": ret.player_name,
                                "receiveLocation": ret.location as i64,
                                "receiveTeamId": ret.new_team_id,
                                "receiveTeamName": ret.new_team_nickname,
                                "sendTeamId": ret.previous_team_id,
                                "sendTeamName": ret.previous_team_nickname,
                            }))
                    });
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GameEnd,
                        category: EventCategory::Outcomes,
                        description: format!("{winning_team_name} {winning_team_score}, {losing_team_name} {losing_team_score}"),
                        team_tags: vec![game.home_team, game.away_team],
                        // This is the default value but I'm stating it just to convey that yes, I
                        // do mean to repeat the team tags. That's how the events are.
                        override_team_tags: false,
                        ..Default::default()
                    })
                    .metadata(json!({ "winner": winner_id }))
                    .children(child)
                    .build()
            }
            FedEventData::MildPitch { ref game, pitcher_id, ref pitcher_name, balls, strikes, runners_advance, ref scores } => {
                let runners_advance_str = if runners_advance {
                    "\nRunners advance on the pathetic play!"
                } else {
                    ""
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\nBall, {balls}-{strikes}.{runners_advance_str}"),
                        player_tags: vec![pitcher_id],
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .build()
            }
            FedEventData::CoffeeBean { ref game, player_id, ref player_name, ref roast, ref notes, ref which_mod, gained_mod, ref sub_event, team_id, ref previous } => {
                let change_str = match (gained_mod, which_mod) {
                    (true, CoffeeBeanMod::Wired) => { "is Wired!" }
                    (true, CoffeeBeanMod::Tired) => { "is Tired." }
                    (false, CoffeeBeanMod::Wired) => { "is no longer Wired." }
                    (false, CoffeeBeanMod::Tired) => { "is no longer Tired!" }
                };
                let mod_id = which_mod.to_str();
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: if previous.is_some() {
                            EventType::ModChange
                        } else if gained_mod {
                            EventType::AddedMod
                        } else {
                            EventType::RemovedMod
                        },
                        category: EventCategory::Changes,
                        description: format!("{player_name} {change_str}"),
                        team_tags: team_id.into_iter().collect(),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(
                        if let Some(prev_mod) = previous {
                            let prev_mod_id = prev_mod.to_str();
                            json!({
                                "from": prev_mod_id,
                                "to": mod_id,
                                "type": 3, // ?
                            })
                        } else {
                            json!({
                                "mod": mod_id,
                                "type": 3, // ?
                            })
                        }
                    );

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::CoffeeBean,
                        category: EventCategory::Special,
                        description: format!("{player_name} is Beaned by a {roast} roast with {notes}.\n{player_name} {change_str}"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BecameMagmatic { ref game, player_id, ref player_name, team_id, ref mod_add_event } => {
                let child = EventBuilderChild::new(mod_add_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} ate some flame.", ),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "MAGMATIC",
                        "type": 0, // ?
                    }));
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::IncinerationBlocked,
                        category: EventCategory::Special,
                        description: format!("Rogue Umpire tried to incinerate {player_name}, but {player_name} ate the flame! They became Magmatic!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::SpecialBlooddrain { ref game, sipper_id, ref sipper_name, sipped_id, sipped_team_id, ref sipped_name, ref sipped_category, ref action, ref sipped_event, rating_before, rating_after } => {
                let child = EventBuilderChild::new(sipped_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerStatDecrease,
                        category: EventCategory::Changes,
                        description: format!("{sipped_name} had blood drained by {sipper_name}."),
                        team_tags: vec![sipped_team_id],
                        player_tags: vec![sipped_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "type": sipped_category.metadata_type(), // ?
                        "before": rating_before,
                        "after": rating_after,
                    }));
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BlooddrainSiphon,
                        category: EventCategory::Special,
                        description: format!("The Blooddrain gurgled!\n{sipper_name}'s Siphon activates!\n{sipper_name} siphoned some of {sipped_name}'s {sipped_category} ability!\n{sipper_name} {action}"),
                        player_tags: vec![sipper_id, sipped_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerModExpires { team_id, player_id, player_name, mods, mod_duration } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ModExpires,
                        category: EventCategory::Changes,
                        description: format!("{} {} mods wore off.", possessive(player_name), mod_duration.to_string()),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mods": mods,
                        "type": mod_duration as i32
                    }))
                    .build()
            }
            FedEventData::TeamModExpires { team_id, team_nickname, mods, mod_duration } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ModExpires,
                        category: EventCategory::Changes,
                        description: format!("The {} {mod_duration} mods wore off.", possessive(team_nickname)),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mods": mods,
                        "type": mod_duration as i32
                    }))
                    .build()
            }
            FedEventData::BirdsCircle { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BirdsCircle,
                        category: EventCategory::Special,
                        description: "The Birds circle ... but they don't find what they're looking for.".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::AmbushedByCrows { ref game, batter_id, ref batter_name, friend_of_crows: ref pitcher } => {
                let prefix = if let Some(PitcherInfo { pitcher_name, .. }) = pitcher {
                    format!("{pitcher_name} calls upon their Friends!\n")
                } else {
                    String::new()
                };
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AmbushedByCrows,
                        category: EventCategory::Special,
                        description: format!("{prefix}A murder of Crows ambush {batter_name}!\nThey run to safety, resulting in an out."),
                        player_tags: if let Some(PitcherInfo { pitcher_id, .. }) = pitcher { vec![*pitcher_id, batter_id] } else { vec![batter_id] },
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2SetWin { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Sun2SetWin,
                        category: EventCategory::Outcomes,
                        description: format!("Sun 2 set a Win upon the {team_nickname}."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::BlackHoleSwallowedWin { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BlackHoleSwallowedWin,
                        category: EventCategory::Outcomes,
                        description: format!("The Black Hole swallowed a Win from the {team_nickname}!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2 { game, team_nickname, caught_some_rays } => {
                let suffix = if let Some(rays) = &caught_some_rays {
                    format!("\n{} catches some rays.", rays.player_name)
                } else {
                    String::new()
                };

                let child = if let Some(rays) = &caught_some_rays {
                    Some(EventBuilderChild::new(&rays.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::PlayerStatIncrease,
                            category: EventCategory::Changes,
                            description: format!("{} caught some rays.", rays.player_name),
                            team_tags: vec![rays.team_id],
                            player_tags: vec![rays.player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                        "type": 4, // ?
                        "before": rays.rating_before,
                        "after": rays.rating_after,
                    })))
                } else {
                    None
                };

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Sun2,
                        category: EventCategory::Special,
                        description: format!("The {team_nickname} collect 10! Sun 2 smiles.\nSun 2 set a Win upon the {team_nickname}.{suffix}"),
                        player_tags: if let Some(rays) = &caught_some_rays {
                            vec![rays.player_id]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    })
                    .children(child)
                    .build()
            }
            FedEventData::BlackHole { game, scoring_team_nickname, victim_team_nickname, carcinization, compressed_by_gamma } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("The {scoring_team_nickname} collect 10!"));
                eb.push_description(&format!("The Black Hole swallows the Runs and a {victim_team_nickname} Win."));

                if let Some(carc_full) = carcinization {
                    let carc = carc_full.mv; // convenience
                    let carc_description = format!("The {} steal {} for the remainder of the game.",
                                                   carc_full.new_team_name, carc.player_name);
                    let mod_add_description = format!("{} was temporarily stolen.", carc.player_name);
                    eb.push_description(&carc_description);
                    eb.push_child(carc.sub_event, |mut child| {
                        child.push_description(&carc_description);
                        child.push_player_tag(carc.player_id);
                        child.push_team_tag(carc.previous_team_id);
                        child.push_team_tag(carc.new_team_id);
                        child.push_metadata_i64("location", carc.location);
                        child.push_metadata_uuid("playerId", carc.player_id);
                        child.push_metadata_str("playerName", carc.player_name);
                        child.push_metadata_i64("receiveLocation", carc.location);
                        child.push_metadata_uuid("receiveTeamId", carc.new_team_id);
                        child.push_metadata_str("receiveTeamName", carc.new_team_nickname);
                        child.push_metadata_uuid("sendTeamId", carc.previous_team_id);
                        child.push_metadata_str("sendTeamName", carc.previous_team_nickname);
                        child.build(EventType::PlayerMoved)
                    });
                    eb.push_child(carc_full.mod_added_sub_event, |mut child| {
                        child.push_description(&mod_add_description);
                        child.push_player_tag(carc.player_id);
                        child.push_team_tag(carc.new_team_id);
                        child.push_metadata_str("mod", "TEMP_STOLEN");
                        child.push_metadata_i64("type", ModDuration::Game as i64);
                        child.build(EventType::AddedMod)
                    });
                }

                if let Some(gamma) = compressed_by_gamma {
                    eb.push_description("The Black Hole burps!");
                    eb.push_description(&format!("{} is compressed by gamma!", gamma.player_name));
                    eb.push_player_tag(gamma.player_id);
                    eb.push_child(gamma.sub_event, |mut child| {
                        child.push_description(&format!("{} was compressed by gamma!", gamma.player_name));
                        child.push_player_tag(gamma.player_id);
                        child.push_team_tag(gamma.team_id);
                        child.push_metadata_f64("before", gamma.rating_before);
                        child.push_metadata_f64("after", gamma.rating_after);
                        child.push_metadata_i64("type", 4);
                        child.build(EventType::PlayerStatDecrease)
                    })
                }

                eb.build(EventType::BlackHole)
            }
            FedEventData::TeamDidShame { shaming_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamDidShame,
                        category: EventCategory::Outcomes,
                        description: format!("The {shaming_team_nickname} shamed the {shamed_team_nickname}."),
                        team_tags: vec![shaming_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "totalShames": total_shames,
                        "totalShamings": total_shamings,
                    }))
                    .build()
            }
            FedEventData::TeamWasShamed { shamed_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamWasShamed,
                        category: EventCategory::Outcomes,
                        description: format!("The {shamed_team_nickname} were shamed by the {shaming_team_nickname}."),
                        team_tags: vec![shamed_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "totalShames": total_shames,
                        "totalShamings": total_shamings,
                    }))
                    .build()
            }
            FedEventData::CharmWalk { game, batter_name, batter_id, pitcher_name, batter_item_damage, pitcher_item_damage, scores } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Walk,
                        category: EventCategory::Special,
                        description: format!("{batter_name} charms {pitcher_name}!\n{batter_name} walks to first base."),
                        player_tags: vec![batter_id, batter_id], // two of them
                        ..Default::default()
                    })
                    .scores(&scores, " scores!")
                    .item_damage_before_event(&pitcher_item_damage, &pitcher_name)
                    .item_damage_before_event(&batter_item_damage, &batter_name)
                    .build()
            }
            FedEventData::GainFreeRefill { ref game, player_id, ref player_name, ref roast, ref ingredient1, ref ingredient2, ref sub_event, team_id } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} got a Free Refill."),
                        team_tags: team_id.into_iter().collect(),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "COFFEE_RALLY",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GainFreeRefill,
                        category: EventCategory::Special,
                        description: format!("{player_name} is Poured Over with a {roast} roast blending {ingredient1} and {ingredient2}!\n{player_name} got a Free Refill."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .children(vec![child])
                    .build()
            }
            FedEventData::AllergicReaction { ref game, team_id, player_id, ref player_name, ref sub_event, rating_before, rating_after } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerStatDecrease,
                        category: EventCategory::Changes,
                        description: format!("{player_name} had an allergic reaction."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "type": 4, // ?
                        "before": rating_before,
                        "after": rating_after,
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AllergicReaction,
                        category: EventCategory::Special,
                        description: format!("{player_name} swallowed a stray peanut and had an allergic reaction!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::MildPitchWalk { ref game, pitcher_id, ref pitcher_name, batter_id, ref batter_name, ref scores } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\n{batter_name} draws a walk."),
                        player_tags: vec![pitcher_id, batter_id],
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .build()
            }
            FedEventData::PerkUp { ref game, ref players } => {
                let children = players.iter()
                    .map(|player| {
                        EventBuilderChild::new(&player.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedModFromOtherMod,
                                category: EventCategory::Changes,
                                description: format!("{} Perks up.", player.player_name),
                                team_tags: vec![player.team_id],
                                player_tags: vec![player.player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "OVERPERFORMING",
                                "source": "PERK",
                                "type": 3, // ?
                            }))
                    });

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Perk,
                        category: EventCategory::Special,
                        description: players.iter()
                            .map(|player| format!("{} Perks up.", player.player_name))
                            .join("\n"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Blooddrain { ref game, is_siphon, ref sipper, ref sipped, sipped_category } => {
                let children: Vec<_> = [
                    (sipped, EventType::PlayerStatDecrease, format!("{} had blood drained by {}.", sipped.player_name, sipper.player_name)),
                    (sipper, EventType::PlayerStatIncrease, format!("{} drained blood from {}.", sipper.player_name, sipped.player_name)),
                ].into_iter().map(|(change, event_type, description)| {
                    EventBuilderChild::new(&change.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: event_type,
                            category: EventCategory::Changes,
                            description,
                            team_tags: vec![change.team_id],
                            player_tags: vec![change.player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "type": sipped_category.metadata_type(),
                            "before": change.rating_before,
                            "after": change.rating_after,
                        }))
                })
                    .collect();

                let siphon_text = if is_siphon {
                    format!("\n{}'s Siphon activates!", sipper.player_name)
                } else {
                    String::new()
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: if is_siphon { EventType::BlooddrainSiphon } else { EventType::Blooddrain },
                        category: EventCategory::Special,
                        description: format!("The Blooddrain gurgled!{siphon_text}\n{} siphoned some of {}'s {sipped_category} ability!\n{} increased their {sipped_category} ability!", sipper.player_name, sipped.player_name, sipper.player_name),
                        player_tags: vec![sipper.player_id, sipped.player_id],
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Feedback { ref game, players: (ref player_a, ref player_b), position_type, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerTraded,
                        category: EventCategory::Changes,
                        description: "Reality flickered in the Feedback.".to_string(),
                        team_tags: vec![player_a.team_id, player_b.team_id],
                        player_tags: vec![player_a.player_id, player_b.player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "aLocation": player_a.location as i64,
                        "aPlayerId": player_a.player_id,
                        "aPlayerName": player_a.player_name,
                        "aTeamId": player_a.team_id,
                        "aTeamName": player_a.team_nickname,
                        "bLocation": player_b.location as i64,
                        "bPlayerId": player_b.player_id,
                        "bPlayerName": player_b.player_name,
                        "bTeamId": player_b.team_id,
                        "bTeamName": player_b.team_nickname,
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FeedbackSwap,
                        category: EventCategory::Special,
                        description: format!("Reality flickers. Things look different ...\n{} and {} switch teams in the feedback!\n{} is now {}.",
                                             player_a.player_name, player_b.player_name, player_b.player_name, position_type.role()),
                        player_tags: vec![player_a.player_id, player_b.player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BestowReverberating { ref game, team_id, player_id, ref player_name, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} is now Reverberating wildly!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "REVERBERATING",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ReverbBestowsReverberating,
                        category: EventCategory::Special,
                        description: format!("Reverberations are at dangerous levels!\n{player_name} is now Reverberating wildly!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Reverb { ref game, team_id, ref team_nickname, ref reverb_type, ref gravity_players } => {
                let get_child = |sub_event, event_type, shuffle_location| {
                    EventBuilderChild::new(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: event_type,
                            category: EventCategory::Changes,
                            description: format!("The {team_nickname} {shuffle_location}"),
                            team_tags: vec![team_id],
                            ..Default::default()
                        })
                        .metadata(json!({ "parent": self.id }))
                };

                let gravity_suffix = gravity_players.iter()
                    .map(|player| format!("\n{}'s Gravity kept them in place!", player.player_name))
                    .join("");

                let mut player_tags = gravity_players.iter()
                    .map(|player| player.player_id)
                    .collect();

                match reverb_type {
                    ReverbType::Lineup(sub_event) => {
                        event_builder.for_game(game)
                            .fill(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations are at unsafe levels!\nThe {team_nickname} had their lineup shuffled in the Reverb!{gravity_suffix}"),
                                player_tags,
                                ..Default::default()
                            })
                            .child(get_child(sub_event, EventType::ReverbLineupShuffle, "had their lineup shuffled."))
                            .build()
                    }
                    ReverbType::Rotation(sub_event) => {
                        event_builder.for_game(game)
                            .fill(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations are at unsafe levels!\nThe {team_nickname} had their rotation shuffled in the Reverb!{gravity_suffix}"),
                                player_tags,
                                ..Default::default()
                            })
                            .child(get_child(sub_event, EventType::ReverbRotationShuffle, "had their rotation shuffled in the Reverb!"))
                            .build()
                    }
                    ReverbType::Full(sub_event) => {
                        event_builder.for_game(game)
                            .fill(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations are at dangerous levels!\nThe {team_nickname} were shuffled in the Reverb!{gravity_suffix}"),
                                player_tags,
                                ..Default::default()
                            })
                            .child(get_child(sub_event, EventType::ReverbFullShuffle, "were shuffled in the Reverb!"))
                            .build()
                    }
                    ReverbType::SeveralPlayers(player_reverbs) => {
                        let children = player_reverbs.iter()
                            .map(|reverb| {
                                player_tags.push(reverb.first_player_id);
                                player_tags.push(reverb.second_player_id);
                                EventBuilderChild::new(&reverb.sub_event)
                                    .update(EventBuilderUpdate {
                                        r#type: EventType::PlayerSwap,
                                        category: EventCategory::Changes,
                                        description: format!("The {team_nickname} had several players shuffled in the Reverb!"),
                                        team_tags: vec![team_id],
                                        player_tags: vec![reverb.first_player_id, reverb.second_player_id],
                                        ..Default::default()
                                    })
                                    .metadata(json!({
                                        "aLocation": reverb.first_player_new_location as i64,
                                        "aPlayerId": reverb.first_player_id,
                                        "aPlayerName": reverb.first_player_name,
                                        "bLocation": reverb.second_player_new_location as i64,
                                        "bPlayerId": reverb.second_player_id,
                                        "bPlayerName": reverb.second_player_name,
                                        "teamId": team_id,
                                        "teamName": team_nickname,
                                    }))
                            })
                            // Need to collect to clear the borrow of player_tags
                            .collect_vec();
                        event_builder.for_game(game)
                            .fill(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations are at high levels!\nThe {team_nickname} had several players shuffled in the Reverb!{gravity_suffix}"),
                                player_tags,
                                ..Default::default()
                            })
                            .children(children)
                            .build()
                    }
                }
            }
            FedEventData::TarotReading { description, metadata, player_tags, team_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TarotReading,
                        category: EventCategory::Changes,
                        description,
                        player_tags,
                        team_tags,
                        ..Default::default()
                    })
                    .metadata(metadata)
                    .build()
            }
            FedEventData::TarotReadingAddedOrRemovedMod { team_id, player_id, description, r#mod, mod_duration, mod_removed } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: if mod_removed { EventType::RemovedMod } else { EventType::AddedMod },
                        category: EventCategory::Changes,
                        description,
                        team_tags: vec![team_id],
                        player_tags: player_id.into_iter().collect(),
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": r#mod,
                        "type": mod_duration as i64,
                    }))
                    .build()
            }
            FedEventData::BecomeTripleThreat { ref game, ref pitchers } => {
                let children = pitchers.iter()
                    .map(|pitcher| {
                        EventBuilderChild::new(&pitcher.sub_event)
                            .update(EventBuilderUpdate {
                                category: EventCategory::Changes,
                                r#type: EventType::AddedMod,
                                description: format!("{} is a Triple Threat.", pitcher.player_name),
                                team_tags: vec![pitcher.team_id],
                                player_tags: vec![pitcher.player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "TRIPLE_THREAT",
                                "type": 0, // ?
                            }))
                    });
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BecomeTripleThreat,
                        category: EventCategory::Special,
                        description: if let Some((pitcher_1, pitcher_2)) = pitchers.iter().collect_tuple() {
                            format!("{} and {} chug a Third Wave of Coffee!\nThey are now Triple Threats!", pitcher_1.player_name, pitcher_2.player_name)
                        } else if let Some((pitcher, )) = pitchers.iter().collect_tuple() {
                            format!("{} chugs a Third Wave of Coffee!\nThey are now a Triple Threat!", pitcher.player_name)
                        } else {
                            panic!("There should either be one or two pitchers here")
                        },
                        player_tags: pitchers.iter().map(|pitcher| pitcher.player_id).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::UnderOver { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Under Over, {}.", if on { "On" } else { "Off" });
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "UNDEROVER",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::UnderOver,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::OverUnder { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Over Under, {}.", if on { "On" } else { "Off" });
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "UNDERPERFORMING",
                        "source": "OVERUNDER",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::OverUnder,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::TasteTheInfinite { ref game, sheller_id, ref sheller_name, shellee_team_id, shellee_id, ref shellee_name, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: format!("{shellee_name} is Shelled!"),
                        team_tags: vec![shellee_team_id],
                        // Yes this makes no sense! but, it appears to be that way
                        player_tags: vec![sheller_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SHELLED",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TasteTheInfinite,
                        category: EventCategory::Special,
                        description: format!("{sheller_name} tastes the infinite!\n{shellee_name} is Shelled!"),
                        player_tags: vec![sheller_id, shellee_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BatterSkipped { game, batter_name, reason } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BatterSkipped,
                        description: match reason {
                            BatterSkippedReason::Shelled => { format!("{batter_name} is Shelled and cannot escape!") }
                            BatterSkippedReason::Elsewhere(_) => { format!("{batter_name} is Elsewhere..") }
                        },
                        // Bizarrely, the player tag is on elsewhere players but not shelled ones
                        player_tags: if let BatterSkippedReason::Elsewhere(id) = reason {
                            vec![id]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FeedbackBlocked { ref game, resisted_id, ref resisted_name, tangled_id, tangled_team_id, ref tangled_name, tangled_rating_before, tangled_rating_after, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatDecrease,
                        description: format!("{tangled_name} is tangled in the flicker!"),
                        team_tags: vec![tangled_team_id],
                        player_tags: vec![tangled_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": tangled_rating_before,
                        "after": tangled_rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FeedbackBlocked,
                        category: EventCategory::Special,
                        description: format!("Reality begins to flicker ...\nBut {resisted_name} resists!\n{tangled_name} is tangled in the flicker!"),
                        player_tags: vec![resisted_id, tangled_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::FlagPlanted { team_id, team_nickname, ballpark_name, prefab_name, renovation_id, votes, is_first } => {
                let flag_planted_str = if is_first {
                    "!\nTHE FLAG IS PLANTED"
                } else {
                    ".\nAnother flag is planted!"
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FlagPlanted,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} break ground on {ballpark_name}, selecting to build the {prefab_name} prefab{flag_planted_str}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "renoId": renovation_id,
                        "title": "Ground Broken",
                        "votes": votes,
                    }))
                    .build()
            }
            FedEventData::EmergencyAlert { message, team_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EmergencyAlert,
                        category: EventCategory::Outcomes,
                        description: message,
                        team_tags,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::TeamJoinedILB { team_id, team_nickname, division_id, division_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have joined the ILB!\nThey will play in the {division_name} division."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "divisionId": division_id,
                        "divisionName": division_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::FloodingSwept { ref game, ref effects, ref free_refills, flood_pumps } => {
                // I'm being uncharacteristically imperative with this one
                let mut children = Vec::new();
                let mut player_tags = Vec::new();
                let mut description = "A surge of Immateria rushes up from Under!\nBaserunners are swept from play!".to_string();

                for effect in effects {
                    match effect {
                        FloodingSweptEffect::Elsewhere(ModChangeSubEventWithNamedPlayer { sub_event, team_id, player_id, player_name }) => {
                            children.push(
                                EventBuilderChild::new(&sub_event)
                                    .update(EventBuilderUpdate {
                                        r#type: EventType::AddedMod,
                                        category: EventCategory::Changes,
                                        description: format!("{player_name} is swept Elsewhere!"),
                                        team_tags: vec![*team_id],
                                        player_tags: vec![*player_id],
                                        ..Default::default()
                                    })
                                    .metadata(json!({
                                        "mod": "ELSEWHERE",
                                        "type": 0, // ?
                                    }))
                            );
                            write!(description, "\n{player_name} is swept Elsewhere!").unwrap();
                        }
                        FloodingSweptEffect::Flippers(PlayerInfo { player_name, player_id }) => {
                            player_tags.push(*player_id);
                            write!(description, "\n{player_name} uses their Flippers to slingshot home!").unwrap();
                        }
                        FloodingSweptEffect::Ego(PlayerInfo { player_name, player_id }) => {
                            player_tags.push(*player_id);
                            write!(description, "\n{player_name}'s Ego keeps them on base!").unwrap();
                        }
                    }
                }

                for refill in free_refills {
                    write!(description, "\n{} used their Free Refill.\n{} Refills the In!",
                           refill.player_name, refill.player_name).unwrap();
                    children.push(make_free_refill_child(refill));
                }

                if flood_pumps {
                    write!(description, "\nThe Flood Pumps activate!").unwrap();
                }

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FloodingSwept,
                        category: EventCategory::Special,
                        description,
                        player_tags,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::ReturnFromElsewhere { ref game, ref player_name, ref flavor } => {
                let (description, children) = match flavor {
                    ReturnFromElsewhereFlavor::Full { team_id, player_id, sub_event, time_elsewhere, scattered, recongealed_differently } => {
                        let description = match time_elsewhere {
                            TimeElsewhere::Days(days) => {
                                let s = if *days == 1 { "" } else { "s" };
                                format!("{player_name} has returned from Elsewhere after {days} day{s}!")
                            }
                            TimeElsewhere::Seasons(1) => {
                                format!("{player_name} has returned from Elsewhere after one season!")
                            }
                            TimeElsewhere::Seasons(seasons) => {
                                format!("{player_name} has returned from Elsewhere after {seasons} seasons!")
                            }
                        };
                        let elsewhere_child = EventBuilderChild::new(sub_event)
                            .update(EventBuilderUpdate {
                                category: EventCategory::Changes,
                                r#type: EventType::RemovedMod,
                                description: description.clone(),
                                team_tags: vec![*team_id],
                                player_tags: vec![*player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "ELSEWHERE",
                                "type": 0, // ?
                            }));

                        let mut children = if let Some(Scattered { scattered_name, sub_event }) = scattered {
                            let scattered_child = EventBuilderChild::new(sub_event)
                                .update(EventBuilderUpdate {
                                    category: EventCategory::Changes,
                                    r#type: EventType::AddedMod,
                                    description: format!("{scattered_name} was Scattered..."),
                                    team_tags: vec![*team_id],
                                    player_tags: vec![*player_id],
                                    ..Default::default()
                                })
                                .metadata(json!({
                                    "mod": "SCATTERED",
                                    "type": 0, // ?
                                }));

                            vec![scattered_child, elsewhere_child]
                        } else {
                            vec![elsewhere_child]
                        };

                        if let Some(recongeal) = recongealed_differently {
                            children.push(
                                EventBuilderChild::new(&recongeal.sub_event)
                                    .update(EventBuilderUpdate {
                                        category: EventCategory::Changes,
                                        r#type: if recongeal.rating_after > recongeal.rating_before {
                                            EventType::PlayerStatIncrease
                                        } else {
                                            EventType::PlayerStatDecrease
                                        },
                                        description: format!("{} re-congealed differently.", recongeal.player_name),
                                        team_tags: vec![recongeal.team_id],
                                        player_tags: vec![recongeal.player_id],
                                        ..Default::default()
                                    })
                                    .metadata(json!({
                                    "after": recongeal.rating_after,
                                    "before": recongeal.rating_before,
                                    "type": 4,
                                }))
                            );
                        }

                        (description, children)
                    }
                    ReturnFromElsewhereFlavor::Short { team_id, player_id, sub_event } => {
                        let description = format!("{player_name} has returned from Elsewhere!");
                        let elsewhere_child = EventBuilderChild::new(sub_event)
                            .update(EventBuilderUpdate {
                                category: EventCategory::Changes,
                                r#type: EventType::RemovedMod,
                                description: description.clone(),
                                team_tags: vec![*team_id],
                                player_tags: vec![*player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "ELSEWHERE",
                                "type": 0, // ?
                            }));

                        (description, vec![elsewhere_child])
                    }
                    ReturnFromElsewhereFlavor::False => {
                        let description = format!("{player_name} has returned from Elsewhere!");
                        (description, vec![])
                    }
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ReturnFromElsewhere,
                        description,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Incineration { ref game, team_id, ref team_nickname, victim_id, ref victim_name, replacement_id, ref replacement_name, location, ref unstable_chain, ref sub_events } => {
                let (incin_child, enter_hall_child, hatch_child, replace_child) = sub_events;
                let location_int: i64 = location.into();
                let mut prefix = String::new();
                let mut suffix = String::new();
                let mut children = vec![
                    EventBuilderChild::new(incin_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::Incineration,
                            description: format!("Rogue Umpire incinerated {victim_name}!"),
                            team_tags: vec![team_id],
                            player_tags: vec![victim_id],
                            ..Default::default()
                        }),
                    EventBuilderChild::new(enter_hall_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::EnterHallOfFlame,
                            description: format!("{victim_name} entered the Hall of Flame."),
                            player_tags: vec![victim_id],
                            ..Default::default()
                        }),
                    EventBuilderChild::new(hatch_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::PlayerHatched,
                            description: format!("{replacement_name} has been hatched from the field of eggs."),
                            player_tags: vec![replacement_id],
                            ..Default::default()
                        })
                        .metadata(json!({ "id": replacement_id })),
                    EventBuilderChild::new(replace_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::PlayerBornFromIncineration,
                            description: format!("{replacement_name} replaced the incinerated {victim_name}."),
                            team_tags: vec![team_id],
                            player_tags: vec![victim_id, replacement_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "inPlayerId": replacement_id,
                            "inPlayerName": replacement_name,
                            "location": location_int,
                            "outPlayerId": victim_id,
                            "outPlayerName": victim_name,
                            "teamId": team_id,
                            "teamName": team_nickname,
                        })),
                ];

                if let Some(chain) = unstable_chain {
                    prefix = format!("{victim_name} is Unstable!\nA Debt was collected.\n");
                    suffix = format!("\nThe Instability chains to {}!", chain.player_name);
                    children.push(
                        EventBuilderChild::new(&chain.sub_event)
                            .update(EventBuilderUpdate {
                                category: EventCategory::Changes,
                                r#type: EventType::AddedMod,
                                description: format!("The Instability chains to {}!", chain.player_name),
                                team_tags: vec![chain.team_id],
                                player_tags: vec![chain.player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "MARKED",
                                "type": ModDuration::Weekly as i64,
                            }))
                    )
                }

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Incineration,
                        category: EventCategory::Special,
                        description: format!("{prefix}Rogue Umpire incinerated {victim_name}!\nThey're replaced by {replacement_name}.{suffix}"),
                        player_tags: vec![victim_id, replacement_id],
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::PitcherChange { game, team_nickname: team_name, pitcher_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PitcherChange,
                        description: format!("{pitcher_name} is now pitching for the {team_name}."),
                        player_tags: vec![pitcher_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Party { ref game, team_id, player_id, ref player_name, ref sub_event, rating_before, rating_after } => {
                let description = format!("{player_name} is Partying!");
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatIncrease,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Party,
                        description,
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerHatched { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerHatched,
                        category: EventCategory::Changes,
                        description: format!("{player_name} has been hatched from the field of eggs."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PostseasonBirth { team_id, team_nickname, player_id, player_name, location } => {
                let location_int: i64 = location.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerAddedToTeam,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} earn a Postseason Birth!"),
                        player_tags: vec![player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": location_int,
                        "playerId": player_id,
                        "playerName": player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::FinalStandings { team_id, team_nickname, place, division_name } => {
                let place_str = match place {
                    0 => "1st".to_string(),
                    1 => "2nd".to_string(),
                    2 => "3rd".to_string(),
                    _ => format!("{}th", place + 1),
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FinalStandings,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} finished {place_str} in the {division_name}."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "place": place }))
                    .build()
            }
            FedEventData::TeamLeftPartyTimeForPostseason { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have been removed from Party Time to join the Postseason!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "PARTY_TIME",
                        "type": 1, // ?
                    }))
                    .build()
            }
            FedEventData::EarnedPostseasonSlot { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EarnedPostseasonSlot,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} earned a spot in the Season {} Postseason.", self.season + 1),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PostseasonAdvance { team_id, team_nickname, round, displayed_season: season } => {
                let round_str = if let Some(round) = round {
                    format!("Round {round}")
                } else {
                    String::from("The Internet Series")
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PostseasonAdvance,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} advanced to {round_str} of the Season {season} Postseason."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PostseasonEliminated { team_id, team_nickname, displayed_season: season } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PostseasonEliminated,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} have been eliminated from the Season {season} Postseason."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PlayerBoosted { team_id, player_id, player_name, rating_before, rating_after } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
                        category: EventCategory::Changes,
                        description: format!("{player_name} was boosted."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }))
                    .build()
            }
            FedEventData::TeamEnteredPartyTime { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have entered Party Time!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "PARTY_TIME",
                        "type": 1
                    }))
                    .build()
            }
            FedEventData::TeamWonInternetSeries { team_id, team_nickname, championships } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamWonInternetSeries,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} won the Season {} Internet Series!", self.season + 1),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "championships": championships
                    }))
                    .build()
            }
            FedEventData::BottomDwellers { team_id, team_nickname, rating_before, rating_after } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} are Bottom Dwellers."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 5, // ?
                    }))
                    .build()
            }
            FedEventData::WillReceived { team_id, will_title, metadata } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::WillRecieved,
                        category: EventCategory::Outcomes,
                        description: format!("Will Received: {will_title}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::BlessingWon { team_tags, blessing_title, metadata } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BlessingOrGiftWon,
                        category: EventCategory::Outcomes,
                        description: format!("Blessing Won: {blessing_title}"),
                        team_tags,
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::EarlbirdsAddedToTeam { ref game, team_id, ref team_nickname, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedModFromOtherMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} are Earlbirds!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "EARLBIRDS",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Earlbird,
                        category: EventCategory::Special,
                        description: format!("Happy Earlseason!\nThe {team_nickname} are Earlbirds!"),
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::DecreePassed { decree_title, metadata } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::DecreePassed,
                        category: EventCategory::Outcomes,
                        description: format!("Decree Passed: {decree_title}"),
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::PlayerJoinedILB { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("{player_name} has joined the ILB."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PlayerPermittedToStay { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerPermittedToStay,
                        category: EventCategory::Special,
                        description: format!("{player_name} has been permitted to stay."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FireproofIncineration { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::IncinerationBlocked,
                        category: EventCategory::Special,
                        description: format!("Rogue Umpire tried to incinerate {player_name}, but they're Fireproof! The Umpire was incinerated instead!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::LineupSorted { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::LineupSorted,
                        category: EventCategory::Changes,
                        description: format!("The {} lineup has been optimized.", possessive(team_nickname)),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EarlbirdsRemovedFromTeam { ref game, team_id, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedModFromOtherMod,
                        category: EventCategory::Changes,
                        description: format!("Earlbirds wears off for the [object Object]."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "EARLBIRDS",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Earlbird,
                        category: EventCategory::Special,
                        description: format!("Happy Earlseason!\nEarlbirds wears off for the [object Object]."),
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Undersea { ref game, ref team_name, team_id, ref sub_event } => {
                let description = format!("The {team_name} go Undersea. They're now Overperforming!");
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedModFromOtherMod,
                        category: EventCategory::Changes,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "UNDERSEA",
                        "type": 3, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Undersea,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }

            FedEventData::RenovationBuilt { team_id, description, renovation_id, renovation_title, votes } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RenovationBuilt,
                        category: EventCategory::Changes,
                        description,
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "renoId": renovation_id,
                        "title": renovation_title,
                        "votes": votes,
                    }))
                    .build()
            }
            FedEventData::LateToThePartyAdded { ref game, team_id, ref team_nickname, ref sub_event } => {
                let children = if let Some(sub_event) = sub_event {
                    vec![EventBuilderChild::new(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::AddedModFromOtherMod,
                            category: EventCategory::Changes,
                            description: format!("The {team_nickname} are Late to the Party!"),
                            team_tags: team_id.into_iter().collect(),
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": "OVERPERFORMING",
                            "source": "LATE_TO_PARTY",
                            "type": 0, // ?
                        }))]
                } else {
                    vec![]
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::LateToTheParty,
                        category: EventCategory::Special,
                        description: format!("Late to the Party!\nThe {team_nickname} are Late to the Party!"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }

            FedEventData::PeanutMister { game, player_id, player_name, superallergy } => {
                let effect_str = if superallergy.is_some() {
                    "is no longer Superallergic"
                } else {
                    "has been cured of their peanut allergy"
                };

                let child = superallergy.map(|superallergy| {
                    EventBuilderChild::new(&superallergy.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::RemovedMod,
                            category: EventCategory::Changes,
                            description: format!("{player_name} lost the Superallergic mod."),
                            player_tags: vec![player_id],
                            team_tags: vec![superallergy.team_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": "SUPERALLERGIC",
                            "type": 0, // ?
                        }))
                });

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PeanutMister,
                        category: EventCategory::Special,
                        description: format!("The Peanut Mister activates!\n{player_name} {effect_str}!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .children(child)
                    .build()
            }
            FedEventData::PlayerNamedMvp { team_id, player_id, player_name, level } => {
                let mod_name = format!("EGO{level}");
                if level == 1 {
                    event_builder
                        .fill(EventBuilderUpdate {
                            r#type: EventType::AddedMod,
                            category: EventCategory::Changes,
                            description: format!("{player_name} is named an MVP."),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": mod_name,
                            "type": 0,
                        }))
                        .build()
                } else {
                    let prev_mod_name = format!("EGO{}", level - 1);
                    event_builder
                        .fill(EventBuilderUpdate {
                            r#type: EventType::ModChange,
                            category: EventCategory::Changes,
                            description: format!("{player_name} is named a {level}-Time MVP{}",
                                                 // i dont like this
                                                 if level == 2 { "." } else { "!" }),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "from": prev_mod_name,
                            "to": mod_name,
                            "type": 0,
                        }))
                        .build()
                }
            }
            FedEventData::LateToThePartyRemoved { game, team_nickname } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::LateToTheParty,
                        category: EventCategory::Special,
                        description: format!("Late to the Party!\nLate to the Party wears off for the {team_nickname}."),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::BirdsUnshell { game, team_id, player_id, player_name, pecked_free_event, superallergy_event } => {
                let pecked_free_child = EventBuilderChild::new(&pecked_free_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The Birds pecked {player_name} free!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SHELLED",
                        "type": 0, // ?
                    }));

                let superallergy_child = EventBuilderChild::new(&superallergy_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} emerges from the shell with a Superallergy!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SUPERALLERGIC",
                        "type": 0, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BirdsUnshell,
                        category: EventCategory::Special,
                        description: format!("The Birds circle...\nThe Birds pecked {player_name} free!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(pecked_free_child)
                    .child(superallergy_child)
                    .build()
            }
            FedEventData::ReplaceReturnedPlayerFromShadows { team_id, team_nickname, promoted_player_id, promoted_player_name, promoted_location, removed_player_id, removed_player_name, removed_location } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerReplacesReturned,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} cut a player and promoted another from the shadows."),
                        player_tags: vec![removed_player_id, promoted_player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "promoteLocation": promoted_location as i64,
                        "promotePlayerId": promoted_player_id,
                        "promotePlayerName": promoted_player_name,
                        "removeLocation": removed_location as i64,
                        "removePlayerId": removed_player_id,
                        "removePlayerName": removed_player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::PlayerCalledBackToHall { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterHallOfFlame,
                        category: EventCategory::Changes,
                        description: format!("{player_name} entered the Hall of Flame."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::TeamUsedFreeWill { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} used their Free Will."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "FREE_WILL",
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::PlayerLostMod { team_id, player_id, player_name, r#mod, mod_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} lost the {mod_name} mod."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": r#mod,
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::InvestigationMessage { player_id, message } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::InvestigationMessage,
                        category: EventCategory::Special,
                        description: message,
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::HighPressure { game, team_id, team_nickname, is_on, sub_event } => {
                let description = if is_on {
                    format!("The pressure is on! The {team_nickname} are Overperforming.")
                } else {
                    format!("The pressure is off. The {team_nickname} are no longer Overperforming.")
                };

                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        r#type: if is_on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        category: EventCategory::Changes,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "HIGH_PRESSURE",
                        "type": 3, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::HighPressure,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerPulledThroughRift { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("{player_name} was pulled through the Rift."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }

            FedEventData::PlayerLocalized { team_id, team_nickname, player_id, player_name, location } => {
                let location_int: i64 = location.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerAddedToTeam,
                        category: EventCategory::Changes,
                        description: format!("{player_name} Localized into the {} {}.", possessive(team_nickname.clone()), location.location()),
                        player_tags: vec![player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": location_int,
                        "playerId": player_id,
                        "playerName": player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::Echo { game, echoee_name, primary_echo: main_echo, receiver_echos: sub_echos, } => {
                let make_children_for_echo = |echo: Echo, mod_type: i64, source: &str, echo_description: &str| {
                    let child_removed = echo.mods_removed.map(|mods_removed| {
                        let removes: Vec<_> = mods_removed.mod_ids.into_iter()
                            .map(|mod_id| json!({ "type": mod_type, "mod": mod_id }))
                            .collect();

                        EventBuilderChild::new(&mods_removed.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::RemovedModsFromAnotherMod,
                                category: EventCategory::Changes,
                                description: format!("{}'s {}Echo faded.", echo.receiver_name,
                                                     if mod_type == 0 { "" } else { "Echoed " }),
                                player_tags: vec![echo.receiver_id],
                                team_tags: vec![echo.receiver_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "removes": removes,
                                "source": source,
                            }))
                    });
                    let child_added = {
                        let adds: Vec<_> = echo.mods_added.mod_ids.into_iter()
                            .map(|mod_id| json!({ "type": mod_type, "mod": mod_id }))
                            .collect();

                        EventBuilderChild::new(&echo.mods_added.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedModsFromAnotherMod,
                                category: EventCategory::Changes,
                                description: format!("{}{echo_description}!", echo.receiver_name),
                                player_tags: vec![echo.receiver_id],
                                team_tags: vec![echo.receiver_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "adds": adds,
                                "source": source,
                            }))
                    };

                    (child_removed, child_added)
                };

                let receiver_echo_description = format!("'s Echoed an Echo from {}", main_echo.receiver_name);
                let main_echo_children = make_children_for_echo(main_echo, 0, "ECHO",
                                                                &format!(" Echoed {echoee_name}"));
                let sub_echo_children = sub_echos.into_iter()
                    .map(|sub_echo| make_children_for_echo(sub_echo, 1, "RECEIVER",
                                                           &receiver_echo_description));

                let description = main_echo_children.1.update.description.clone();
                let children = iter::once(main_echo_children)
                    .chain(sub_echo_children)
                    .map(|(removed, added)| [removed, Some(added)])
                    .flatten() // This one should flatten the array
                    .flatten() // This one should flatten the options
                    .collect_vec(); // for debugging

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Echo,
                        category: EventCategory::Special,
                        description,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::SolarPanelsAwait { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::SolarPanelsAwait,
                        category: EventCategory::Special,
                        description: "The Solar Panels are angled toward Sun 2.".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EchoIntoStatic { game, echoer, echoee } => {
                let description = format!("ECHO {} STATIC\nECHO {} STATIC", echoer.player_name, echoee.player_name);

                let make_sub_event = |echo: &EchoIntoStatic, sub_event: &SubEvent, event_type: EventType| {
                    let child = EventBuilderChild::new(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: event_type,
                            category: EventCategory::Changes,
                            description: description.clone(),
                            player_tags: vec![echo.player_id],
                            team_tags: vec![echo.team_id],
                            ..Default::default()
                        });

                    if event_type == EventType::PlayerRemovedFromTeam {
                        child.metadata(json!({
                            "playerId": echo.player_id,
                            "playerName": echo.player_name,
                            "teamId": echo.team_id,
                            "teamName": echo.team_nickname,
                        }))
                    } else {
                        child.metadata(json!({
                            "from": "ECHO",
                            "to": "STATIC",
                            "type": 0,
                        }))
                    }
                };

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoIntoStatic,
                        category: EventCategory::Special,
                        description: description.clone(),
                        ..Default::default()
                    })
                    .child(make_sub_event(&echoer, &echoer.removed_from_team_sub_event,
                                          EventType::PlayerRemovedFromTeam))
                    .child(make_sub_event(&echoee, &echoee.removed_from_team_sub_event,
                                          EventType::PlayerRemovedFromTeam))
                    .child(make_sub_event(&echoer, &echoer.mod_changed_sub_event,
                                          EventType::ModChange))
                    .child(make_sub_event(&echoee, &echoee.mod_changed_sub_event,
                                          EventType::ModChange))
                    .build()
            }
            FedEventData::ConsumerAttack { game, team_id, player_id, player_name_all_caps: player_name, effect, sensed_something_fishy, scattered } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_player_tag(player_id);
                eb.push_description("CONSUMERS ATTACK");
                if scattered {
                    eb.push_description("SCATTERED");
                }

                match effect {
                    ConsumerAttackEffect::Chomp { rating_before, rating_after, sub_event } => {
                        eb.push_description(&player_name);
                        let description = eb.description().to_string();
                        eb.push_child(sub_event, |mut child| {
                            child.push_player_tag(player_id);
                            child.push_team_tag(team_id);
                            child.set_description(description);
                            child.build_player_stat_changed(rating_before, rating_after, StatChangeCategory::All as i64)
                        });
                    }
                    ConsumerAttackEffect::DefendedWithItem(damage) => {
                        // Sticking the extra \n here arbitrarily. There are two in a row.
                        eb.push_description(&format!("{player_name} DEFENDS\n"));
                        eb.push_description(&format!("{} {}",
                                                     damage.item_name.to_ascii_uppercase(),
                                                     if damage.health == 0 { "BREAKS" } else { "DAMAGED" }));
                        let description = eb.description().to_string();
                        eb.push_child(damage.sub_event, |mut child| {
                            child.set_description(description);
                            child.build_item_damaged(damage)
                        });
                    }
                }

                if let Some(fishy) = sensed_something_fishy {
                    eb.push_child(fishy.sub_event, |mut child| {
                        child.push_description(&format!("{} sensed something fishy.", fishy.detective_name));
                        child.build_detective_activity(fishy)
                    });
                }

                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::Psychoacoustics { game, stadium_name, team_id, team_nickname, mod_name, mod_id, sub_event } => {
                let description = format!("{stadium_name} is Resonating.\nPsychoAcoustics Echo {mod_name} {} the {team_nickname}.",
                                          if (self.season, self.day) < (15, 33) { "at" } else { "to" });
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedModFromOtherMod,
                        category: EventCategory::Changes,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": mod_id,
                        "source": "PSYCHOACOUSTICS",
                        "type": 3,
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Psychoacoustics,
                        category: EventCategory::Special,
                        description: if (self.season, self.day) < (15, 33) {
                            String::new() // tgb did a whoopsie
                        } else {
                            description
                        },
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::EchoReceiver { game, echoer_name, echoee_name, echoee_id, echoee_team_id, sub_event } => {
                let description = format!("ECHO {echoer_name} ECHO {echoee_name} ECHO");
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::ModChange,
                        category: EventCategory::Changes,
                        description: description.clone(),
                        player_tags: vec![echoee_id],
                        team_tags: vec![echoee_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "from": "RECEIVER",
                        "to": "ECHO",
                        "type": 0,
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoReciever,
                        category: EventCategory::Special,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::TeamGainedFreeWill { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} gain Free Will."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "FREE_WILL",
                        "type": 0,
                    }))
                    .build()
            }
            FedEventData::Tidings { message, metadata, player_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Tidings,
                        category: EventCategory::Outcomes,
                        description: message,
                        player_tags,
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::HomebodyGameStart { game, homebodies } => {
                let (descriptions, children): (Vec<_>, Vec<_>) = homebodies.into_iter()
                    .map(|toggle| {
                        let description = format!("{} is {}.", toggle.player_name,
                                                  if toggle.is_overperforming { "happy to be home" } else { "homesick" });
                        let change_event = make_switch_performing_child(&toggle, &description, "HOMEBODY");
                        (description, change_event)
                    })
                    .unzip();

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Homebody,
                        description: descriptions.into_iter().join("\n"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::SalmonSwim { game, inning_num, run_losses, item_restored, player_expelled } => {
                eb.set_game(game);
                eb.push_description("The Salmon swim upstream!");
                eb.push_description(&format!("Inning {inning_num} begins again."));
                eb.push_description(&run_losses.to_string());

                if let Some(item_restored) = item_restored {
                    let restored_description = format!(
                        "{} {} was {}",
                        Possessive(&item_restored.player_name), item_restored.item_name,
                        if item_restored.health == 1 { "restored!" } else { "repaired." },
                    );
                    eb.push_description(&restored_description);
                    eb.push_child(item_restored.sub_event, |mut child| {
                        // Yes, the parent says swim and the child says swam
                        child.push_description("The Salmon swam upstream!");
                        child.push_description(&restored_description);
                        child.build_item_repaired(item_restored)
                    });
                }

                if let Some(mod_change) = player_expelled {
                    eb.set_category(EventCategory::Special);
                    eb.push_description(&format!("{} is caught in the bind!", mod_change.player_name));
                    eb.push_player_tag(mod_change.player_id);
                    eb.push_child(mod_change.sub_event, |mut child| {
                        child.push_description(&format!("Salmon Cannons expelled {} Elsewhere.", mod_change.player_name));
                        child.push_player_tag(mod_change.player_id);
                        child.push_team_tag(mod_change.team_id);
                        child.push_metadata_str("mod", "ELSEWHERE");
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(EventType::AddedMod)
                    });
                }

                eb.build(EventType::SalmonSwim)
            }
            FedEventData::HitByPitch { game, pitcher_id, pitcher_name, batter_team_id, batter_id, batter_name, sub_event, scores } => {
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: format!("{batter_name} is now being Observed..."),
                        team_tags: vec![batter_team_id],
                        player_tags: vec![batter_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "COFFEE_PERIL",
                        "type": 2, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::HitByPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} hits {batter_name} with a pitch!\n{batter_name} is now being Observed..."),
                        player_tags: vec![pitcher_id, batter_id],
                        ..Default::default()
                    })
                    .scores(&scores, " scores!")
                    .child(child)
                    .build()
            }
            FedEventData::SolarPanelsActivate { game, num_runs, team_nickname } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::SolarPanelsActivation,
                        category: EventCategory::Special,
                        description: format!("The Solar Panels absorb Sun 2's energy!\n{num_runs} Runs are collected and saved for the {team_nickname}'s next game."),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::RunsOverflowing { game, team_nickname, num_runs } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RunsOverflowing,
                        category: EventCategory::Special,
                        description: format!("Runs are Overflowing!\n{team_nickname} gain {}.",
                                             if num_runs == -1. {
                                                 format!("1 Unrun")
                                             } else if num_runs == 1. {
                                                 format!("1 Run")
                                             } else if num_runs < 0. {
                                                 format!("{} Unruns", -num_runs)
                                             } else {
                                                 format!("{num_runs} Runs")
                                             }),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Middling { game, team_nickname, change_event, is_middling } => {
                let child_description = if is_middling {
                    format!("The {team_nickname} are Middling!")
                } else {
                    format!("Middling wears off for the {team_nickname}.")
                };
                let parent_description = format!("Happy Midseason!\n{child_description}");
                let child = EventBuilderChild::new(&change_event.sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: if is_middling { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        description: child_description,
                        team_tags: vec![change_event.team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "MIDDLING",
                        "type": 0, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Middling,
                        category: EventCategory::Special,
                        description: parent_description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::EnterCrimeScene { game, player_id, player_name, previous_team_id, previous_team_name, previous_location, new_team_id, new_team_name, stadium_name, rating_before, rating_after, enter_crime_scene_sub_event: crime_scene_sub_event, enter_shadows_sub_event } => {
                let crime_child = EventBuilderChild::new(&crime_scene_sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerMoved,
                        description: format!("{player_name} entered the Crime Scene at {stadium_name} to Investigate..."),
                        team_tags: vec![previous_team_id, new_team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": previous_location as i64,
                        "playerId": player_id,
                        "playerName": player_name,
                        "receiveLocation": 3,
                        "receiveTeamId": new_team_id,
                        "receiveTeamName": new_team_name,
                        "sendTeamId": previous_team_id,
                        "sendTeamName": previous_team_name,
                    }));
                let shadows_child = EventBuilderChild::new(&enter_shadows_sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatIncrease,
                        description: format!("{player_name} entered the Shadows."),
                        team_tags: vec![new_team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterCrimeScene,
                        category: EventCategory::Special,
                        description: format!("{player_name} enters the Crime Scene at {stadium_name} to Investigate..."),
                        ..Default::default()
                    })
                    .child(crime_child)
                    .child(shadows_child)
                    .build()
            }
            FedEventData::ReturnFromInvestigation { player_id, player_name, previous_team_id, previous_team_name, new_location, new_team_id, new_team_name, emptyhanded } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerMoved,
                        category: EventCategory::Changes,
                        description: format!("{player_name} returns from the Investigation{}.",
                                             if emptyhanded { " emptyhanded" } else { "" }),
                        player_tags: vec![player_id],
                        team_tags: vec![previous_team_id, new_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": 3,
                        "playerId": player_id,
                        "playerName": player_name,
                        "receiveLocation": new_location as i64,
                        "receiveTeamId": new_team_id,
                        "receiveTeamName": new_team_name,
                        "sendTeamId": previous_team_id,
                        "sendTeamName": previous_team_name,
                    }))
                    .build()
            }
            FedEventData::InvestigationConcluded { stadium_name, team_id } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The Crime Scene Investigation at {stadium_name} has concluded."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "CRIME_SCENE",
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::GrindRail { game, player_id, player_name, first_trick, success } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GrindRail,
                        category: EventCategory::Special,
                        description: format!("{player_name} hops on the Grind Rail toward third base.\nThey do a {first_trick}!\n{success}"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EnterSecretBase { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterSecretBase,
                        category: EventCategory::Special,
                        description: format!("{player_name} enters the Secret Base..."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::ExitSecretBase { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ExitSecretBase,
                        category: EventCategory::Special,
                        description: format!("{player_name} exits the Secret Base to Second Base!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EchoChamber { game, team_id, player_id, player_name, which_mod, sub_event } => {
                let mod_id = match which_mod {
                    EchoChamberModAdded::Repeating => { "REPEATING" }
                    EchoChamberModAdded::Reverberating => { "REVERBERATING" }
                };
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: "The Echo Chamber traps a wave.".to_string(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": mod_id,
                        "type": 3, // ?
                    }));


                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoChamber,
                        category: EventCategory::Special,
                        description: format!("The Echo Chamber traps a wave.\n{player_name} is temporarily {which_mod}!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Roam { player_id, player_name, location, previous_team_id, previous_team_nickname, new_team_id, new_team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerMoved,
                        category: EventCategory::Changes,
                        description: format!("{player_name} wandered to a new team."),
                        player_tags: vec![player_id],
                        team_tags: vec![previous_team_id, new_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": location as i64,
                        "playerId": player_id,
                        "playerName": player_name,
                        "receiveLocation": location as i64,
                        "receiveTeamId": new_team_id,
                        "receiveTeamName": new_team_nickname,
                        "sendTeamId": previous_team_id,
                        "sendTeamName": previous_team_nickname,
                    }))
                    .build()
            }
            FedEventData::GlitterCrate { game, player_name, gained_item } => {
                eb.set_game(game);
                eb.push_description("A shimmering Crate descends.");
                eb.push_gained_item(player_name, gained_item);
                eb.build(EventType::GlitterCrateDrop)
            }
            FedEventData::ModsFromAnotherModRemoved { team_id, player_id, player_name, mods_removed, source_mod_name, source_mod_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{player_name}'s mods caused by {source_mod_name} were removed."));
                eb.push_player_tag(player_id);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("source", source_mod_id);
                eb.push_metadata_json_vec("removes", mods_removed.iter()
                    .map(|r| json!({ "mod": r.mod_id, "type": r.mod_duration as i64 }))
                    .collect());

                eb.build(EventType::RemovedModsFromAnotherMod)
            }
            FedEventData::ConsumerExpelled { game, player_id } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("SALMON CANNONS FIRE");
                eb.push_description("CONSUMER EXPELLED");
                eb.push_player_tag(player_id);
                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::EarlbirdsAddedToPlayer { game, team_id, player_id, player_name, sub_event } => {
                let description = format!("{player_name} is an Earlbird.");
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&description);
                eb.push_player_tag(player_id);
                eb.push_child(sub_event, |mut child| {
                    child.push_description(&description);
                    child.push_player_tag(player_id);
                    child.push_team_tag(team_id);
                    child.push_metadata_str("mod", "OVERPERFORMING");
                    child.push_metadata_str("source", "EARLBIRDS");
                    child.push_metadata_i64("type", ModDuration::Permanent as i64);
                    child.build(EventType::AddedModFromOtherMod)
                });
                eb.build(EventType::Earlbird)
            }
            FedEventData::MindTrickWalk { game, strikeout_type, batter_id, batter_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("{batter_name} strikes out {strikeout_type}."));
                eb.push_description(&format!("{batter_name} uses a Mind Trick!"));
                eb.push_description("The umpire sends them to first base.");
                eb.push_player_tag(batter_id);
                eb.build(EventType::Walk)
            }
            FedEventData::MindTrickStrikeout { game, batter_id, batter_name, pitcher_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("{batter_name} draws a walk."));
                eb.push_description(&format!("{pitcher_name} uses a Mind Trick!"));
                eb.push_description(&format!("{batter_name} strikes out thinking."));
                eb.push_player_tag(batter_id);
                eb.push_player_tag(batter_id); // batter twice, apparently
                eb.build(EventType::Walk) // ugh
            }
            FedEventData::BlooddrainBlocked { game, sipper_id, sipper_name, sippee_id, sippee_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                eb.push_description(&format!("{sipper_name} tried to siphon blood from {sippee_name}, but they were Sealed!"));
                eb.push_player_tag(sipper_id);
                eb.push_player_tag(sippee_id); // batter twice, apparently
                eb.build(EventType::BlooddrainBlocked)
            }
        }
    }

    fn make_mod_change_sub_events<'a>(&self, mod_changes: &[ModChangeSubEventWithNamedPlayer], event_type: EventType, message: &str, mod_name: &str) -> (Vec<EventBuilderChildFull>, String) {
        let suffix = mod_changes.iter()
            .map(|e| format!("\n{} {message}", e.player_name))
            .join("");

        let children = mod_changes.iter()
            .map(|e| {
                EventBuilderChild::new(&e.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: event_type,
                        category: EventCategory::Changes,
                        description: format!("{} {message}", e.player_name),
                        team_tags: vec![e.team_id],
                        player_tags: vec![e.player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": mod_name,
                        "type": 0, // ?
                    }))
            })
            .collect();

        (children, suffix)
    }
}

fn base_name(base_stolen: i32) -> &'static str {
    match base_stolen {
        2 => "second",
        3 => "third",
        4 => "fourth",
        5 => "fifth",
        _ => panic!("What base is this")
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