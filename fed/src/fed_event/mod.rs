mod fed_event_impl;
pub mod run_source;
use crate::format_utils::WholeRuns;
pub use run_source::RunSource;

use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Write};
use std::iter;
use std::marker::PhantomData;
use chrono::{DateTime, Utc};
use enum_access::EnumDisplay;
use itertools::{Either, Itertools};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use eventually_api::{EventMetadata, EventType, EventuallyEvent, Weather};
use num_enum::{IntoPrimitive, TryFromPrimitive, TryFromPrimitiveError};
use derive_builder::Builder;
use schemars::JsonSchema;
use strum_macros::{AsRefStr, Display as StrumDisplay};
use with_structure::WithStructure;
use enum_flatten_derive::{EnumFlatten, EnumFlattenable};

use crate::FeedParseError;
use crate::format_utils::{NewlineDelimiter, RunDisplay, Runs};
use crate::parse::builder::possessive;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, IntoPrimitive, TryFromPrimitive, WithStructure)]
#[repr(i64)]
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

// TODO Check to see if this is a dupe of an existing struct or if any subfields can be consolidated
//   into an existing struct
// TODO After doing the above, document this struct
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct TraderTrade {
    pub victim_id: Uuid,
    pub victim_name: String,
    pub victim_team_id: Uuid,
    pub victim_item_rating_before: f64,
    pub victim_item_rating_after: f64,
    pub victim_rating: f64,

    pub trader_id: Uuid,
    pub trader_name: String,
    pub trader_team_id: Uuid,
    pub trader_item_rating_before: f64,
    pub trader_item_rating_after: f64,
    pub trader_rating: f64,

    pub stolen_item_id: Uuid,
    pub stolen_item_name: String,
    pub stolen_item_mods: Vec<String>,

    pub exchanged_item_name: Option<String>,
    pub victim_lost_item_sub_event: SubEvent,
    pub trader_gained_item_sub_event: SubEvent,
}

/// Game data. Every game event has one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

    /// If a Trader initiated a Trade on this tick, contains information about the trade
    pub trader_trade: Option<TraderTrade>,
}

/// Pitch data. The normal-baseball game events all have one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
    pub nuts: i64,
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
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct WinSubEvent {
    /// Uuid of the team that gains the Win
    pub team_id: Uuid,

    /// Number of Wins the winning team has once the newly earned Win is added
    pub wins_after: i64,

    #[serde(flatten)]
    pub sub_event: SubEvent,

    /// If the stadium inflated some Balloons from this Win, the name of the stadium that inflated
    /// the Balloons. Otherwise null.
    pub balloons: Option<String>,
}

// TODO Consolidate with ModChangeSubEventWithNamedPlayer
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

    /// Info about the Hotel Motel party on this score, if any
    pub hotel_motel_party: Option<HotelMotelParty>,

    /// Info about Hype building as a result of this score, if any
    pub hype: Option<Hype>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct HotelMotelParty {
    /// If Birds were attracted to the stadium, the name of the stadium
    pub birds: Option<String>,

    #[serde(flatten)]
    pub boost: PlayerBoostSubEventWithTeam,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HotelMotelScoringPlayer {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,

    #[serde(flatten)]
    pub party: HotelMotelParty,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct Scores<LedgerRunT: LedgerV2> {
    /// Info for all the scores that happened on this event
    pub scores: Vec<ScoringPlayer>,

    /// List of free refills used on this event, if any. This should always be empty if `scores` is
    /// empty, but if `scores` is non-empty it may be larger than `scores`.
    ///
    /// It's almost possible to attribute each one to the specific score that caused it, but not
    /// quite because FlyOut events don't have pitcher and batter uuids.
    pub free_refills: Vec<FreeRefill>,

    /// Starting in season 20 the sim started outputting score summary events (RunsScored) and
    /// attaching effects (such as Balloons) to the score summary. This contains that information.
    pub score_summary: Option<ScoreSummary<LedgerRunT>>,
}

impl<T: LedgerV2> Scores<T> {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, IntoPrimitive, TryFromPrimitive, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
// Which of those it is will come from context. If the id of the player is not present in the
// containing event, use ModChangeSubEventWithPlayer or ModChangeSubEventWithNamedPlayer instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct FlipNegative {
    /// Uuid of the undertaker player (the one who did the flipping)
    pub undertaker_player_id: Uuid,

    /// Name of the undertaker player (the one who did the flipping)
    pub undertaker_player_name: String,

    /// Metadata for the sub-event associated with sending the undertaker player Elsewhere as well
    pub undertaker_elsewhere_sub_event: SubEvent,

    /// Metadata for the sub-event associated with flipping the player negative
    pub flip_negative_sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct PlayerModChangeSubject {
    /// Uuid of the team whose player's mod changed
    pub team_id: Uuid,

    /// Uuid of the player whose mod changed
    pub player_id: Uuid,

    /// Name of the player whose mod changed
    pub player_name: String,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct TeamModChangeSubject {
    /// Uuid of the team whose mod changed
    pub team_id: Uuid,

    /// Nickname of the team whose mod changed. There is (at least?) one instance where the
    /// team's name was not shown and \[object Object] was in its place. For those events, this
    /// field will be null (to try to encourage clients to handle this edge case). If you want
    /// to replicate the displayed event, replace nulls with "\[object Object]".
    pub team_nickname: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub enum SubseasonalMod {
    // Earlseason
    Earlbirds,
    // Midseason
    Middling,
    Coasting,
    // Lateseason
    LateToTheParty,
    EarlyToTheParty,
    // Postseason
    Ambitious,
    Unambitious,
}

impl SubseasonalMod {
    pub fn performing_mod_id(&self) -> &'static str {
        match self {
            SubseasonalMod::Earlbirds => { "OVERPERFORMING" }
            SubseasonalMod::Middling => { "OVERPERFORMING" }
            SubseasonalMod::Coasting => { "UNDERPERFORMING" }
            SubseasonalMod::LateToTheParty => { "OVERPERFORMING" }
            SubseasonalMod::EarlyToTheParty => { "UNDERPERFORMING" }
            SubseasonalMod::Ambitious => { "OVERPERFORMING" }
            SubseasonalMod::Unambitious => { "UNDERPERFORMING" }
        }
    }

    pub fn mod_id(&self) -> &'static str {
        match self {
            SubseasonalMod::Earlbirds => { "EARLBIRDS" }
            SubseasonalMod::Middling => { "MIDDLING" }
            SubseasonalMod::Coasting => { "COASTING" }
            SubseasonalMod::LateToTheParty => { "LATE_TO_PARTY" }
            SubseasonalMod::EarlyToTheParty => { "EARLY_TO_PARTY" }
            SubseasonalMod::Ambitious => { "AMBITIOUS" }
            SubseasonalMod::Unambitious => { "UNAMBITIOUS" }
        }
    }

    pub fn label_for_teams(&self) -> &'static str {
        match self {
            SubseasonalMod::Earlbirds => { "Earlbirds" }
            SubseasonalMod::Middling => { "Middling" }
            SubseasonalMod::Coasting => { "Coasting" }
            SubseasonalMod::LateToTheParty => { "Late to the Party" }
            SubseasonalMod::EarlyToTheParty => { "Early to the Party" }
            SubseasonalMod::Ambitious => { "Ambitious" }
            SubseasonalMod::Unambitious => { "Unambitious" }
        }
    }

    pub fn label_for_players(&self) -> &'static str {
        match self {
            SubseasonalMod::Earlbirds => { "an Earlbird" }
            SubseasonalMod::Middling => { "Middling" }
            SubseasonalMod::Coasting => { "Coasting" }
            SubseasonalMod::LateToTheParty => { "Late to the Party" }
            SubseasonalMod::EarlyToTheParty => { "Early to the Party" }
            // The 2/3 ellipsis is a little hack. The "period" after the label will complete it.
            SubseasonalMod::Ambitious => { "feeling Ambitious.." }
            SubseasonalMod::Unambitious => { "feeling Unambitious.." }
        }
    }

    pub fn prefix(&self) -> Option<&'static str> {
        match self {
            SubseasonalMod::Earlbirds => { Some("Happy Earlseason!") }
            SubseasonalMod::Middling => { Some("Happy Midseason!") }
            SubseasonalMod::Coasting => { None }
            SubseasonalMod::LateToTheParty => { Some("Late to the Party!") }
            SubseasonalMod::EarlyToTheParty => { Some("Early to the Party!") }
            SubseasonalMod::Ambitious => { None }
            SubseasonalMod::Unambitious => { None }
        }
    }

    pub fn event_type(&self) -> EventType {
        match self {
            SubseasonalMod::Earlbirds => { EventType::Earlbird }
            SubseasonalMod::Middling => { EventType::Middling }
            SubseasonalMod::Coasting => { EventType::Coasting }
            SubseasonalMod::LateToTheParty => { EventType::LateToTheParty }
            SubseasonalMod::EarlyToTheParty => { EventType::EarlyToTheParty }
            SubseasonalMod::Ambitious => { EventType::Ambitious }
            SubseasonalMod::Unambitious => { EventType::Unambitious }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct SubseasonalModChange<SubjectType: WithStructure> {
    /// Team or player whose subseasonal mod (de)activated
    pub subject: SubjectType,

    /// Mod which caused the addition or removal. Whether over/underperforming was added or removed
    /// is not stored, but is inferred from this ID.
    pub source_mod: SubseasonalMod,

    /// True if the over/underperforming mod was added, false if it was removed
    pub active: bool,

    /// Metadata for the sub-event associated with the mod change. In Season 13, Late to the Party
    /// announced itself on every game during lateseason, but it only had a sub-event the first time
    /// (the game only generates a sub-event if the mod actually changed). For those events, this
    /// will be null.
    pub sub_event: Option<SubEvent>,

    /// If this mod change caused a dependent mod to be removed, this is the information about that
    /// mod removal.
    pub dependent_mod_change: Option<ModsFromAnotherModRemoved>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

// Like PlayerStatChange for when the player and team is known from other context. Intended for use in an Option
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct KnownPlayerStatChange {
    /// Player's rating before the stats changed. The rating category is stored externally. Rating
    /// is equivalent to stars but is on an 0-1 scale rather than an 0-5 scale.
    pub rating_before: f64,

    /// Player's rating after the stats changed
    pub rating_after: f64,

    /// Metadata for the sub-event associated with the player stat change event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive, WithStructure)]
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

    pub fn title(&self) -> &'static str {
        match self {
            ActivePositionType::Lineup => "Batter",
            ActivePositionType::Rotation => "Pitcher",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, TryFromPrimitive, IntoPrimitive, WithStructure)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
// This uses a combo of flatten and adjacent tagging
#[serde(rename_all = "camelCase", tag = "type", content = "subEvent")]
pub enum ReverbType {
    Rotation(SubEvent),
    Lineup(SubEvent),
    Full(SubEvent),
    SeveralPlayers(Vec<PlayerReverb>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct PlayerNameId {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct KnownPlayerRemovedFromTeam {
    /// Uuid of team the player was removed from
    pub team_id: Uuid,

    /// Nickname of team the player was removed from
    pub team_nickname: String,
    
    /// Metadata for the player removed from team sub-event
    pub sub_event: SubEvent,
}

// This is identical to PlayerInfo except for field names. It's used for JSON schema reasons
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSentElsewhere {
    /// Uuid of the team whose player was sent Elsewhere
    pub team_id: Uuid,

    /// Uuid of the player who was sent Elsewhere
    pub player_id: Uuid,

    /// Name of the player who was sent Elsewhere
    pub player_name: String,

    /// Metadata for the sub-event associated with adding the Elsewhere mod
    pub sub_event: SubEvent,

    /// If the player was flipped negative, this is information about that
    pub flipped_negative: Option<FlipNegative>
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FloodingSweptEffect {
    Elsewhere(PlayerSentElsewhere),
    Flippers {
        /// Uuid of player who scored with Flippers
        player_id: Uuid,

        /// Name of player who scored with Flippers
        player_name: String,

        /// Info about the Hotel Motel party on this score, if any
        hotel_motel_party: Option<HotelMotelParty>,

        /// If this event built hype, the metadata about the hype event
        hype: Option<Hype>,
    },
    Ego(PlayerNameId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(untagged, rename_all = "camelCase")]
pub enum RenovationVotes {
    Normal(i64),
    Manual(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct MultipleModsAddedOrRemoved {
    /// Vector of mods that were added/removed. Each mod is represented by its internal ID.
    pub mods: Vec<ModDesc>,

    /// Metadata for the event associated with adding or removing these mods
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
    Days(i64),
    Seasons(i64),
}

impl Display for TimeElsewhere {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeElsewhere::Days(1) => {
                write!(f, "1 day")
            }
            TimeElsewhere::Days(days) => {
                write!(f, "{days} days")
            }
            TimeElsewhere::Seasons(1) => {
                write!(f, "one season")
            }
            TimeElsewhere::Seasons(seasons) => {
                write!(f, "{seasons} seasons")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnFromElsewhere {
    /// Name of player who returned from Elsewhere
    pub player_name: String,

    /// Which flavor of return from elsewhere this is
    #[serde(flatten)]
    pub flavor: ReturnFromElsewhereFlavor,
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
    /// Player was pulled back from elsewhere by a Seeker
    PulledBack {
        /// Team uuid of player who returned from Elsewhere (also the team of the Seeker)
        team_id: Uuid,

        /// Uuid of player who returned from Elsewhere
        sought_player_id: Uuid,

        /// Uuid of Seeker player who pulled the other player back
        seeker_player_id: Uuid,

        /// Name of Seeker player who pulled the other player back
        seeker_player_name: String,

        /// Scattered sub-event, if the player was scattered, or null otherwise
        scattered: Option<Scattered>,

        /// Metadata for sub-event associated with removing the Elsewhere mod
        sub_event: SubEvent,

        /// Number of days or seasons the player was Elsewhere, if present. Not all elsewhere
        /// returns say the amount of time the player was Elsewhere.
        time_elsewhere: Option<TimeElsewhere>,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct TeamRunsLost {
    /// Number of runs lost
    pub runs_lost: f32,

    /// Name of team who lost the runs
    pub team_name: String,
}

impl Display for TeamRunsLost {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} of the {}'s {} are lost!", self.runs_lost, self.team_name, if self.runs_lost < 0. {
            "Unruns"
        } else {
            "Runs"
        })
    }
}

// TODO: Make this into a static vec with max size 2 (third-party crate)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(rename_all = "camelCase")]
pub struct DetectiveActivity {
    /// Uuid of the detective
    pub detective_id: Uuid,

    /// Name of the detective
    pub detective_name: String,

    /// Metadata for the sub-event associated with the detective activity
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure)]
#[serde(rename_all = "camelCase")]
pub enum DebtType {
    Unstable,
    Observed,
}

impl DebtType {
    pub fn mod_id(&self) -> &'static str {
        // I think it's just a coincidence that neither of these mods' ids match their display names
        match self {
            DebtType::Unstable => { "MARKED" }
            DebtType::Observed => { "COFFEE_PERIL" }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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

    /// Which type of Debt this was, the kind that makes victims Unstable or the kind that makes
    /// them Observed
    pub debt_type: DebtType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
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
    pub points: i64,
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
    /// before being damaged. This is null sometimes and I don't know why.
    // TODO Clarify damage vs. breaking)
    pub player_item_rating_before: Option<f64>,

    /// The increase or decrease that all the wielding player's remaining items cause to their star
    /// rating. This is null sometimes and I don't know why.
    pub player_item_rating_after: Option<f64>,

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
pub struct ItemLost {
    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before losing this item
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's items now cause to their star rating
    pub player_item_rating_after: f64,

    /// The player's star rating. TODO: Is this with or without items?
    pub player_rating: f64,

    /// Metadata for the event associated with losing the item
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq,Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ItemRepaired {
    /// Uuid of item that was repaired
    pub item_id: Uuid,

    /// Name of item that was repaired
    pub item_name: String,

    /// Mods bestowed by item that was repaired
    pub item_mods: Vec<String>,

    /// Durability of item. This is its max health.
    pub durability: i64,

    /// Health of item before being repaired. This cannot be calculated, apparently, since salmon
    /// cannons sometimes restores by one and sometimes restores to full. This may be a change that
    /// took effect in s17, or maybe s17 just happened to be the first time it restored to full.
    pub health_before: i64,

    /// Health of item after being repaired
    pub health_after: i64,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before being repaired (TODO Clarify damage vs. breaking)
    /// As with many of these ratings, it can be `null` for reasons I don't yet understand.
    pub player_item_rating_before: Option<f64>,

    /// The increase or decrease that all the wielding player's items now cause to their star
    /// rating.
    /// As with many of these ratings, it can be `null` for reasons I don't yet understand.
    pub player_item_rating_after: Option<f64>,

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

    /// Metadata for the event associated with the item being repaired
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
pub struct PlayerGrippedByForce {
    /// Uuid of player who was gripped by Force
    pub player_id: Uuid,

    /// Name of player who was gripped by Force
    pub player_name: String,

    /// Sub-event associated with the player not moving
    pub sub_event: SubEvent,
}

// I would love to be able to tag this with `"success": true/false`, but the PR to allow that was
// rejected for developer bandwidth reasons: https://github.com/serde-rs/serde/pull/2056
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
#[serde(tag = "success")]
pub enum PlayerMaybeCarcinized {
    Successful {
        #[serde(flatten)]
        move_event: PlayerMovedTeams,

        /// Metadata for sub-event associated with adding the TEMP_STOLEN mod
        mod_added_sub_event: SubEvent,
    },
    FailedByForce(PlayerGrippedByForce),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Carcinization {
    #[serde(flatten)]

    /// This usually contains the information about the player moving. However, for unknown reasons
    /// (possibly the unpassed decree Force Fields triggering when it shouldn't) the steal failed
    /// once. The child event's description still implied that the player was moved, but it was a
    /// different event type and the Stolen mod was not added.
    pub player_moved: PlayerMaybeCarcinized,

    /// Full name of player's new team
    pub new_team_name: String,
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
    #[serde(alias = "mod")]
    pub mod_id: String,

    /// Duration of the mod
    #[serde(alias = "type")]
    pub mod_duration: ModDuration,
}

impl Into<serde_json::Value> for ModDesc {
    fn into(self) -> serde_json::Value {
        serde_json::json!({
            "mod": self.mod_id,
            "type": self.mod_duration as i64,
        })
    }
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
    FourRun, // Only applies with The Fifth Base, otherwise a 4-run HR is a Grand Slam
    GrandSlam,
}

impl Display for HomeRunType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HomeRunType::Solo => { write!(f, "solo home run") }
            HomeRunType::TwoRun => { write!(f, "2-run home run") }
            HomeRunType::ThreeRun => { write!(f, "3-run home run") }
            HomeRunType::FourRun => { write!(f, "4-run home run") }
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

    /// Metadata for the sub-event associated with activating Maintenance Mode, if applicable
    pub maintenance_mode: Option<MaintenanceMode>,

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PlayerBoostSubEvent {
    /// Player's rating before the boost
    pub rating_before: f64,

    /// Player's rating after the boost
    pub rating_after: f64,

    /// Metadata for the boost sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PlayerBoostSubEventWithTeam {
    /// Team uuid of the player who was boosted
    pub team_id: Uuid,

    /// Player's rating before the boost
    pub rating_before: f64,

    /// Player's rating after the boost
    pub rating_after: f64,

    /// Metadata for the boost sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct NamedPlayerBoostSubEvent {
    /// Player's rating before the boost
    pub rating_before: f64,

    /// Player's rating after the boost
    pub rating_after: f64,

    /// Metadata for the boost sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
pub enum TeamNicknameOrPlayerName {
    TeamNickname(String),
    PlayerName(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct MaintenanceMode {
    pub sub_event: SubEvent,
    pub team_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
pub enum PostseasonBirthBoostEventOrder {
    // TODO Do all 3 of these actually appear in real data?
    AfterHatch,
    AfterBirth,
    AfterEarnedSlot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModsFromAnotherModRemoved {
    /// List of mods that were removed
    pub mods_removed: Vec<ModDesc>,

    /// Name of the mod that had originally added the removed mods. It's implied that this mod
    /// was just removed, which caused these others to be removed as well.
    pub source_mod_name: String,

    /// Metadata for the mod-added/removed-from-other-mod event
    pub event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModsFromAnotherModRemovedWithName {
    /// Name of the player or team who lost the mod(s)
    pub name: TeamNicknameOrPlayerName,

    /// List of mods that were removed
    pub mods_removed: Vec<ModDesc>,

    /// Name of the mod that had originally added the removed mods. It's implied that this mod
    /// was just removed, which caused these others to be removed as well.
    pub source_mod_name: String,

    /// Metadata for the mod-added/removed-from-other-mod event
    pub event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModRemoval {
    /// Internal ID of the mod that was removed
    pub mod_id: String,

    /// If this mod change caused a dependent mod to be removed, this is the information about that
    /// mod removal.
    pub dependent_mod_removal: Option<ModsFromAnotherModRemoved>,

}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct GoodRiddanceParty {
    /// Uuid of player who partied
    pub player_id: Uuid,

    /// Name of player who partied
    pub player_name: String,

    /// Metadata for sub-event associated with player stat change
    pub sub_event: SubEvent,

    /// Player's rating before the party
    ///
    /// TODO I think SIBR figured out how this rating works. Look that up
    pub rating_before: f64,

    /// Player's rating after the party
    pub rating_after: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Hype {
    /// Name of stadium which built hype
    pub stadium_name: String,

    /// Stadium hype before
    pub hype_before: f64,

    /// Stadium hype after
    pub hype_after: f64,

    /// Metadata for sub-event associated with hype change
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum HomeRunHypeSource {
    HomeRun,
    Buckets,
    Hoops,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct HomeRunHype {
    #[serde(flatten)]
    pub hype: Hype,

    /// Which part of this home run caused the hype
    pub source: HomeRunHypeSource,
}

impl HomeRunHype {
    pub fn from_hype_and_source(hype: Hype, source: HomeRunHypeSource) -> Self {
        Self { hype, source }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum NumbersGo {
    Up,
    Down,
}

impl Display for NumbersGo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            NumbersGo::Up => { write!(f, "up") }
            NumbersGo::Down => { write!(f, "down") }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct Ambush {
    /// Uuid of ambushing team. Note that this is not necessarily the team whose player was
    /// incinerated, it can also be the other team in the same game
    pub team_id: Uuid,

    /// Nickname of ambushing team. Note that this is not necessarily the team whose player was
    /// incinerated, it can also be the other team in the same game
    pub team_nickname: String,

    /// Uuid of ambushed player
    pub player_id: Uuid,

    /// Name of ambushed player
    pub player_name: String,
    
    /// If this player was formerly on a team (which can only happen if their whole team was 
    /// Incinerated), this is the info about that team and the removed-from-team event. Otherwise
    /// null.
    pub former_team: Option<KnownPlayerRemovedFromTeam>,

    /// Metadata for the exit-hall-of-flame event
    pub exit_hall_event: SubEvent,

    /// Metadata for the player-added-to-team event
    pub added_to_team_event: SubEvent,

    /// Metadata for the player's shadow boost event. Ambush was added after shadow boosts, so this
    /// sub-event always exists.
    pub shadow_boost_event: SubEvent,

    /// Ambushed player's rating before the shadow boost
    pub player_rating_before: f64,

    /// Ambushed player's rating after the shadow boost
    pub player_rating_after: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum RoamFromLocation {
    Team {
        /// Uuid of player's previous team
        previous_team_id: Uuid,

        /// Nickname of player's previous team
        previous_team_nickname: String,

        /// Parties as a result of the Good Riddance mod
        good_riddance_parties: Vec<GoodRiddanceParty>
    },
    HallOfFlame {
        /// Metadata for the player-left-hall-of-flame sub-event
        sub_event: SubEvent,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum GameStartAnnouncement {
    LetsGo,
    TeamNames {
        /// Away team name
        away: String,

        /// Home team name
        home: String,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum LedgerLineV1 {
    NegativePolarity,
    Underachiever,
    Underhanded,
    Subtractor,
    Tired(String),
    Wired(String),
    AcidicPitch,
    Magnified,
}

impl Display for LedgerLineV1 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LedgerLineV1::NegativePolarity => { write!(f, "Negative Polarity (x-1)") }
            LedgerLineV1::Underachiever => { write!(f, "Underachiever (x-1)") }
            LedgerLineV1::Underhanded => { write!(f, "Underhanded (x-1)") }
            LedgerLineV1::Subtractor => { write!(f, "Subtractor (x-1)") }
            LedgerLineV1::Tired(name) => { write!(f, "{name} is Tired. (0.5 Unruns)") }
            LedgerLineV1::Wired(name) => { write!(f, "{name} is Wired! (0.5 Runs)") }
            LedgerLineV1::AcidicPitch => { write!(f, "Acidic Pitch (0.1 Unruns)") }
            LedgerLineV1::Magnified => { write!(f, "Batter Magnified 2x (x2)") }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum LedgerRunModifier {
    Magnified {
        position: ActivePositionType,
    },
    Underhanded,
    SunPoint1 {
        // The value of a Sun .1 run can theoretically only be a natural number multiple of .1, so
        // it could be stored as a fixed point value, but I decided not to do that because blaseball
        // is blaseball and javascript is javascript
        value: f64,
    },
    Subtractor,
    AcidicPitch,
    Wired {
        player_name: String,
    },
    Tired {
        player_name: String,
    },
    NegativePolarity,
}

impl LedgerRunModifier {
    pub fn modify(&self, in_value: f64) -> f64 {
        match self {
            LedgerRunModifier::Magnified { .. } => { in_value * 2.0 }
            LedgerRunModifier::Underhanded => { in_value * -1.0 }
            LedgerRunModifier::SunPoint1 { value } => { in_value + value }
            LedgerRunModifier::Subtractor => { in_value * -1.0 }
            LedgerRunModifier::AcidicPitch => { in_value - 0.1 }
            LedgerRunModifier::Wired { .. } => { in_value + 0.5 }
            LedgerRunModifier::Tired { .. } => { in_value - 0.5 }
            LedgerRunModifier::NegativePolarity => { in_value * -1.0 }
        }
    }

    pub fn modify_and_write(&self, run_value_before: f64, mut w: &mut impl Write) -> Result<f64, std::fmt::Error> {
        let run_value_after = self.modify(run_value_before);
        match self {
            LedgerRunModifier::Magnified { position } => {
                write!(w, "\t{} Magnified 2x: {} * 2 = {}", position.title(), RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::Underhanded => {
                write!(w, "\tUnderhanded: {} * -1 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::SunPoint1 { value } => {
                write!(w, "\tSun .1: {} + {value} = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::Subtractor => {
                write!(w, "\tSubtractor: {} * -1 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::AcidicPitch => {
                write!(w, "\tAcidic Pitch: {} + -0.1 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::Wired { player_name } => {
                write!(w, "\t{player_name} is Wired!: {} + 0.5 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::Tired { player_name } => {
                write!(w, "\t{player_name} is Tired.: {} + -0.5 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
            LedgerRunModifier::NegativePolarity => {
                write!(w, "\tNegative Polarity: {} * -1 = {}", RunDisplay(run_value_before), RunDisplay(run_value_after))?;
            }
        }

        Ok(run_value_after)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct LedgerRun {
    pub modifiers: Vec<LedgerRunModifier>,
}

impl LedgerRun {
    pub fn new(modifiers: Vec<LedgerRunModifier>) -> Self {
        Self { modifiers }
    }

    pub fn compute_and_write(&self, ledger_label: &str, w: &mut impl Write) -> Result<f64, std::fmt::Error> {
        self.compute_and_write_with_value(1.0, ledger_label, w)
    }

    pub fn compute_and_write_with_value(&self, mut run_value: f64, ledger_label: &str, mut w: &mut impl Write) -> Result<f64, std::fmt::Error> {
        // The !self.modifiers.is_empty() part seems to be a bug in Blaseball
        write!(w, "{ledger_label}: {} Run{}", RunDisplay(run_value), if run_value == 1.0 || !self.modifiers.is_empty() { "" } else { "s" } )?;

        for modifier in &self.modifiers {
            write!(w, "\n")?;
            run_value = modifier.modify_and_write(run_value, w)?;
        }

        Ok(run_value)
    }

    // TODO dedup logic with compute_and_write
    pub fn value(&self, run_value: f64) -> f64 {
        self.modifiers.iter()
            .fold(run_value, |value, modifier| modifier.modify(value))
    }
}

fn write_sum_sun(num_runs: i64, mut w: &mut impl Write) -> Result<(), std::fmt::Error> {
    write!(w, "Sum Sun: {}", WholeRuns(num_runs))
}

pub trait LedgerV2: WithStructure {
    fn label() -> &'static str;

    // Returns the number of Run lines in the ledger
    fn len(&self) -> usize;

    fn run_values(&self) -> impl Iterator<Item=f64>;

    fn write(&self, season: i64, day: i64, w: &mut impl Write) -> std::fmt::Result;
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct SimpleLedgerV2<RunSourceT: WithStructure> {
    pub runs: Vec<LedgerRun>,
    pub sum_sun: Option<i64>,
    source: PhantomData<RunSourceT>,
}

impl<RunSourceT: WithStructure> SimpleLedgerV2<RunSourceT> {
    pub fn from_runs(runs: Vec<LedgerRun>, sum_sun: Option<i64>) -> Self {
        Self {
            runs,
            sum_sun,
            source: Default::default(),
        }
    }
}

impl<RunSourceT: RunSource + WithStructure> LedgerV2 for SimpleLedgerV2<RunSourceT> {
    // TODO I can't remember why I have this indirection and it might not be necessary
    fn label() -> &'static str {
        RunSourceT::label()
    }

    fn len(&self) -> usize {
        self.runs.len() + if self.sum_sun.is_some() { 1 } else { 0 }
    }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        self.runs.iter()
            // SimpleLedger runs are always worth 1.0 before modifiers
            .map(|run| run.value(1.0))
            .chain(self.sum_sun.map(|sum_sun_runs| sum_sun_runs as f64))
    }

    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        let mut delimiter = NewlineDelimiter::new();

        for run in &self.runs {
            delimiter.print(w)?;
            run.compute_and_write(Self::label(), w)?;
        }

        if let Some(sum_sun_runs) = self.sum_sun {
            delimiter.print(w)?;
            write_sum_sun(sum_sun_runs, w)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct HomeRunLedger {
    pub home_run: SimpleLedgerV2<run_source::HomeRun>,
    pub big_bucket: Option<SimpleLedgerV2<run_source::HomeRunBigBucket>>,
    pub alley_oop: Option<SimpleLedgerV2<run_source::HomeRunSlamDunk>>,
    pub sum_sun: Option<i64>,
    pub equal_sun: Option<i64>,
}

impl LedgerV2 for HomeRunLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize {
        let mut len = self.home_run.len();
        if let Some(bucket) = &self.big_bucket { len += bucket.len() }
        if let Some(oop) = &self.alley_oop { len += oop.len() }
        if self.sum_sun.is_some() { len += 1 }
        if self.equal_sun.is_some() { len += 1 }
        len
    }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        // The Either crate very conveniently does the work to consolidate 2 iterators of
        // different concrete types but with the same Item type into a single Iterator type
        self.home_run.run_values()
            .chain(
                if let Some(oop) = &self.big_bucket {
                    Either::Left(oop.run_values())
                } else {
                    Either::Right(iter::empty())
                }
            )
            .chain(
                if let Some(oop) = &self.alley_oop {
                    Either::Left(oop.run_values())
                } else {
                    Either::Right(iter::empty())
                }
            )
            .chain(
                if let Some(sum_sun_runs) = self.sum_sun {
                    Either::Left(iter::once(sum_sun_runs as f64))
                } else {
                    Either::Right(iter::empty())
                }
            )
            .chain(
                if let Some(equal_sun_runs) = self.equal_sun {
                    Either::Left(iter::once(equal_sun_runs as f64))
                } else {
                    Either::Right(iter::empty())
                }
            )
    }

    fn write(&self, season: i64, day: i64, w: &mut impl Write) -> std::fmt::Result {
        self.home_run.write(season, day, w)?;
        if let Some(big_bucket) = &self.big_bucket {
            write!(w, "\n")?;
            big_bucket.write(season, day, w)?;
        }
        if let Some(alley_oop) = &self.alley_oop {
            write!(w, "\n")?;
            alley_oop.write(season, day, w)?;
        }
        if let Some(sum_sun_runs) = self.sum_sun {
            write!(w, "\n")?;
            write_sum_sun(sum_sun_runs, w)?;
        }
        if let Some(equal_sun_runs) = self.equal_sun {
            write!(w, "\n")?;
            write!(w, "Equal Sun: {}", WholeRuns(equal_sun_runs))?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct ModerationLedger {
    // Will always be negative, indicating Unruns, except for that one time it was bugged and gave
    // the talkers runs instead
    pub num_runs: f64,
}

impl ModerationLedger {
    pub fn new(num_runs: f64) -> Self {
        Self { num_runs }
    }
}

impl LedgerV2 for ModerationLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize { 1 }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        iter::once(self.num_runs)
    }

    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        write!(w, "Moderation: {} Unruns", RunDisplay(self.num_runs))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
#[repr(u8)]
pub enum TripleThreats {
    One = 1,
    Two = 2,
    Three = 3,
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct TripleThreatLedger {
    /// Triple threat has 3 conditions under which it can give 0.3 unruns, and they can stack. This
    /// indicates how many of them are active  
    pub threats: TripleThreats,
    pub modifiers: Vec<LedgerRunModifier>,
}

impl TripleThreatLedger {
    pub fn new(threats: TripleThreats, modifiers: Vec<LedgerRunModifier>) -> Self {
        Self { threats, modifiers }
    }

    pub fn value(&self) -> f64 {
        (self.threats as u8) as f64 * -0.3
    }
}

impl LedgerV2 for TripleThreatLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize { 1 }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        iter::once(self.value())
    }

    // TODO: This used to use season and day but it turns out that was the wrong signal. If this was
    //   the only use, remove them from the signature
    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        // Yes, whether this is pluralized depends entirely on whether there are any modifiers. I
        // was surprised too.
        write!(w, "Triple Threat: {}", Runs(self.value())
            .singular_if(!self.modifiers.is_empty()))?;

        for modifier in &self.modifiers {
            write!(w, "\n")?;
            modifier.modify_and_write(self.value(), w)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct HeatMagnetLedger;

impl HeatMagnetLedger {
    pub fn new() -> Self {
        Self
    }
}

impl LedgerV2 for HeatMagnetLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize { 1 }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        iter::once(5.0)
    }

    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        write!(w, "Heat Magnet: 5 Runs")
    }
}

// Should this be generic too? I think the pattern of "one group of an
// arbitrary number of runs, with modifiers" is repeated
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct OverflowLedger {
    // Apparently there's no instance of floating point runs here? Might be
    // wrong but I'm feeling hubrisy
    pub num_runs: i64,
    pub modifiers: Vec<LedgerRunModifier>,
}

impl OverflowLedger {
    pub fn new(num_runs: i64, modifiers: Vec<LedgerRunModifier>) -> Self {
        Self { num_runs, modifiers }
    }
}

impl LedgerV2 for OverflowLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize { 1 }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        iter::once(self.num_runs as f64)
    }

    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        // Like with triple threat, this only gets pluralized if there are no modifiers.
        write!(w, "Overflow: {}",
               Runs(self.num_runs as f64)
                .unruns_always_plural()
                .singular_if(!self.modifiers.is_empty()))?;

        let mut run_value = self.num_runs as f64;
        for modifier in &self.modifiers {
            write!(w, "\n")?;
            run_value = modifier.modify_and_write(run_value, w)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, WithStructure)]
pub struct StolenBaseLedger {
    pub steal_home: Option<LedgerRun>,
    pub blaserunning: Option<LedgerRun>,
    pub sum_sun: Option<i64>,
}

impl LedgerV2 for StolenBaseLedger {
    fn label() -> &'static str {
        todo!()
    }

    fn len(&self) -> usize {
        let mut len = 0;
        if self.steal_home.is_some() { len += 1 }
        if self.blaserunning.is_some() { len += 1 }
        if self.sum_sun.is_some() { len += 1 }
        len
    }

    fn run_values(&self) -> impl Iterator<Item=f64> {
        // I wrote this stuff with the chain when steal_home and blaserunning
        // were different types. It can probably be simpler.
        iter::empty()
            .chain(
                if let Some(sh) = &self.steal_home {
                    Either::Left(iter::once(sh.value(1.0)))
                } else {
                    Either::Right(iter::empty())
                }
            )
            .chain(
                if let Some(br) = &self.blaserunning {
                    Either::Left(iter::once(br.value(0.2)))
                } else {
                    Either::Right(iter::empty())
                }
            )
            .chain(
                if let Some(sum_sun_runs) = self.sum_sun {
                    Either::Left(iter::once(sum_sun_runs as f64))
                } else {
                    Either::Right(iter::empty())
                }
            )
    }

    fn write(&self, _: i64, _: i64, w: &mut impl Write) -> std::fmt::Result {
        let mut delimiter = NewlineDelimiter::new();

        if let Some(sh) = &self.steal_home {
            delimiter.print(w)?;
            sh.compute_and_write("Steal Home", w)?;
        }
        
        if let Some(br) = &self.blaserunning {
            delimiter.print(w)?;
            br.compute_and_write_with_value(0.2, "Blaserunning", w)?;
        }

        if let Some(sum_sun_runs) = self.sum_sun {
            delimiter.print(w)?;
            write_sum_sun(sum_sun_runs, w)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum Ledger<LedgerRunT> where LedgerRunT: LedgerV2 + with_structure::WithStructure {
    None,
    // TODO: If possible, have the V1 parser convert to V2 and always store V2
    V1 {
        base_runs: f64,
        lines: Vec<LedgerLineV1>,
    },
    V2(LedgerRunT),
}

impl<LedgerRunT: LedgerV2> Ledger<LedgerRunT> {
    pub fn to_string(&self, season: i64, day: i64) -> String {
        let mut s = String::new();
        self.write(season, day, &mut s).expect("write() should not fail on a string formatter");
        s
    }
    
    pub fn write(&self, season: i64, day: i64, w: &mut impl Write) -> std::fmt::Result {
        match self {
            Ledger::None => {},
            Ledger::V1 { base_runs, lines } => {
                let (abs_runs, run_type) = if *base_runs < 0. {
                    (-base_runs, "Unrun")
                } else {
                    (*base_runs, "Run")
                };

                if abs_runs == 1. {
                    write!(w, "(1 {run_type}),")?;
                } else {
                    write!(w, "({} {run_type}s),", abs_runs)?;
                }

                for line in lines {
                    write!(w, " {line}")?;
                }
            }
            Ledger::V2(ledger) => {
                ledger.write(season, day, w)?;

                // A summary line is printed iff there was more than 1 instance of runs being scored
                if ledger.len() > 1 {
                    let mut runs_total_value = 0.0;
                    let mut is_first = true;
                    for value in ledger.run_values() {
                        runs_total_value += value;
                        if is_first {
                            write!(w, "\n")?;
                            is_first = false;
                        } else {
                            write!(w, " + ")?;
                        }

                        write!(w, "{}", RunDisplay(value))?;
                    }

                    write!(w, " = {}", RunDisplay(runs_total_value))?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ScoreSummary<LedgerRunT: LedgerV2> {
    // TODO document fields
    pub away_emoji: String,
    pub away_score: f64,
    pub home_emoji: String,
    pub home_score: f64,
    pub runs_scored: f64, // negative for unruns
    pub ledger: Ledger<LedgerRunT>,
    pub team_id: Uuid,
    pub team_nickname: String,
    pub sub_event: SubEvent,

    /// If this Score inflated Balloons, this is the name of the stadium in which the Balloons were
    /// inflated. Otherwise null.
    // This may need to be extended to support number of balloons.
    pub balloons: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PressureBuilt {
    /// The amount of pressure after this pressure-building event
    pub pressure_after: f64,

    /// Metadata for the SunSunPressure sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
pub enum RenovationBuiltEffect {
    None,
    ModAdded {
        /// Description for the AddedMod sub-event. This description is not currently parsed but
        /// contributions are welcome.
        description: String,

        /// Internal ID of the mod that was added
        mod_id: String,

        /// Metadata for the AddedMod sub-event
        sub_event: SubEvent,
    },
    LightSwitchFlipped {
        /// Name of the stadium that flipped its light switch
        stadium_name: String,

        /// True if the light switch is now on, false otherwise
        is_on: bool,

        /// Metadata for the LightSwitchFlipped sub-event
        sub_event: SubEvent,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ModRemovedFromRatification {
    /// Description of the mod being removed from the stadium. Usually contains the stadium name and
    /// team nickname, but in an inconsistent format so we don't parse it.
    pub description: String,

    /// Uuid of the team whose stadium this is
    pub team_id: Uuid,

    /// Metadata for the RemovedMod sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, IntoPrimitive, JsonSchema, AsRefStr, WithStructure, EnumDisplay, EnumFlattenable)]
#[repr(i64)]
pub enum BracketType {
    Overbracket = 0,
    Underbracket = 1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct EarnedWin {
    /// Nickname of winning team
    pub winning_team_nickname: String,

    /// Uuid of winning team
    pub winning_team_id: Uuid,

    /// Number of Wins the winning team has once the newly earned Win is added
    pub wins_after: i64,

    /// Metadata for the team-earned-win sub-event
    pub sub_event: SubEvent,

    /// If this is a postseason event, whether it's an overbracket or underbracket game. Otherwise
    /// null
    pub bracket_type: Option<BracketType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct ShortEarnedWin {
    /// Nickname of winning team
    pub team_nickname: String,

    /// Number of Wins the winning team has once the newly earned Win is added
    pub wins_after: i64,

    /// Metadata for the team-earned-win sub-event
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct BalloonsPopped {
    /// Name of the stadium in which the balloons were popped
    pub stadium_name: String,

    /// Number of Birds that were scared away
    pub birds_scared_away: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct TookTheFifthBase {
    /// Name of the stadium from which The Fifth Base was taken
    pub stadium_name: String,

    /// Uuid of the team whose player just took The Fifth Base
    pub team_id: Uuid,

    /// Metadata for the sub-event associated with the stadium losing the The Fifth Base mod
    pub remove_mod_from_stadium_sub_event: SubEvent,

    /// The increase or decrease that all the wielding player's items caused to their star rating
    /// before taking The Fifth Base
    pub player_item_rating_before: f64,

    /// The increase or decrease that all the wielding player's items now cause to their star rating
    pub player_item_rating_after: f64,

    /// The player's star rating. TODO: Is this with or without items?
    pub player_rating: f64,

    /// Metadata for the event associated with gaining the The Fifth Base item
    pub player_gained_item_sub_event: SubEvent,

    /// If the player dropped an item as a result of taking The Fifth bAse, contains information
    /// about the dropped item. Otherwise null.
    pub dropped_item: Option<ItemDroppedForNewItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct PlayerTogethernessModChange {
    /// List of the other players with the same togetherness mod. May be empty.
    pub other_player_names: Vec<String>,

    /// Metadata for the associated mod being added/removed
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, WithStructure)]
pub struct RunStolenThroughTunnelsDetails {        
    /// Uuid of the team who had their run stolen
    pub victim_team_id: Uuid,

    /// Name of the team whose player stole the run
    pub thieving_team_nickname: String,

    /// Uuid of the team whose player stole the run
    pub thieving_team_id: Uuid,

    // TODO document fields
    pub away_emoji: String,
    pub away_score: f64,
    pub home_emoji: String,
    pub home_score: f64,

    /// Metadata for the RunsScored event for the team who gained a run
    pub run_gained_sub_event: SubEvent,

    /// Metadata for the RunsScored event for the team who lost a run
    pub run_lost_sub_event: SubEvent,

    /// I can't figure out what determines which team's sub-event goes first, so I have to store
    /// it. If you can see the pattern please let me know.
    // TODO Try to deduce this from data
    pub victim_event_first: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, StrumDisplay, WithStructure)]
pub enum RiffElement {
    #[strum(to_string = "bow")] Bow,
    #[strum(to_string = "bah")] Bah,
    #[strum(to_string = "wah")] Wah,
    #[strum(to_string = "ah")] Ah,
    #[strum(to_string = "doo")] Doo,
    #[strum(to_string = "la")] La,
    #[strum(to_string = "ooo")] Ooo,
    #[strum(to_string = "bee")] Bee,
    #[strum(to_string = "ski")] Ski,
    #[strum(to_string = "ooie")] Ooie,
    #[strum(to_string = "dah")] Dah,
    #[strum(to_string = "da")] Da,
    #[strum(to_string = "louie")] Louie,
    #[strum(to_string = "shoo")] Shoo,
    #[strum(to_string = "boh")] Boh,
    #[strum(to_string = "dee")] Dee,
    #[strum(to_string = "sha")] Sha,
    #[strum(to_string = "doh")] Doh,
    #[strum(to_string = "bop")] Bop,
    #[strum(to_string = "boo")] Boo,
    #[strum(to_string = "do")] Do,
    #[strum(to_string = "bip")] Bip,
    #[strum(to_string = "ska")] Ska,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AsRefStr, WithStructure, EnumFlattenable)]
pub enum TraderTraitor {
    /// The player is described as a Trader
    Trader,

    /// The player is described as a Traitor
    Traitor,

    /// The player is not described as either a Traitor or Trader, but formatting implies they would
    /// be called one of the two.
    ///
    /// Specifically, there is an extra space before their name, which I assume is the space that
    /// would be between the word "Traitor"/"Trader" and their name.
    Unknown,

    /// The player is not described as either a Traitor or Trader, and formatting implies they would
    /// not be called either.
    ///
    /// This does not have the extra space that Unknown has. It appears exactly once, during the
    /// Semi-Centennial, when New Megan Ito traded their nothing for Dunlap Figueroa's The Fifth
    /// Base.
    Neither,
}

impl Display for TraderTraitor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TraderTraitor::Trader => { write!(f, "Trader ") }
            TraderTraitor::Traitor => { write!(f, "Traitor ") }
            TraderTraitor::Unknown => { write!(f, " ") }
            TraderTraitor::Neither => { write!(f, "") }
        }
    }
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
    GameStart {
        #[serde(flatten)]
        game: GameEvent,

        /// Weather for this game
        #[with_structure(ignore)]
        weather: Weather,

        /// Uuid of the stadium this game is being played in, if any
        stadium_id: Option<Uuid>,

        /// What the event type announces. Before season 20 it always announced "Let's Go!", but
        /// starting in season 20 it started announcing the team names.
        announcement: GameStartAnnouncement,
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
        inning: i64,

        /// Full name of the team at bat
        batting_team_name: String,

        /// List of subseasonal mods that came into effect this game. All of these mods add either
        /// Overperforming or Underperforming for the subseason.
        ///
        /// This array is only populated on the first HalfInning event of a game on the first game a
        /// team plays in a given subseason (Earlseason, Midseason, Lateseason, or Postseason). Most
        /// of the time this is the first day of the subseason, but the wildcard rounds in the
        /// Postseason mean that some teams don't have their first game on the first day.
        ///
        /// This is an apparent bug that only started in season 16. Player mod changes get their own
        /// events, and team event changes get reported on the player mod change event, HalfInning
        /// event, or Psycoachoustics event. This list may not be exhaustive.
        team_subseasonal_mod_changes: Vec<SubseasonalModChange<TeamModChangeSubject>>,
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

        /// True if the player is Skipping
        is_skipping: bool,
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
        balls: i64,

        /// Number of strikes in the count
        strikes: i64,

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
        balls: i64,

        /// Number of strikes in the count
        strikes: i64,

        /// Meta about the batter's item breaking, if it broke, otherwise null.
        batter_item_damage: Option<(String, ItemDamaged)>,

        /// If a new Bird found a Birdhouse, this is the total number of birds in this stadium.
        /// Otherwise null (null does not indicate there are no birds, just that there was no
        /// Birdhouse event on this foul ball). Note there can be negative birds.
        birds: Option<i64>,

        /// True if this was a Very foul ball (or balls), false otherwise.
        very_foul: bool,

        /// True if this was an Offworld foul ball (or balls), false otherwise.
        offworld: bool,
    },

    /// Strike, swinging
    #[serde(rename_all = "camelCase")]
    StrikeSwinging {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Number of balls in the count
        balls: i64,

        /// Number of strikes in the count
        strikes: i64,

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
        balls: i64,

        /// Number of strikes in the count
        strikes: i64,

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
        balls: i64,

        /// Number of strikes in the count. Should always be 0, but still present in the data for
        /// forward-compatibility and convenience.
        strikes: i64,

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
        scores: Scores<SimpleLedgerV2<run_source::Flyout>>,

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
        scores: Scores<SimpleLedgerV2<run_source::GroundOut>>,

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

        /// Damage that the pitcher's item took from catching the out, if any
        pitcher_item_damage_from_out: Option<(String, ItemDamaged)>,

        /// Damage that the pitcher's item took from the runner advancing, if any
        pitcher_item_damage_from_advance: Option<(String, ItemDamaged)>,

        /// Damage that the fielder's item took from catching the out, if any
        fielder_item_damage_from_out: Option<ItemDamaged>,

        /// Damage that the fielder's item took from the runner advancing, if any
        fielder_item_damage_from_advance: Option<ItemDamaged>,

        /// If this ground out popped a Flooding Balloon, contains the stadium name and birds scared
        /// away. Otherwise null.
        flood_balloon_popped: Option<BalloonsPopped>,
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
        scores: Scores<SimpleLedgerV2<run_source::FieldersChoice>>,

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
        scores: Scores<SimpleLedgerV2<run_source::DoublePlay>>,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,

        /// If the pitcher's item was damage, includes the pitcher's name and details about the item
        /// being damaged
        pitcher_item_damage: Option<(String, ItemDamaged)>,

        /// If this ground out popped a Flooding Balloon, contains the stadium name and birds scared
        /// away. Otherwise null.
        flood_balloon_popped: Option<BalloonsPopped>,
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
        scores: Scores<SimpleLedgerV2<run_source::Hit>>,

        /// The Spicy status of the batter
        spicy_status: SpicyStatus,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null. The batter is not supposed to cool off when getting a hit, but
        /// on at least one occasion (a16405db-107a-473c-acbf-2834d71834e0) it happened (and on the
        /// same event as they became spicy), presumably because of a bug.
        cooled_off: Option<ModChangeSubEventWithPlayer>,

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

        /// If this was a Holiday Inning, contains the Hotel Motel parties
        hotel_motel_parties: Vec<HotelMotelScoringPlayer>,

        /// If the home run built hype, the metadata about the hype event
        hype: Option<HomeRunHype>,

        /// TODO Describe alley oops
        alley_oop: Option<(String, bool)>,

        /// Starting in s20 there's a separate RunsScored sub-event. This contains that information,
        /// if applicable. There are also effects attached to scoring in general, rather than each
        /// individual Run scored, and those also appear here.
        score_summary: Option<ScoreSummary<HomeRunLedger>>,

        /// If this home run popped some Balloons, this contains the name of the stadium whose
        /// balloons were popped and the number of birds that were scared away.
        balloons_popped: Option<BalloonsPopped>,
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

        /// If this event built hype, the metadata about the hype event
        hype: Option<Hype>,

        /// Score summary effects, if applicable. This will be populated if the season is 20 or
        /// later and either the base stolen was home or if blaserunning is true, otherwise null.
        score_summary: Option<ScoreSummary<StolenBaseLedger>>,

        /// Info about the Hotel Motel party on this score, if any
        hotel_motel_party: Option<HotelMotelParty>,

        /// If the player took The Fifth Base, contains info about the stadium losing the mod, the
        /// player gaining the item, and the player possibly dropping their previous item
        took_the_fifth_base: Option<TookTheFifthBase>,
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

        /// Fielder item damage if any, otherwise null
        // TODO do this in a way that serializes nicely
        fielder_item_damage: Option<(String, ItemDamaged)>,
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

        /// Starting in season 20 the sim started outputting score summary events (RunsScored) and
        /// attaching effects (such as Balloons) to the score summary. This contains that
        /// information. Runs can be scored on Strikeout events thanks to Triple Threat.
        score_summary: Option<ScoreSummary<TripleThreatLedger>>,
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

        /// Starting in season 20 the sim started outputting score summary events (RunsScored) and
        /// attaching effects (such as Balloons) to the score summary. This contains that
        /// information. Runs can be scored on Strikeout events thanks to Triple Threat.
        score_summary: Option<ScoreSummary<TripleThreatLedger>>,
    },

    /// Player drew a walk
    #[serde(rename_all = "camelCase")]
    Walk {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of the batter who drew the walk
        batter_name: String,

        /// Uuid of the batter who drew the walk
        batter_id: Uuid,

        #[serde(flatten)]
        scores: Scores<SimpleLedgerV2<run_source::Walk>>,

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
        inning_num: i64,

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
        num_swings: i64,
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
        ///
        /// Sometimes this information is on the GameOver event instead (TODO: when?)
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
        balls: i64,

        /// Number of strikes in the count
        strikes: i64,

        /// Whether runners advance on the pathetic play (I believe runners always advance if there
        /// are any runners at all)
        runners_advance: bool,

        #[serde(flatten)]
        scores: Scores<SimpleLedgerV2<run_source::MildPitch>>,
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
        scores: Scores<SimpleLedgerV2<run_source::MildPitchWalk>>,
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

        /// Metadata for the sub-event associated with activating Maintenance Mode, if applicable
        // TODO: Should this be on PlayerStatChange?
        maintenance_mode: Option<MaintenanceMode>,

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

        /// If maintenance mode activated, contains metadata about that event. Otherwise null.
        maintenance_mode: Option<SubEvent>,
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
        mods: Vec<ModRemoval>,

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
        mods: Vec<ModRemoval>,

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
    // TODO Unify the two sun2 events?
    #[serde(rename_all = "camelCase")]
    Sun2 {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who earned the Win
        scoring_team_nickname: String,

        /// If a player caught some rays, info about the player's attribute increase, otherwise null
        caught_some_rays: Option<PlayerStatChange>,

        /// Starting in s20, wins had a SubEvent associated with them. This is the metadata for that
        /// sub
        win_event: Option<WinSubEvent>,
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

        /// Starting in s20, wins had a SubEvent associated with them. This is the metadata for that
        /// sub
        win_event: Option<WinSubEvent>,
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

        #[serde(flatten)]
        pitch: GamePitch,

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
        scores: Scores<SimpleLedgerV2<run_source::CharmWalk>>,
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

        /// Starting in s20, there's an additional child event for the weather proc. This is the
        /// information in that event, if applicable.
        weather_event: Option<SubEvent>,
    },

    /// Player suffered a Superallergic reaction
    #[serde(rename_all = "camelCase")]
    SuperallergicReaction {
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

        /// Starting in s20, there's an additional child event for the weather proc. This is the
        /// information in that event, if applicable.
        weather_event: Option<SubEvent>
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

        /// Starting in s20, there's an additional child event for the weather proc. This is the
        /// information in that event, if applicable.
        weather_event: Option<SubEvent>,
    },

    /// Tarot readings
    #[serde(rename_all = "camelCase")]
    TarotReading {
        /// Tarot reading description
        description: String,

        /// Metadata associated with the tarot reading. This is vague on purpose to be generic.
        #[with_structure(ignore)]
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

        /// If this mod removal caused other mods to be removed, this is that
        mods_removed_from_other_mod: Option<ModsFromAnotherModRemovedWithName>,
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

        /// Starting in season 20 the sim started outputting score summary events (RunsScored) and
        /// attaching effects (such as Balloons) to the score summary. This contains that
        /// information. Runs can be scored on Flooding events thanks to Flippers.
        score_summary: Option<ScoreSummary<SimpleLedgerV2<run_source::Flippers>>>,

        /// Whether a flood balloon was filled
        flood_balloon: bool,

        /// Whether the Anti Flood Pumps activated
        anti_flood_pumps: bool,
    },

    /// Player(s) returned from Elsewhere
    #[serde(rename_all = "camelCase")]
    ReturnFromElsewhere {
        #[serde(flatten)]
        game: GameEvent,

        // List of returns from elsewhere. This is almost always a length-1 array, but it is
        // possible for multiple players to return on the same event, and it has happened at least
        // once (Basilos Mason and Fig, Season 18 Day 33). The array should never be empty.
        // TODO: Make this a Nonempty<>? Compile time non-emptiness guarantee.
        returns: Vec<ReturnFromElsewhere>,
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

        /// If a player was Ambushed, information about the ambush. Otherwise null.
        ambush: Option<Ambush>,

        /// In season 20, incinerations started building Sun(Sun)'s Pressure. This holds the
        /// metadata for the pressure building sub-event, if applicable
        pressure_built: Option<PressureBuilt>,

        /// If the Heat Magnet activated on this Incineration, contains the score summary for the
        /// resulting score. Otherwise `null`.
        heat_magnet: Option<ScoreSummary<HeatMagnetLedger>>,
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

        /// If this Party attracted birds, the name of the stadium the birds were attracted to.
        /// Otherwise null
        attracted_birds: Option<String>,
    },

    /// Player was hatched from the Field of Eggs
    // TODO: Can all of these be merged with another event?
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
        place: i64,

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

        /// Name of postseason birth
        postseason_birth_name: String,

        /// Uuid of postseason birth
        postseason_birth_id: Uuid,

        /// Location of postseason birth
        postseason_birth_location: ShadowPositionType,

        /// Metadata for the postseason birth's hatching event
        hatch_event_metadata: SubEvent,

        /// Metadata for the team-left-the-party event, if applicable
        left_party_event_metadata: Option<SubEvent>,

        /// Metadata for the postseason birth's shadow boost, if applicable
        /// (shadow boosts began in Season 18 [TODO: fact check])
        shadow_boost: Option<(KnownPlayerStatChange, PostseasonBirthBoostEventOrder)>,

        /// Metadata for the postseason birth's added-to-team event. This is *almost* always
        /// present, but for unknown reason the Lovers' postseason birth in season 19 was missing
        /// this event. In that case, this will be null.
        postseason_birth_event_metadata: Option<SubEvent>,
    },

    /// Team advanced to next round of the postseason
    #[serde(rename_all = "camelCase")]
    PostseasonAdvance {
        /// Uuid of team who advanced in the postseason
        team_id: Uuid,

        /// Nickname of team who advanced in the postseason
        team_nickname: String,

        /// Round to which the team advanced, or null for the Internet Series
        round: Option<i64>,

        /// One-indexed season number
        displayed_season: i64,
    },

    /// Team was eliminated from the postseason
    #[serde(rename_all = "camelCase")]
    PostseasonEliminated {
        /// Uuid of team who was eliminated from the postseason
        team_id: Uuid,

        /// Nickname of team who was eliminated from the postseason
        team_nickname: String,

        /// One-indexed season number
        displayed_season: i64,

        /// In seasons with an overbracket and underbracket, indicates which bracket this event came
        /// from. Otherwise null.
        bracket: Option<BracketType>,
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

        /// Indicates whether this win was for the Overbracket or Underbracket, or if it was earned
        /// before there was a distinction (indicated by a null value, and equivalent to an
        /// Overbracket win).
        bracket_type: Option<BracketType>,

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
        #[with_structure(ignore)]
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
        #[with_structure(ignore)]
        metadata: EventMetadata,
    },

    /// Subseasonal mods are added or removed from a single team.
    ///
    /// Not all subseasonal mod changes cause this event. Due to what seems to be a bug, in Season
    /// 16 team mod changes stopped being their own event and started being attached to the next
    /// event. This could be a PlayerSubseasonalModsChange event, a HalfInningStart event, a
    /// Psychoacoustics event, and possibly others.
    #[serde(rename_all = "camelCase")]
    TeamSubseasonalModsChange {
        #[serde(flatten)]
        game: GameEvent,

        /// The team subseasonal mod that changed
        #[serde(flatten)]
        change: SubseasonalModChange<TeamModChangeSubject>,
    },

    /// Subseasonal mods are added or removed from a single player (and, due to an apparent bug,
    /// possibly multiple teams).
    ///
    /// See the description of TeamSubseasonalModsChange
    #[serde(rename_all = "camelCase")]
    PlayerSubseasonalModsChange {
        #[serde(flatten)]
        game: GameEvent,

        /// Changes to the team subseasonal mods. Due to what I assume is a bug, multiple of these
        /// may get collected in front of a single player mod change. If there is no player mod
        /// change, the team mod changes will instead get collected on the HalfInning event
        team_changes: Vec<SubseasonalModChange<TeamModChangeSubject>>,

        /// The player subseasonal mod change that triggered this event
        player_change: SubseasonalModChange<PlayerModChangeSubject>,
    },

    /// Decree passed. This event is currently minimally parsed, with metadata simply included
    /// as-is. If you have a use-case where thoroughly parsing this event type would be useful
    /// please let us know in the SIBR discord.
    #[serde(rename_all = "camelCase")]
    DecreePassed {
        /// Title of Decree that passed. This may be redundant with the title in `metadata`
        decree_title: String,

        /// Event metadata exactly as it appears in the Feed event
        #[with_structure(ignore)]
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

        /// Whether the fireproof player was Unstable
        is_unstable: bool,
    },

    /// Team's lineup was sorted as a result of gaining Base Dealing
    #[serde(rename_all = "camelCase")]
    LineupSorted {
        /// Uuid of team whose lineup was just sorted
        team_id: Uuid,

        /// Nickname of team whose lineup was just sorted
        team_nickname: String,
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

        /// TODO Document
        effect: RenovationBuiltEffect,

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
        level: i64,
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

    /// Team used their Free Gift
    // TODO: Combine with TeamUsedFreeWill?
    #[serde(rename_all = "camelCase")]
    TeamUsedFreeGift {
        /// Uuid of team who used their Free Gift
        team_id: Uuid,

        /// Name of team who used their Free Gift
        team_nickname: String,
    },

    /// Player lost a mod
    // TODO: Can this be more specific?
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

    /// The Event Horizon awaits at the beginning of a game
    #[serde(rename_all = "camelCase")]
    EventHorizonAwaits {
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

        /// List of team subseasonal mods that changed on this Psychoacoustics event.
        /// See HalfInningStart.subseasonal_mod_changes for details.
        team_subseasonal_mod_changes: Vec<SubseasonalModChange<TeamModChangeSubject>>,
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
        #[with_structure(ignore)]
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
        inning_num: i64,

        /// Runs lost to the Salmon
        run_losses: RunLossesFromSalmon,

        /// Item repaired or restored by the salmon, if any
        item_repaired: Option<ItemRepaired>,

        /// Player caught in the bind, if any
        player_expelled: Option<PlayerSentElsewhere>,
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

        /// Which type of Debt was applied to the player who was hit by the HBP
        debt_type: DebtType,

        /// Metadata for the event associated with adding the Observed mod
        sub_event: SubEvent,

        #[serde(flatten)]
        scores: Scores<SimpleLedgerV2<run_source::HitByPitch>>,
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

        /// Nickname of team who gained or lost the (Un)runs
        team_nickname: String,

        /// Number of Runs or Unruns gained/lost. This can be negative or positive independently of
        /// whether they are runs or unruns, and also of whether they are gained or lost. This means
        /// there can be a triple negative.
        num_runs: f64,

        /// True if the run objects gained/lost were Unruns, false if they were Runs
        unruns: bool,

        /// True if the run objects were gained, flase if they were lost
        gained: bool,

        /// Starting in season 20 the sim started outputting score summary events (RunsScored) and
        /// attaching effects (such as Balloons) to the score summary. This contains that
        /// information.
        score_summary: Option<ScoreSummary<OverflowLedger>>,
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

        /// When detectives enter the Secret Base (TODO: Every time?) they sense a Deep Darkness.
        /// This is the metadata for that sub-event, if it exists. Otherwise null.
        deep_darkness: Option<SubEvent>,
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

        /// Whether the player exited directly to The Fifth Base. This is only possible if the
        /// stadium currently has The Fifth Base. If this is false, the player exited to second
        /// base.
        to_fifth: bool,
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

        /// Location of player on the new team. If the player roamed from the team, this is also
        /// the location on their old team
        location: PositionType,

        /// Uuid of player's new team
        new_team_id: Uuid,

        /// Nickname of player's new team
        new_team_nickname: String,

        /// Where the player roamed from, either another team or the Hall of Flame
        roam_from: RoamFromLocation,
    },

    /// Player Roamed at the end of the Week
    #[serde(rename_all = "camelCase")]
    SuperRoam {
        /// Uuid of player who roamed
        player_id: Uuid,

        /// Name of player who roamed
        player_name: String,

        /// Location of player on the new team. If the player roamed from the team, this is also
        /// the location on their old team
        location: PositionType,

        /// Uuid of player's new team
        new_team_id: Uuid,

        /// Nickname of player's new team
        new_team_nickname: String,

        // TODO document these
        previous_team_id: Uuid,
        previous_team_nickname: String,
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
    // TODO Try to combine each of these with the event that removed the source mod
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

    /// A Consumer was defended by a detective
    #[serde(rename_all = "camelCase")]
    ConsumerDefended {
        #[serde(flatten)]
        game: GameEvent,

        /// Exclamation that starts this event, like "SLAM" or "THUD"
        // TODO should this be an enum?
        exclamation: String,

        /// Verb for this consumer defense event, like "POWERBOMBED" or "TOASTED"
        // TODO should this be an enum?
        verb: String,

        /// Name of player who defended the attack, in all caps
        defender_name_caps: String,

        /// Uuid of player who defended the attack
        defender_id: Uuid,

        /// Uuid of player who was targeted by the Consumer
        targeted_player_id: Uuid,
    },

    /// Walk as a result of a Mind Trick
    #[serde(rename_all = "camelCase")]
    MindTrickWalk {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

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
        // TODO is this needed?
        // pitcher_item_damage: Option<ItemDamaged>,
        //
        // /// Meta about the batter's item breaking, if it broke, otherwise null.
        // TODO is this needed?
        // batter_item_damage: Option<ItemDamaged>,

        #[serde(flatten)]
        scores: Scores<SimpleLedgerV2<run_source::MindTrickWalk>>,
    },

    /// Walk as a result of a Mind Trick which overrode a Charm strikeout
    #[serde(rename_all = "camelCase")]
    CharmedMindTrickWalk {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Uuid of the pitcher that did the charm
        pitcher_id: Uuid,

        /// Name of the pitcher that did the charm
        pitcher_name: String,

        /// Uuid of the batter that did the mind trick
        batter_id: Uuid,

        /// Name of the batter that did the mind trick
        batter_name: String,

        // Item damages would go here but I haven't encountered one yet so I haven't put it in

        #[serde(flatten)]
        scores: Scores<SimpleLedgerV2<run_source::CharmedMindTrickWalk>>,
    },

    /// Strikeout as a result of a Mind Trick ("strikes out thinking"). From the introduction of
    /// Mind Tricks until s18d43, mind trick strikeouts were under the Walk event type. From s18d94
    /// onward, they were under the Strikeout event type. (There were no mind trick strikeouts
    /// between those two game days.)
    #[serde(rename_all = "camelCase")]
    MindTrickStrikeout {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

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

        /// True if the attempted sipper was a Siphon, false otherwise
        is_siphon: bool,

        /// Uuid of the player that attempted to blooddrain the Sealed player
        sipper_id: Uuid,

        /// Name of the player that attempted to blooddrain the Sealed player
        sipper_name: String,

        /// Uuid of the Sealed player
        sippee_id: Uuid,

        /// Name of the Sealed player
        sippee_name: String,
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
        /// As with many of these ratings, it can be `null` for reasons I don't yet understand.
        player_item_rating_before: Option<f64>,

        /// The increase or decrease that all the wielding player's items now cause to their star rating
        /// As with many of these ratings, it can be `null` for reasons I don't yet understand.
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

        /// If this player has the Yolked mod from being on a team with another Hard Boiled player,
        /// it is momentarily lost and then regained. This is the events for those, in order.
        // TODO Make struct not tuple
        yolked_change: Option<(PlayerTogethernessModChange, PlayerTogethernessModChange)>
    },

    /// A Redacted event
    #[serde(rename_all = "camelCase")]
    Redacted {
        /// Event description, which seems to contain only "|" characters and spaces
        description: String,

        /// Number of upscales. This is like nuts but for Redacted events
        scales: i64,
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
        inning_number: i64,
    },

    /// Team applies Home Field Advantage
    #[serde(rename_all = "camelCase")]
    HomeFieldAdvantage {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who applied Home Field advantage (this will always be the home team)
        team_nickname: String,
    },

    /// Prize Match announcement
    #[serde(rename_all = "camelCase")]
    PrizeMatch {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the Item that the Winner will get
        item_name: String,
    },

    /// Team won a Prize Match
    #[serde(rename_all = "camelCase")]
    WonPrizeMatch {
        /// There are two formats for this event. In the first format, the nickname of team who won
        /// the Prize Match is mentioned, but not the player name. In the second, it's the reverse.
        // TODO: Serialize this as either team_nickname or player_name
        team_nickname_or_player_name: TeamNicknameOrPlayerName,

        /// Uuid of team who won the Prize Match
        team_id: Uuid,

        /// Uuid of player who got the Prize. Oddly, the player's name is not mentioned.
        player_id: Uuid,

        /// Uuid of the Item that the winner got
        item_id: Uuid,

        /// Name of the Item that the winner got
        item_name: String,

        /// Mods that the Item bestows, as a list of internal IDs
        item_mods: Vec<String>,

        /// The increase/decrease that all the wielding player's items caused to their star rating
        /// before gaining the item
        player_item_rating_before: f64,

        /// The increase/decrease that all the wielding player's items now cause to their star
        /// rating. This is sometimes null for reasons which are unknown to me.
        player_item_rating_after: Option<f64>,

        /// The player's star rating. TODO: Is this with or without items?
        player_rating: f64,
    },

    /// Team wins gifts from the Gift Shop.
    #[serde(rename_all = "camelCase")]
    TeamReceivedGifts {
        // TODO: Document these fields. I suppose I should verify that they do what they obviously
        //   are meant to do.
        recipient: Uuid,
        top_3_benefactor_coins: [i64; 3],
        top_3_benefactors: [Uuid; 3],
        total_benefactor_coins: i64,
        total_gifts: i64,
    },

    /// Team received a Gift. This event is currently minimally parsed, with metadata simply
    /// included as-is. If you have a use-case where thoroughly parsing this event type would be
    /// useful please let us know in the SIBR discord.
    // TODO: Now that I decided to open the "combining events" can of worms, should this be 
    //   combined with TeamReceivedGifts?
    #[serde(rename_all = "camelCase")]
    GiftReceived {
        /// Uuid of the team that received the gift
        team_id: Uuid,

        /// Title of Gift that was received along with the name of who received it. If you have a
        /// use case where having these separate would be useful, let us know. This may be redundant
        /// with the title and team name in `metadata`
        title_and_recipient: String,

        /// Event metadata exactly as it appears in the Feed event
        #[with_structure(ignore)]
        metadata: EventMetadata,

        // TODO Figure out what should happen here
        successors: Vec<EventuallyEvent>
    },

    /// Replica player faded to dust at the end of the season
    #[serde(rename_all = "camelCase")]
    ReplicaFadedToDust {
        /// Uuid of the team whose replica faded
        team_id: Uuid,

        /// Nickname of the team whose replica faded
        team_nickname: String,

        /// Uuid of the player who faded
        player_id: Uuid,

        /// Name of the player who faded
        player_name: String,

        /// Metadata for the associated ModAdded event for adding the Dust mod
        mod_added_event: SubEvent,

        /// If a replica of a Hard Boiled player fades to dust while Yolked, they'll lose the Yolked
        /// mod. This is metadata for that event.
        weaker_apart_event: Option<PlayerTogethernessModChange>,
    },

    /// Team with A Blood gets A blood type at the beginning of a game
    #[serde(rename_all = "camelCase")]
    ABloodType {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the team whose blood triggered
        team_id: Uuid,

        /// Nickname of the team whose blood triggered
        team_nickname: String,

        /// Mod ID of the blood mod the team gained
        blood_type_mod_id: String,

        /// Metadata for the associated ModAdded event
        sub_event: SubEvent,
    },

    /// Polarity weather shifts between Positive and Negative polarity
    #[serde(rename_all = "camelCase")]
    PolarityShift {
        #[serde(flatten)]
        game: GameEvent,

        /// Which way numbers go, i.e. which Polarity weather is now active
        numbers_go: NumbersGo,

        /// Metadata for the WeatherChange sub event
        sub_event: SubEvent,
    },

    /// Donated shame, which the team received by being shamed by a team with Shame Donor, are
    /// applied at the start of the next game
    #[serde(rename_all = "camelCase")]
    DonatedShameApplied {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who was shamed and is now receiving the shame unruns
        team_nickname: String,

        /// Number of unruns received
        unruns: f64,

        /// If after s20, the associated score summary
        score_summary: Option<ScoreSummary<SimpleLedgerV2<run_source::DonatedShame>>>,
    },

    /// Game Over event which bestows the Win object on the winning team. This event did not exist
    /// until s20
    #[serde(rename_all = "camelCase")]
    GameOver {
        #[serde(flatten)]
        game: GameEvent,

        /// The Earned Win event data. After s20 this data always exists somewhere, but it may be
        /// attached to different events.
        earned_win: Option<EarnedWin>,

        /// Players who were Carcinized and are now being returned to their original team at the end
        /// of the game.
        ///
        /// Arguably much of this information is redundant, since there were never any instances
        /// where a non-Crabs team triggered carcinization, and it only ever moves lineup players.
        /// I may compress it more in the future.
        temp_stolen_players_returned: Vec<PlayerMovedTeams>,
    },

    /// "<Team> inflated 10 Balloons!" event that occurs when the home team wins a game and their
    /// stadium has the Balloons modifier.
    #[serde(rename_all = "camelCase")]
    BalloonsCollectedFromWin {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the stadium that inflated the balloons. This will be the winning team's stadium
        /// and the home team's stadium (this event only occurs when the home team wins).
        stadium_name: String,

        /// The Earned Win event data. Before s21d81 this field was always populated on this event.
        /// It "steals" the value from the following GameOver event. On that day, the value was
        /// instead on the preceding GameEnd event. TODO: Figure out if this is a change that stuck
        /// around, or if it only happens in certain circumstances (it may be notable that a
        /// voicemail happened in the s21d81 game in question)
        earned_win: Option<EarnedWin>,
    },

    /// Team practices Moderation
    #[serde(rename_all = "camelCase")]
    Moderation {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of team who practiced Moderation
        team_nickname: String,

        /// Once, due to a bug, Moderation accidentally took too many runs and caused the opposing
        /// team to win. Since this was at the end of the game it counted as Shame and built Hype.
        hype: Option<Hype>,

        /// The associated score summary, if applicable.
        score_summary: Option<ScoreSummary<ModerationLedger>>,
    },

    /// Player placed and stole to The Fifth Base
    #[serde(rename_all = "camelCase")]
    PlacedFifthBase {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who placed and stole to The Fifth Base
        player_id: Uuid,

        /// Name of player who placed and stole to The Fifth Base
        player_name: String,

        /// Team uuid of player who placed and stole to The Fifth Base
        player_team_id: Uuid,

        /// Name of stadium the player put The Fifth Base down in
        stadium_name: String,

        /// The increase or decrease that all the wielding player's items caused to their star rating
        /// before putting down The Fifth Base
        player_item_rating_before: f64,

        /// The increase or decrease that all the wielding player's items now cause to their star rating
        player_item_rating_after: f64,

        /// TODO: Is this the player's rating before or after putting down the Base?
        player_rating: f64,

        /// Metadata for the player-lost-item sub-event
        player_lost_item_event: SubEvent,

        /// Metadata for the stadium-gained-mod sub-event
        stadium_gained_mod_event: SubEvent,
    },

    /// Event Horizon activates, stops the Black Hole from swallowing the runs, and converts them to
    /// Unruns for the away team's next game
    #[serde(rename_all = "camelCase")]
    EventHorizonActivates {
        #[serde(flatten)]
        game: GameEvent,

        /// Number of unruns saved for the victim team's next game
        num_unruns: f32,

        /// Nickname of the team who will receive the unruns (always the away team)
        away_team_nickname: String,
    },

    /// A Renovation was Ratified
    #[serde(rename_all = "camelCase")]
    RenovationRatified {
        /// Name of the renovation that was Ratified
        renovation_name: String,

        /// Internal ID of the renovation that was ratified. These are in lower snake case.
        renovation_id: String,

        /// Internal ID of the mod granted by the renovation that was ratified. These are in upper
        /// snake case.
        mod_id: String,

        mod_removals: Vec<ModRemovedFromRatification>,
    },

    /// A player stole a Run from their opponent using the Stadium's Tunnels
    #[serde(rename_all = "camelCase")]
    RunStolenThroughTunnels {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the player who stole the run
        thieving_player_name: String,

        /// Uuid of the player who stole the run
        thieving_player_id: Uuid,

        /// Nickname of the team who had their run stolen
        victim_team_nickname: String,

        /// More details about the stolen run, if they exist. These details exist for almost every 
        /// event of this type, but presumably due to a bug there were two occasions where a 
        /// RunStolenThroughTunnels event did not have any children (event ids 
        /// dd244af4-c5d1-4bd0-b2f4-9d7b1e11f2f7 and 4338a482-f7eb-448c-9827-e9220f2e86a4), and 
        /// those children are where this info can be found.
        details: Option<RunStolenThroughTunnelsDetails>,

        /// If balloons were inflated on this run theft, contains the name of the stadium. This will
        /// always be the home stadium. Also, this will always be exactly 1 balloon.
        balloons: Option<String>,

        /// If this run activated Hype, information about the hype. Ohterwise null.
        hype: Option<Hype>,

        /// Free Refill data if one was used, otherwise null
        free_refill: Option<FreeRefill>,
    },

    /// A player tried to steal an item from an opponent player using the Stadium's Tunnels, but
    /// was caught and fled Elsewhere.
    #[serde(rename_all = "camelCase")]
    CaughtStealingItemWithTunnels {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who was caught stealing and fled Elsewhere. This player is always on
        /// the home team.
        thief_id: Uuid,

        /// Name of the player who was caught stealing and fled Elsewhere. This player is always on
        /// the home team.
        thief_name: String,

        /// Uuid of the player whose item the thief wanted to steal. This player is always on the
        /// away team.
        victim_id: Uuid,

        /// Name of the player whose item the thief wanted to steal. This player is always on the
        /// away team.
        victim_name: String,

        /// Name of the item the thief wanted to steal
        item_name: String,

        /// Metadata for the apparently useless sub-event that repeats the parent event but with the
        /// CaughtStealingItemFromTunnels event type
        caught_stealing_item_sub_event: SubEvent,

        /// Metadata for the sub-event associated with adding the Elsewhere mod, if applicable.
        /// Sometimes this didn't exist and I don't know why.
        fled_elsewhere_sub_event: Option<SubEvent>,

        /// If the player was flipped negative, this is information about that
        // TODO: This should be inside fled_elsewhere_sub_event, because you can't have this without
        //   that
        flipped_negative: Option<FlipNegative>,
    },

    /// A player stole an item from an opponent player using the Stadium's Tunnels
    #[serde(rename_all = "camelCase")]
    StoleItemWithTunnels {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who stole the item. This player is always on the home team.
        thief_id: Uuid,

        /// Name of the player who stole the item. This player is always on the home team.
        thief_name: String,

        /// Uuid of the player whose item was stolen. This player is always on the away team.
        victim_id: Uuid,

        /// Name of the player whose item was stolen. This player is always on the away team.
        victim_name: String,

        /// Uuid of the team whose player's item was stolen. This *should* always be the away team,
        /// because only the home team can use the Tunnels, but thanks to the linked items bug it
        /// can be a team that's not even in this game!
        ///
        /// The thief's team is always the home team, though.
        victim_team_id: Uuid,
        
        /// Uuid of the item that was stolen
        item_id: Uuid,
        
        /// Name of the item that was stolen
        item_name: String,
        
        /// List of mods that this item grants. Each element is the internal id of a mod.
        item_mods: Vec<String>,

        /// The increase/decrease that all the thief's items caused to their star rating before 
        /// gaining this item
        thief_item_rating_before: f64,

        /// The increase/decrease that all the thief's items now cause to their star rating
        thief_item_rating_after: f64,

        /// The thief's star rating. TODO: Is this with or without items?
        thief_rating: f64,

        /// The increase/decrease that all the victim's items caused to their star rating before 
        /// gaining this item
        ///
        /// For reasons currently unknown to me, some items (like the Smokey Plant-Based Sunglasses
        /// of Intelligence) have a `null` for one or more of their `Rating` properties. That causes
        /// this value to be `null` when the player loses that item.
        victim_item_rating_before: Option<f64>,

        /// The increase/decrease that all the victim's items now cause to their star rating
        victim_item_rating_after: f64,

        /// The victim's star rating. TODO: Is this with or without items?
        victim_rating: f64,

        /// Metadata for the apparently useless sub-event that repeats the parent event but with the
        /// StoleItemFromTunnels event type
        stole_item_sub_event: SubEvent,

        /// Metadata for the victim losing the item
        item_lost_sub_event: SubEvent,

        /// Metadata for the thief dropping the item they previously had, if applicable
        thief_item_dropped: Option<ItemDroppedForNewItem>,

        /// Metadata for the thief gaining the item
        item_gained_sub_event: SubEvent,
    },

    /// A player entered the Tunnels but didn't find anything interesting
    #[serde(rename_all = "camelCase")]
    NothingInterestingInTunnels {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the player who stole the item. This player is always on the home team.
        thief_id: Uuid,

        /// Name of the player who stole the item. This player is always on the home team.
        thief_name: String,

        /// Metadata for the apparently useless sub-event that repeats the parent event but with the
        /// FoundNothingInterestingInTunnels event type
        sub_event: SubEvent,
    },

    /// Sun(Sun) recharged at the end of the Season.
    ///
    /// As far as I'm aware, Sun(Sun)'s maximum pressure is always 99999 and the recharge value is
    /// always 26244, so those values are not stored.
    #[serde(rename_all = "camelCase")]
    SunSunRecharged {
        /// The pressure after recharge
        pressure_after: f64,
    },

    /// Sun 30 smiled upon both teams in a game. This happens whenever a game reaches extra innings
    /// and the Sun 30 rule is active
    #[serde(rename_all = "camelCase")]
    Sun30Smiles {
        #[serde(flatten)]
        game: GameEvent,

        /// Metadata for the away team's Win
        away: ShortEarnedWin,

        /// Metadata for the home team's Win, if the corresponding earned-a-Win sub-event exists.
        /// Otherwise, just the team's nickname.
        ///
        /// The earned-a-Win sub-event doesn't always exist (see event
        /// d7f39a58-f148-4506-a59f-7e14c3680d55) but I don't know why. If you know why, please
        /// contact beiju in the SIBR discord.
        home: Either<ShortEarnedWin, String>,

        /// If Balloons were inflated as a result of this Win, this is the name of the Stadium.
        /// Otherwise `null`. The stadium is always the home stadium, and the number of Balloons
        /// inflated is always 10.
        balloons: Option<String>,
    },

    /// Voicemail activates. A player on the home team is swapped with a player in the Shadows
    #[serde(rename_all = "camelCase")]
    Voicemail {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of player who is being replaced
        replaced_player_id: Uuid,

        /// Name of player who is being replaced
        replaced_player_name: String,

        /// Uuid of replacement player
        replacement_player_id: Uuid,

        /// Name of replacement player
        replacement_player_name: String,

        /// Uuid of team whose player got voicemailed
        team_id: Uuid,

        /// Nickname of team whose player got voicemailed
        team_nickname: String,

        /// Metadata associated with the player swap event
        swap_sub_event: SubEvent,

        /// Metadata associated with the player shadowed event
        shadowed_sub_event: PlayerBoostSubEvent,
    },

    /// A team smashed through the Bad Gateway
    #[serde(rename_all = "camelCase")]
    BadGatewayBroken {
        /// Uuid of the team who smashed through the Bad Gateway
        team_id: Uuid,
    },

    /// There were tumbleweed sounds
    #[serde(rename_all = "camelCase")]
    TumbleweedSounds {
        /// Uuid of the team who caused the tumbleweed sounds
        team_id: Uuid,
    },

    /// Pitcher intentionally gave batter a walk. Only known cases are detective pitchers walking
    /// Debted batters.
    #[serde(rename_all = "camelCase")]
    IntentionalWalk {
        #[serde(flatten)]
        game: GameEvent,

        #[serde(flatten)]
        pitch: GamePitch,

        /// Name of the batter who was walked
        batter_name: String,

        /// Uuid of the batter who was walked
        batter_id: Uuid,

        /// Name of pitcher who intentionally gave up the walk
        pitcher_name: String,

        /// UUid of pitcher who intentionally gave up the walk
        pitcher_id: Uuid,

        /// Metadata associated with the "sensed foul play" sub-event, if present
        // TODO When is it present? Theory: only the first time
        sensed_foul_play_sub_event: Option<SubEvent>,
    },

    /// Player sought out a trade, but nothing caught their eye
    NothingToTrade {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the player who sought out the trade
        trader_name: String,

        /// Uuid of the player who sought out the trade
        trader_id: Uuid,

        /// Uuid of the player who the trader looked at
        victim_id: Uuid,

        /// Sub-event associated with finding nothing to trade. Not sure why this requires a
        /// sub-event.
        sub_event: SubEvent,
    },

    /// Trader or Traitor did a trade
    Trade {
        #[serde(flatten)]
        game: GameEvent,

        /// Whether the player who made this trade was a Trader (takes items from a member of the
        /// opponent team) or Traitor (takes items from a member of their own team).
        trader_traitor: TraderTraitor,

        /// Name of the player who sought out the trade
        trader_name: String,

        /// Uuid of the player who sought out the trade
        trader_id: Uuid,

        /// Name of the item that the trader gave to the victim
        donated_item_name: String,

        /// Uuid of the item that the trader gave to the victim
        donated_item_id: Uuid,

        /// Mods the trader gained by switching items
        trader_mods_gained: Vec<String>,

        /// Mods the trader lost by switching items
        trader_mods_lost: Vec<String>,

        /// Trader's item rating before the swap
        ///
        /// Can be null under unidentified circumstances (see c5d43346-7242-4bd9-9b9e-4bcb14c6cca0)
        trader_item_rating_before: Option<f64>,

        /// Trader's item rating after the swap
        ///
        /// Can be null under unidentified circumstances (see 69b1e333-db49-40e7-bcf6-432814f0c391)
        trader_item_rating_after: Option<f64>,

        /// Trader's total rating
        trader_rating: f64,

        /// Metadata for the sub-event associated with the trader changing items
        trader_item_change_sub_event: SubEvent,

        /// Name of the player whose item the trader took
        victim_name: String,

        /// Uuid of the player whose item the trader took
        victim_id: Uuid,

        /// Name of the item that the trader took from the victim
        taken_item_name: String,

        /// Uuid of the item that the trader took from the victim
        taken_item_id: Uuid,

        /// Mods the victim gained by switching items
        victim_mods_gained: Vec<String>,

        /// Mods the victim lost by switching items
        victim_mods_lost: Vec<String>,

        /// Victim's item rating before the swap
        ///
        /// Can be null under unidentified circumstances (see c5d43346-7242-4bd9-9b9e-4bcb14c6cca0)
        victim_item_rating_before: Option<f64>,

        /// Victim's item rating after the swap
        ///
        /// Can be null under unidentified circumstances (see c5d43346-7242-4bd9-9b9e-4bcb14c6cca0)
        victim_item_rating_after: Option<f64>,

        /// Victim's total rating
        victim_rating: f64,

        /// Metadata for the sub-event associated with the victim changing items
        victim_item_change_sub_event: SubEvent,
    },

    /// Player tried to trade with another player, but the other had nothing to offer. This event is
    /// very similar to NothingToTrade, but has a slightly different message that names the victim.
    /// I don't know what the difference is, but the two event messages intermingle so it's not just
    /// a case of the message changing.
    NothingToOffer {
        #[serde(flatten)]
        game: GameEvent,

        /// Name of the player who sought out the trade
        trader_name: String,

        /// Uuid of the player who sought out the trade
        trader_id: Uuid,

        /// Name of the player who the trader looked at
        victim_name: String,

        /// Uuid of the player who the trader looked at
        victim_id: Uuid,

        /// Sub-event associated with finding nothing to trade. Not sure why this requires a
        /// sub-event.
        sub_event: SubEvent,
    },

    /// Player tried and failed to Roam. The only observed instances of this were Parker MacMillan
    /// trying to Roam out of the Vault and being blocked by The Force Field
    RoamFailed {
        /// Name of the player who tried and failed to Roam
        player_name: String,
        
        /// Uuid of the player who tried and failed to Roam
        player_id: Uuid,
    },

    /// A team's Thieves Guild stole a player from their opponents' Shadows
    ThievesGuildStolePlayer {
        #[serde(flatten)]
        game: GameEvent,

        /// Uuid of the thieving team.
        thieving_team_id: Uuid,

        /// Nickname of the thieving team.
        thieving_team_nickname: String,

        /// Name of the stadium owned by the thieving team. This is how that team's thieves' guild
        /// is identified.
        thieving_team_stadium_name: String,

        /// Uuid of the team whose player was stolen.
        victim_team_id: Uuid,

        /// Nickname of the team whose player was stolen.
        victim_team_nickname: String,

        /// Uuid of the player who was stolen
        stolen_player_id: Uuid,

        /// Name of the player who was stolen
        stolen_player_name: String,

        /// Metadata for the sub-event associated with the player moving teams
        player_moved_teams_sub_event: SubEvent,

        /// Metadata for the player's shadow boost
        player_shadows_boost: PlayerBoostSubEvent,
    },

    /// A team's Thieves Guild stole an item from a player from their opponents' Shadows
    ThievesGuildStoleItem {
        #[serde(flatten)]
        game: GameEvent,

        /// Nickname of the thieving team.
        thieving_team_nickname: String,

        /// Name of the stadium owned by the thieving team. This is how that team's thieves' guild
        /// is identified.
        thieving_team_stadium_name: String,

        /// Name of player who gained the stolen item
        beneficiary_player_name: String,

        /// Information associated with the thieving team's player gaining the item
        beneficiary_gained_item: ItemGained,

        /// Uuid of the team who lost the item
        victim_team_id: Uuid,

        /// Nickname of the team who lost the item
        victim_team_nickname: String,

        /// Uuid of player who lost the item
        victim_player_id: Uuid,

        /// Name of player who lost the item
        victim_player_name: String,

        /// Information associated with the victim team's player losing the item
        victim_lost_item: ItemLost,
    },

    /// When a Riff opens in Jazz weather and changes the weather
    RiffOpened {
        #[serde(flatten)]
        game: GameEvent,

        /// The riff that was played
        riff: Vec<RiffElement>,

        /// The weather that Jazz changed to
        new_weather: Weather,
    },

    /// When the Band begins to Play during Polarity weather and changes the weather to Jazz
    ///
    /// This only happened once ever:
    /// https://reblase.sibr.dev/game/945e65e3-afb4-488b-84d0-613f5c39fa10#1a1471ec-406e-f315-8f57-daa15527fd88
    BandBeginsToPlay {
        #[serde(flatten)]
        game: GameEvent,

        /// Which variant of Polarity weather it was before the switch
        numbers_went: NumbersGo,

        /// Sub-event associated with the weather changing to Jazz
        sub_event: SubEvent,
    }
}

#[derive(Debug, Clone, PartialEq, Copy, Serialize, Deserialize, JsonSchema, WithStructure, IntoPrimitive, TryFromPrimitive)]
#[repr(i64)]
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
    pub tournament: i64,

    /// Zero-indexed season
    pub season: i64,

    /// Zero-indexed day
    pub day: i64,

    /// Phase of the sim. Corresponds to the schedule section on the Blaseball homepage, with a few
    /// extra entries.
    pub phase: SimPhase,

    /// The number of times this event has been upshelled
    pub nuts: i64,

    /// The event type and specific event-specific data
    #[serde(flatten)]
    #[serde(with = "FedEventData")]
    pub data: FedEventData,
}

impl FedEventData {
    pub fn game(&self) -> Option<&GameEvent> {
        match self {
            FedEventData::BeingSpeech { .. } => { None }
            FedEventData::GameStart { game, .. } => { Some(game) }
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
            FedEventData::SuperallergicReaction { game, .. } => { Some(game) }
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
            FedEventData::DecreePassed { .. } => { None }
            FedEventData::PlayerJoinedILB { .. } => { None }
            FedEventData::PlayerPermittedToStay { .. } => { None }
            FedEventData::FireproofIncineration { game, .. } => { Some(game) }
            FedEventData::LineupSorted { .. } => { None }
            FedEventData::Undersea { game, .. } => { Some(game) }
            FedEventData::RenovationBuilt { .. } => { None }
            FedEventData::PeanutMister { game, .. } => { Some(game) }
            FedEventData::PlayerNamedMvp { .. } => { None }
            FedEventData::BirdsUnshell { game, .. } => { Some(game) }
            FedEventData::ReplaceReturnedPlayerFromShadows { .. } => { None }
            FedEventData::PlayerCalledBackToHall { .. } => { None }
            FedEventData::TeamUsedFreeWill { .. } => { None }
            FedEventData::TeamUsedFreeGift { .. } => { None }
            FedEventData::PlayerLostMod { .. } => { None }
            FedEventData::InvestigationMessage { .. } => { None }
            FedEventData::HighPressure { game, .. } => { Some(game) }
            FedEventData::PlayerPulledThroughRift { .. } => { None }
            FedEventData::PlayerLocalized { .. } => { None }
            FedEventData::Echo { game, .. } => { Some(game) }
            FedEventData::SolarPanelsAwait { game, .. } => { Some(game) }
            FedEventData::EventHorizonAwaits { game, .. } => { Some(game) }
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
            FedEventData::EnterCrimeScene { game, .. } => { Some(game) }
            FedEventData::ReturnFromInvestigation { .. } => { None }
            FedEventData::InvestigationConcluded { .. } => { None }
            FedEventData::GrindRail { game, .. } => { Some(game) }
            FedEventData::EnterSecretBase { game, .. } => { Some(game) }
            FedEventData::ExitSecretBase { game, .. } => { Some(game) }
            FedEventData::EchoChamber { game, .. } => { Some(game) }
            FedEventData::Roam { .. } => { None }
            FedEventData::SuperRoam { .. } => { None }
            FedEventData::GlitterCrate { game, .. } => { Some(game) }
            FedEventData::ModsFromAnotherModRemoved { .. } => { None }
            FedEventData::ConsumerExpelled { game, .. } => { Some(game) }
            FedEventData::ConsumerDefended { game, .. } => { Some(game) }
            FedEventData::MindTrickWalk { game, .. } => { Some(game) }
            FedEventData::CharmedMindTrickWalk { game, .. } => { Some(game) }
            FedEventData::MindTrickStrikeout { game, .. } => { Some(game) }
            FedEventData::BlooddrainBlocked { game, .. } => { Some(game) }
            FedEventData::TarotReadingAddedOrRemovedItem { .. } => { None }
            FedEventData::CommunityChestOpens { .. } => { None }
            FedEventData::PlayerDropsItem { .. } => { None }
            FedEventData::CommunityChestGameMessage { game, .. } => { Some(game) }
            FedEventData::TeamSubseasonalModsChange { game, .. } => { Some(game) }
            FedEventData::PlayerSubseasonalModsChange { game, .. } => { Some(game) }
            FedEventData::Fax { game, .. } => { Some(game) }
            FedEventData::Redacted { .. } => { None }
            FedEventData::Smithy { game, .. } => { Some(game) }
            FedEventData::HolidayInning { game, .. } => { Some(game) }
            FedEventData::HomeFieldAdvantage { game, .. } => { Some(game) }
            FedEventData::PrizeMatch { game, .. } => { Some(game) }
            FedEventData::WonPrizeMatch { .. } => { None }
            FedEventData::TeamReceivedGifts { .. } => { None }
            FedEventData::GiftReceived { .. } => { None }
            FedEventData::ReplicaFadedToDust { .. } => { None }
            FedEventData::ABloodType { game, .. } => { Some(game) }
            FedEventData::PolarityShift { game, .. } => { Some(game) }
            FedEventData::DonatedShameApplied { game, .. } => { Some(game) }
            FedEventData::GameOver { game, .. } => { Some(game) }
            FedEventData::BalloonsCollectedFromWin { game, .. } => { Some(game) }
            FedEventData::Moderation { game, .. } => { Some(game) }
            FedEventData::PlacedFifthBase { game, .. } => { Some(game) }
            FedEventData::EventHorizonActivates { game, .. } => { Some(game) }
            FedEventData::RenovationRatified { .. } => { None }
            FedEventData::RunStolenThroughTunnels { game, .. } => { Some(game) }
            FedEventData::CaughtStealingItemWithTunnels { game, .. } => { Some(game) }
            FedEventData::StoleItemWithTunnels { game, .. } => { Some(game) }
            FedEventData::NothingInterestingInTunnels { game, .. } => { Some(game) }
            FedEventData::SunSunRecharged { .. } => { None }
            FedEventData::Sun30Smiles { game, .. } => { Some(game) }
            FedEventData::Voicemail { game, .. } => { Some(game) }
            FedEventData::BadGatewayBroken { .. } => { None }
            FedEventData::TumbleweedSounds { .. } => { None }
            FedEventData::IntentionalWalk { game, .. } => { Some(game) }
            FedEventData::NothingToTrade { game, .. } => { Some(game) }
            FedEventData::Trade { game, .. } => { Some(game) }
            FedEventData::NothingToOffer { game, .. } => { Some(game) }
            FedEventData::RoamFailed { .. } => { None }
            FedEventData::ThievesGuildStolePlayer { game, .. } => { Some(game) }
            FedEventData::ThievesGuildStoleItem { game, .. } => { Some(game) }
            FedEventData::RiffOpened { game, .. } => { Some(game) }
            FedEventData::BandBeginsToPlay { game, .. } => { Some(game) }
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