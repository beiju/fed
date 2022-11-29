use std::fmt::{Display, Formatter, Write};
use std::iter;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;
use fed_api::{EventMetadata, EventType, EventCategory, EventuallyEvent, Weather, builder::*};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use derive_builder::Builder;
use schemars::JsonSchema;
use crate::parse::error::FeedParseError;

#[derive(Debug, Clone, Serialize, JsonSchema, IntoPrimitive, TryFromPrimitive)]
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
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
}


impl GameEvent {
    pub fn try_from_event(event: &EventuallyEvent) -> Result<Self, FeedParseError> {
        let (&game_id, ) = event.game_tags.iter().collect_tuple()
            .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "game" })?;

        // Order is very important here
        let (&away_team, &home_team) = event.team_tags.iter().collect_tuple()
            .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "team" })?;

        Self::try_from_event_with_teams(event, game_id, away_team, home_team)
    }

    pub fn try_from_event_extra_teams(event: &EventuallyEvent) -> Result<Self, FeedParseError> {
        let (&game_id, ) = event.game_tags.iter().collect_tuple()
            .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "game" })?;

        // Order is very important here. Apparently game end events have extra teams?
        let (&away_team, &home_team, &home_team2, &away_team2) = event.team_tags.iter().collect_tuple()
            .ok_or_else(|| FeedParseError::MissingTags { event_type: event.r#type, tag_type: "team" })?;

        assert_eq!(away_team, away_team2);
        assert_eq!(home_team, home_team2);

        Self::try_from_event_with_teams(event, game_id, away_team, home_team)
    }

    fn try_from_event_with_teams(event: &EventuallyEvent, game_id: Uuid, away_team: Uuid, home_team: Uuid) -> Result<Self, FeedParseError> {
        Ok(Self {
            game_id,
            home_team,
            away_team,
            play: event.metadata.play
                .ok_or_else(|| {
                    FeedParseError::MissingMetadata {
                        event_type: event.r#type,
                        field: "play",
                    }
                })?,
        })
    }
}

impl Into<EventBuilderGame> for &GameEvent {
    fn into(self) -> EventBuilderGame {
        EventBuilderGame {
            game_id: self.game_id,
            home_team_id: self.home_team,
            away_team_id: self.away_team,
            play: self.play,
        }
    }
}

// This contains only the event properties that will differ from the parent, including id, created,
// and nuts; but not properties that will be the same, like day, season, and tournament.
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
    pub fn from_event(event: &EventuallyEvent) -> Self {
        Self {
            id: event.id,
            created: event.created,
            nuts: event.nuts,
        }
    }
}

impl Into<EventBuilderChildCommon> for &SubEvent {
    fn into(self) -> EventBuilderChildCommon {
        EventBuilderChildCommon {
            id: self.id,
            created: self.created,
            nuts: self.nuts,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FreeRefill {
    /// Metadata for the sub-event associated with losing the Free Refill mod
    pub sub_event: SubEvent,

    /// Subplay for the sub-event associated with losing the Free Refill mod
    pub sub_play: i64,

    /// Name of the player who used their Free Refill. This may be the batter, a scoring runner, or
    /// in rare cases, the pitcher.
    pub player_name: String,

    /// Uuid of the player who used their Free Refill
    pub player_id: Uuid,

    /// Uuid of the team of the player who used their Free Refill
    pub team_id: Uuid,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ScoringPlayer {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ScoreInfo {
    /// List of players who scored a Run
    pub scoring_players: Vec<ScoringPlayer>,

    /// List of players who used a Free Refill
    pub free_refills: Vec<FreeRefill>,
}

impl ScoreInfo {
    pub fn to_description(&self, score_text: &str) -> String {
        let mut output = String::new();
        for score in &self.scoring_players {
            write!(output, "\n{}{}", score.player_name, score_text).unwrap();
        }
        for refill in &self.free_refills {
            write!(output, "\n{} used their Free Refill.\n{} Refills the In!", refill.player_name, refill.player_name).unwrap();
        }

        output
    }

    pub fn scorer_ids(&self) -> Vec<Uuid> {
        self.scoring_players.iter()
            .map(|p| p.player_id)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Inhabiting {
    /// Metadata for the sub-event associated with adding the Inhabiting modifier
    pub sub_event: SubEvent,

    /// The name of the player who's being inhabited
    pub inhabited_player_name: String,

    /// The uuid of the player who's being inhabited
    pub inhabited_player_id: Uuid,

    /// The uuid of the player who's inhabiting
    pub inhabiting_player_id: Uuid,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StoppedInhabiting {
    /// Sub-event associated with losing the Inhabiting mod
    pub sub_event: SubEvent,

    /// Name of inhabiting player
    pub inhabiting_player_name: String,

    /// Uuid of inhabiting player
    pub inhabiting_player_id: Uuid,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub enum BlooddrainAction {
    AddBall,
    RemoveBall,
    AddStrike,
    RemoveStrike,
    AddOut,
    RemoveOut,
}

impl Display for BlooddrainAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlooddrainAction::AddBall => { write!(f, "adds a Ball") }
            BlooddrainAction::RemoveBall => { write!(f, "removes a Ball") }
            BlooddrainAction::AddStrike => { write!(f, "adds a Strike") }
            BlooddrainAction::RemoveStrike => { write!(f, "removes a Strike") }
            BlooddrainAction::AddOut => { write!(f, "adds a Out") }
            BlooddrainAction::RemoveOut => { write!(f, "removes a Out") }
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[repr(i32)]
pub enum ModDuration {
    // Permanent = 0,
    Seasonal = 1,
    // Weekly = 2,
    Game = 3,
}

impl Display for ModDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            // ModDuration::Permanent => { write!(f, "permanent") }
            ModDuration::Seasonal => { write!(f, "seasonal") }
            // ModDuration::Weekly => { write!(f, "weekly") }
            ModDuration::Game => { write!(f, "game") }
        }
    }
}

// Struct that bundles metadata necessary to reconstruct a ModAdded/ModChanged/ModRemoved event.
// Which of those it is will come from context. If the od of the player is not present in the
// containing event, use ModChangeSubEventWithPlayer or ModChangeSubEventWithNamedPlayer instead.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ModChangeSubEvent {
    /// Metadata for the sub-event associated with the mod change
    pub sub_event: SubEvent,

    /// Uuid of the team whose player's mod changed
    pub team_id: Uuid,
}

// Struct that bundles metadata necessary to reconstruct a ModAdded/ModChanged/ModRemoved event.
// Which of those it is will come from context. If the name of the player is not present in the
// containing event, use ModChangeSubEventWithNamedPlayer instead.
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
pub enum ActivePositionType {
    Lineup = 0,
    Rotation = 1,
}

impl Display for ActivePositionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivePositionType::Lineup => { write!(f, "batting") }
            ActivePositionType::Rotation => { write!(f, "pitching") }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, TryFromPrimitive, IntoPrimitive)]
#[repr(i64)]
pub enum ShadowPositionType {
    Bench = 2,
    Bullpen = 3,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FeedbackPlayerData {
    pub team_id: Uuid,
    pub team_nickname: String,
    pub player_id: Uuid,
    pub player_name: String,
    pub location: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub enum ReverbType {
    Rotation(SubEvent),
    Lineup(SubEvent),
    Full(SubEvent),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub enum BatterSkippedReason {
    /// Batter is Shelled
    Shelled,

    /// Batter is Elsewhere
    ///
    /// For whatever reason, this has a player_id while the Shelled variant does not
    Elsewhere(Uuid),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PlayerInfo {
    /// Player uuid
    pub player_id: Uuid,

    /// Player name
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Scattered {
    /// Name of player after being Scattered
    pub scattered_name: String,

    /// Sub-event associated with adding the Scattered mod
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum FedEventData {
    /// When a being (a god, Binky, or a similar entity) speaks
    BeingSpeech {
        /// Which being is speaking
        being: Being,
        /// The text of the being's message
        message: String,
    },

    /// This is always the first event of every game
    LetsGo {
        game: GameEvent,

        /// Weather for this game
        weather: Weather,

        /// Uuid of the stadium this game is being played in, if any
        stadium_id: Option<Uuid>,
    },

    /// This is always the second of event of every game
    PlayBall {
        game: GameEvent,
    },

    /// Marks the start of a half-inning
    HalfInningStart {
        game: GameEvent,

        /// Whether this is the top of the inning (true) or bottom of the inning (false)
        top_of_inning: bool,

        /// Zero-indexed inning number
        inning: i32,

        /// Full name of the team at bat
        batting_team_name: String,
    },

    /// Marks a new batter stepping up to the plate
    BatterUp {
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
    SuperyummyGameStart {
        game: GameEvent,

        /// Uuid of the Superyummy player
        player_id: Uuid,

        /// Uuid of the Superyummy player's team
        team_id: Uuid,

        /// Name of the Superyummy player
        player_name: String,

        /// Whether peanuts are present. Determines whether the player "loves" (true) or "misses"
        /// (false) peanuts.
        peanuts_present: bool,

        /// Whether this is the first time superyummy has procced. This is necessary for accurate
        /// reconstruction of the game event.
        is_first_proc: bool,

        /// Metadata for the event that adds or replaces the Overperforming or Underperforming mod
        sub_event: SubEvent,
    },

    /// Ball
    Ball {
        game: GameEvent,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,
    },

    /// Foul Ball
    FoulBall {
        game: GameEvent,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,
    },

    /// Strike, swinging
    StrikeSwinging {
        game: GameEvent,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,
    },

    /// Strike, looking
    StrikeLooking {
        game: GameEvent,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count
        strikes: i32,
    },

    /// Strike, flinching
    StrikeFlinching {
        game: GameEvent,

        /// Number of balls in the count
        balls: i32,

        /// Number of strikes in the count. Should always be 0, but still present in the data for
        /// forward-compatibility and convenience.
        strikes: i32,
    },

    /// Flyout
    Flyout {
        game: GameEvent,

        /// Name of the batter that hit the flyout
        batter_name: String,

        /// Name of the batter that caught the out
        fielder_name: String,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,
    },

    /// A simple ground out. This includes sacrifices but does not include fielder's choices or
    /// double plays.
    GroundOut {
        game: GameEvent,

        /// Name of player who hit the ground out
        batter_name: String,

        /// Name of fielder who caught the ground out
        fielder_name: String,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
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

    /// Fielders choice event
    FieldersChoice {
        game: GameEvent,

        /// Name of batter who hit into the fielder's choice
        batter_name: String,

        /// Name of the runner who got out as a result of the fielder's choice
        runner_out_name: String,

        /// Which base the runner was tagged out on. First base is `1`, second is `2`, etc.
        out_at_base: i32,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter was Red Hot and cooled off, contains metadata about them losing the Red
        /// Hot mod, otherwise null.
        cooled_off: Option<ModChangeSubEventWithPlayer>,
    },

    /// Double play event
    DoublePlay {
        game: GameEvent,

        /// Name of batter who hit into the double play
        batter_name: String,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    /// Hit event (Single, Double, Triple, or Quadruple)
    Hit {
        game: GameEvent,

        /// Name of hte player who hit the ball
        batter_name: String,

        /// Uuid of the player who hit the ball
        batter_id: Uuid,

        /// Number of bases the batter got. Single is `1`, double is `2`, etc.
        num_bases: i32,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// The Spicy status of the batter
        spicy_status: SpicyStatus,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players scored with Tired or Wired the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,
    },

    /// Home run, including Grand Slam
    HomeRun {
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
    },

    /// Stolen base
    StolenBase {
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
    },

    /// Caught stealing
    CaughtStealing {
        game: GameEvent,

        /// Name of the runner who tried to steal the base
        runner_name: String,

        /// Which base they tried to steal
        base_stolen: i32,
    },

    /// Strikeout swinging
    StrikeoutSwinging {
        game: GameEvent,

        /// Name of batter who struck out swinging
        batter_name: String,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players got Unrun strikeouts the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,
    },

    /// Strikeout looking
    StrikeoutLooking {
        game: GameEvent,

        /// Name of batter who struck out looking
        batter_name: String,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the event was a Special type. Usually this can be inferred from other fields.
        /// However, the early Expansion Era, when players got Unrun strikeouts the event was
        /// Special but that was the only way of knowing. (It's possible that there are other
        /// circumstances that cause an otherwise-undetectable Special event.)
        is_special: bool,
    },

    /// Player drew a walk
    Walk {
        game: GameEvent,

        /// Name of the batter who drew the walk
        batter_name: String,

        /// Uuid of the batter who drew the walk
        batter_id: Uuid,

        #[serde(flatten)]
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,

        /// If the batter went to a later base with Base Instincts, this is the base number.
        /// Otherwise null.
        base_instincts: Option<i32>,
    },

    /// Marks the end of the half-inning
    InningEnd {
        game: GameEvent,

        /// Which inning just ended (one-indexed)
        inning_num: i32,

        /// List of pitchers who lost Triple Threat. Should be at most two players.
        lost_triple_threat: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Player struck out by charming the batter
    CharmStrikeout {
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
    StrikeZapped {
        game: GameEvent,
    },

    /// Peanut flavor text messages
    PeanutFlavorText {
        game: GameEvent,

        /// The text of the message
        message: String,
    },

    GameEnd {
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
    },

    /// Mild pitch that does not result in a walk
    MildPitch {
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
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    /// Mild pitch that results in a walk
    MildPitchWalk {
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
        scores: ScoreInfo,

        /// If the batter was Inhabiting, contains metadata about the player losing the Inhabiting
        /// mod, otherwise null.
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    /// Player is Beaned with a Tired or Wired
    CoffeeBean {
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

        /// Whether the player already has the mod (if it is, the mod will be removed)
        has_mod: bool,

        /// Metadata of the sub-event associated with adding or removing the Tired/Wired mod
        sub_event: SubEvent,

        /// Uuid for the team whose player was Beaned
        team_id: Uuid,

        /// The mod this player previously had, if any. This isn't visible in the text of the event
        /// but it is in the metadata.
        previous: Option<CoffeeBeanMod>,
    },

    /// Player became magmatic
    BecameMagmatic {
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
    Blooddrain {
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
    SpecialBlooddrain {
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
    BirdsCircle {
        game: GameEvent,
    },

    /// Batter is ambushed by crows, leading to an out. This can happen randomly or as a result of
    /// the Friend of Crows mod
    AmbushedByCrows {
        game: GameEvent,

        /// Uuid of batter who was ambushed
        batter_id: Uuid,

        /// Name of batter who was ambushed
        batter_name: String,

        /// If this is a Friends of Crows proc, the uuid and name of the pitcher who called upon
        /// their friends
        pitcher: Option<PlayerInfo>,
    },

    /// Sun2 set a Win. This version of the event shows up in the Outcomes section and is separate
    /// from the version that shows up in the game log.
    Sun2SetWin {
        /// Uuid of team who earned the Win
        team_id: Uuid,

        /// Nickname of team who earned the Win
        team_nickname: String,
    },

    /// Black hole swallowed a win. This version of the event shows up in the Outcomes section and
    /// is separate from the version that shows up in the game log.
    BlackHoleSwallowedWin {
        /// Uuid of team whose Win was swallowed
        team_id: Uuid,

        /// Nickname of team whose Win was swallowed
        team_nickname: String,
    },

    /// Sun2 set a Win. This version of the event shows up in the game log and is separate from the
    /// version that shows up in the Outcomes section.
    Sun2 {
        game: GameEvent,

        /// Nickname of team who earned the Win
        team_nickname: String,
    },

    /// Black hole swallowed a win. This version of the event shows up in the game log and is
    /// separate from the version that shows up in the Outcomes section.
    BlackHole {
        game: GameEvent,

        /// Nickname of the team that caused the event
        scoring_team_nickname: String,

        /// Nickname of the team whose Win was swallowed
        victim_team_nickname: String,
    },

    /// Team shamed another team
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
    CharmWalk {
        game: GameEvent,

        /// Uuid of the batter that did the charming
        batter_id: Uuid,

        /// Name of the batter that did the charming
        batter_name: String,

        /// Name of the pitcher that was charmed
        pitcher_name: String,
    },

    /// Player gained a Free Refill
    GainFreeRefill {
        game: GameEvent,

        /// Uuid of the team of the player who gained the Free Refill
        team_id: Uuid,

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
    AllergicReaction {
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
    PerkUp {
        game: GameEvent,

        /// Players who gained Overperforming as a result of Perk
        players: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Feedback
    Feedback {
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
    BestowReverberating {
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
    Reverb {
        game: GameEvent,

        /// Uuid of team who got reverbed
        team_id: Uuid,

        /// Nickname of team who got reverbed
        team_nickname: String,

        /// Type of reverb that happened, with metadata for the associated `ReverbRosterShuffle`
        /// sub-event
        reverb_type: ReverbType,
    },

    /// Tarot readings
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

    /// Added "Over Under" mod. This happened as a result of the tarot reading.
    AddedOverUnder {
        /// Uuid of team of player who gained Over Under
        team_id: Uuid,

        /// Uuid player who gained Over Under
        player_id: Uuid,

        /// Name of player who gained Over Under
        player_name: String,
    },

    /// Added "Under Over" mod. This happened as a result of the tarot reading.
    AddedUnderOver {
        /// Uuid of team of player who gained Under Over
        team_id: Uuid,

        /// Uuid player who gained Under Over
        player_id: Uuid,

        /// Name of player who gained Under Over
        player_name: String,
    },

    /// Team entered Party Time!
    TeamEnteredPartyTime {
        /// Uuid of team who just entered Party Time
        team_id: Uuid,

        /// Nickname of team who just entered Party Time
        team_nickname: String,
    },

    /// Player becomes Triple Threat at start of game
    BecomeTripleThreat {
        game: GameEvent,

        /// Add mod events for the players who became Triple Threat
        pitchers: (ModChangeSubEventWithNamedPlayer, ModChangeSubEventWithNamedPlayer),
    },

    /// Under Over procced
    UnderOver {
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
    OverUnder {
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
    TasteTheInfinite {
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
    BatterSkipped {
        game: GameEvent,

        /// Name of batter who got skipped
        batter_name: String,

        /// Reason the batter was skipped
        reason: BatterSkippedReason,
    },

    /// Feedback failed and initiator was tangled in the feedback
    FeedbackBlocked {
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
    FlagPlanted {
        /// Uuid of team who broke ground
        team_id: Uuid,

        /// Nickname of team who broke ground
        team_nickname: String,

        /// Name of newly created ballpark
        ballpark_name: String,

        /// Name of prefab used for newly created ballpark
        prefab_name: String,

        /// Number of votes team spent on the ballpark
        votes: i64,
    },

    /// Emergency Alerty
    EmergencyAlert {
        /// Message of emergency alert
        message: String,

        /// Teams involved in emergency alert
        team_tags: Vec<Uuid>,
    },

    /// Team was added to ILB
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
    FloodingSwept {
        game: GameEvent,

        /// List of players who were swept Elsewhere
        swept_elsewhere: Vec<ModChangeSubEventWithNamedPlayer>,
    },

    /// Player returned from Elsewhere
    ReturnFromElsewhere {
        game: GameEvent,

        /// Team uuid of player who returned from Elsewhere
        team_id: Uuid,

        /// Uuid of player who returned from Elsewhere
        player_id: Uuid,

        /// Name of player who returned from Elsewhere
        player_name: String,

        /// Metadata for sub-event associated with removing the Elsewhere mod
        sub_event: SubEvent,

        /// Number of days the player was Elsewhere, or null if the player was elsewhere for one
        /// season. No player has ever returned after more than one season
        number_of_days: Option<i32>,

        /// Scattered sub-event, if the player was scattered, or null otherwise
        scattered: Option<Scattered>,
    },

    /// Player was incinerated
    Incineration {
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

        /// Metadata for the incineration sub-event, the enters-hall sub-event, the hatch sub-event,
        /// and the replacement sub-event, in that order
        sub_events: (SubEvent, SubEvent, SubEvent, SubEvent),
    },

    /// Pitcher change event. This happens automatically when something incapacitates the active
    /// pitcher (e.g. the player is shelled by Taste the Infinite)
    PitcherChange {
        game: GameEvent,

        /// Nickname of team whose pitcher changed
        team_nickname: String,

        /// Uuid of new pitcher
        pitcher_id: Uuid,

        /// Name of new pitcher
        pitcher_name: String,
    },

    /// Team partied
    Party {
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
    PlayerHatched {
        /// Uuid of newly hatched player
        player_id: Uuid,

        /// Name of newly hatched player
        player_name: String,
    },

    /// Team received a postseason birth. I believe this is always preceded by a PlayerHatched event
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
    TeamLeftPartyTimeForPostseason {
        /// Uuid of team who left Party Time
        team_id: Uuid,

        /// Name of team who left Party Time
        team_name: String,
    },

    /// Team earned a slot in the postseason
    EarnedPostseasonSlot {
        /// Uuid of team who earned a slot in the postseason
        team_id: Uuid,

        /// Nickname of team who earned a slot in the postseason
        team_nickname: String,
    },

    /// Team advanced to next round of the postseason
    PostseasonAdvance {
        /// Uuid of team who advanced in the postseason
        team_id: Uuid,

        /// Nickname of team who advanced in the postseason
        team_nickname: String,

        /// Round to which the team advanced, or null for the Internet Series
        round: Option<i32>,

        /// One-indexed season number
        season: i32,
    },

    /// Team was eliminated from the postseason
    PostseasonEliminated {
        /// Uuid of team who was eliminated from the postseason
        team_id: Uuid,

        /// Nickname of team who was eliminated from the postseason
        team_nickname: String,

        /// One-indexed season number
        season: i32,
    },

    /// Player was boosted during election
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
    TeamWonInternetSeries {
        /// Uuid of team who won the series
        team_id: Uuid,

        /// Name of team who won the series
        team_nickname: String,

        /// Number of championships the team now has
        championships: i64,
    },

    /// Bottom Dwellers team mod procs
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
    BlessingWon {
        /// Uuid of team who won the Blessing
        team_id: Uuid,

        /// Title of Blessing that was won. This may be redundant with the title in `metadata`
        blessing_title: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,
    },

    /// Earlbirds mod procs at the beginning of Earlseason
    Earlbirds {
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
    DecreePassed {
        /// Title of Decree that passesd. This may be redundant with the title in `metadata`
        decree_title: String,

        /// Event metadata exactly as it appears in the Feed event
        metadata: EventMetadata,
    },

    /// Player was added to ILB
    PlayerJoinedILB {
        /// Uuid of newly added player
        player_id: Uuid,

        /// Name of newly added player
        player_name: String,
    },

    /// A Returned player was permitted to stay (not called back to the Hall at the end of the
    /// season)
    PlayerPermittedToStay {
        /// Uuid of player who was permitted to stay
        player_id: Uuid,

        /// Name of player who was permitted to stay
        player_name: String,
    },

    /// Umpire tried to incinerate the player, but the player was Fireproof
    FireproofIncineration {
        game: GameEvent,

        /// Uuid of fireproof player
        player_id: Uuid,

        /// Name of fireproof player
        player_name: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, IntoPrimitive, TryFromPrimitive)]
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
#[derive(Debug, Builder, JsonSchema)]
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
    pub data: FedEventData,
}

trait GameEventForBuilder {
    fn for_game(self, game: &GameEvent) -> Self;
    fn for_sub_event(self, sub: &SubEvent) -> Self;
}

fn possessive(player_name: String) -> String {
    if player_name.chars().last().unwrap() == 's' {
        player_name + "'"
    } else {
        player_name + "'s"
    }
}

impl FedEvent {
    pub fn into_feed_event(self) -> EventuallyEvent {
        let event_builder = self.make_event_builder();

        match self.data {
            FedEventData::BeingSpeech { being, message } => {
                let being_id: i32 = being.into();
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::LetsGo,
                        description: "Let's Go!".to_string(),
                        ..Default::default()
                    })
                    .metadata(metadata)
                    .build()
            }
            FedEventData::PlayBall { game } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayBall,
                        description: "Play ball!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::HalfInning,
                        description: format!("{} of {inning}, {batting_team_name} batting.",
                                             if top_of_inning { "Top" } else { "Bottom" }),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::BatterUp { ref game, ref batter_name, ref team_name, wielding_item: ref wielding_item_name, ref inhabiting, is_repeating } => {
                let item_suffix = if let Some(item_name) = wielding_item_name {
                    format!(", wielding {}", item_name)
                } else {
                    String::default()
                };

                let prefix = if is_repeating {
                    format!("{batter_name} is Repeating!\n")
                } else {
                    String::default()
                };

                let children = inhabiting.iter()
                    .map(|inhabiting| {
                        EventBuilder::child(&inhabiting.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedMod,
                                category: EventCategory::Changes,
                                description: format!("{} is Inhabiting {}!",
                                                     batter_name, inhabiting.inhabited_player_name),
                                player_tags: vec![inhabiting.inhabiting_player_id],
                                team_tags: vec![], // need to clear it
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "INHABITING",
                                "type": 0, // ?
                            }))
                    });

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
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
                    .children(children)
                    .build()
            }
            FedEventData::SuperyummyGameStart { ref game, ref player_name, peanuts_present: peanuts, is_first_proc, ref sub_event, player_id, team_id } => {
                let description = format!("{} {} Peanuts.", player_name,
                                          if peanuts { "loves" } else { "misses" });
                let mod_name = if peanuts { "OVERPERFORMING" } else { "UNDERPERFORMING" };
                let opposite_mod_name = if peanuts { "UNDERPERFORMING" } else { "OVERPERFORMING" };
                let change_event = if is_first_proc {
                    EventBuilder::child(sub_event)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::AddedModFromOtherMod,
                            description: description.clone(),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                        })
                        .metadata(json!({
                            "mod": mod_name,
                            "source": "SUPERYUMMY",
                            "type": 0, // ?
                        }))
                } else {
                    EventBuilder::child(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::ChangedModFromOtherMod,
                            category: EventCategory::Changes,
                            description: description.clone(),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                        })
                        .metadata(json!({
                            "from": opposite_mod_name,
                            "source": "SUPERYUMMY",
                            "to": mod_name,
                            "type": 0, // ?
                        }))
                };
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Superyummy,
                        description,
                        ..Default::default()
                    })
                    .child(change_event)
                    .build()
            }
            FedEventData::Ball { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Ball,
                        description: format!("Ball. {}-{}", balls, strikes),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::StrikeSwinging { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strike,
                        description: format!("Strike, swinging. {balls}-{strikes}"),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::StrikeLooking { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strike,
                        description: format!("Strike, looking. {balls}-{strikes}"),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::StrikeFlinching { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strike,
                        description: format!("Strike, flinching. {balls}-{strikes}"),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FoulBall { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::FoulBall,
                        description: format!("Foul Ball. {balls}-{strikes}"),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Flyout { ref game, ref batter_name, ref fielder_name, ref scores, ref stopped_inhabiting, ref cooled_off } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " tags up and scores!");
                let mut player_tags = scores.scorer_ids();

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                let suffix = self.push_cooled_off(&game, batter_name, cooled_off, &mut children, &mut player_tags);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::FlyOut,
                        category: EventCategory::special_if(has_any_refills || cooled_off.is_some()),
                        description: format!("{batter_name} hit a flyout to {fielder_name}.{score_text}{suffix}"),
                        player_tags,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Hit { ref game, ref batter_name, batter_id, num_bases, ref scores, ref stopped_inhabiting, ref spicy_status, is_special } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                self.push_red_hot(&game, batter_name, batter_id, spicy_status, &mut children);

                let spicy_text = match spicy_status {
                    SpicyStatus::None => String::new(),
                    SpicyStatus::HeatingUp => format!("\n{} is Heating Up!", batter_name),
                    SpicyStatus::RedHot { .. } => format!("\n{} is Red Hot!", batter_name),
                };
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Hit,
                        category: EventCategory::special_if(has_any_refills || spicy_status.is_special() || is_special),
                        description: format!("{batter_name} hits a {}!{score_text}{spicy_text}", match num_bases {
                            1 => "Single",
                            2 => "Double",
                            3 => "Triple",
                            4 => "Quadruple",
                            // TODO Turn this into a Result error
                            _ => panic!("Unknown hit type")
                        }),
                        player_tags: iter::once(batter_id)
                            .chain(scores.scorer_ids())
                            .chain(if spicy_status.is_none() { None } else { Some(batter_id) }.into_iter())
                            .collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::HomeRun { ref game, ref magmatic, ref batter_name, batter_id, num_runs, ref free_refills, ref spicy_status, ref stopped_inhabiting, is_special } => {
                let suffix = free_refills.iter()
                    .map(|free_refill| {
                        format!("\n{} used their Free Refill.\n{} Refills the In!",
                                free_refill.player_name, free_refill.player_name)
                    })
                    .join("");

                let suffix = match spicy_status {
                    SpicyStatus::None => suffix,
                    SpicyStatus::HeatingUp => format!("{suffix}\n{batter_name} is Heating Up!"),
                    SpicyStatus::RedHot { .. } => format!("{suffix}\n{batter_name} is Red Hot!"),
                };

                let mut children = if let Some(ModChangeSubEvent { sub_event, team_id }) = magmatic {
                    vec![EventBuilder::child(sub_event)
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
                        }))
                    ]
                } else {
                    Vec::new()
                };

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                self.push_red_hot(&game, batter_name, batter_id, spicy_status, &mut children);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
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
                        player_tags: if spicy_status.is_none() { vec![batter_id] } else { vec![batter_id, batter_id] },
                        ..Default::default()
                    })
                    .children(
                        free_refills.iter()
                            .map(|free_refill| self.make_free_refill_child(&game, free_refill))
                            .chain(children.into_iter())
                    )
                    .build()
            }
            FedEventData::GroundOut { ref game, ref batter_name, ref fielder_name, ref scores, ref stopped_inhabiting, ref cooled_off, is_special } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " advances on the sacrifice.");
                let mut player_tags = scores.scorer_ids();

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                let suffix = self.push_cooled_off(&game, batter_name, cooled_off, &mut children, &mut player_tags);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::special_if(has_any_refills || cooled_off.is_some() || is_special),
                        description: format!("{} hit a ground out to {}.{}{}",
                                             batter_name, fielder_name, score_text, suffix),
                        player_tags,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::StolenBase { ref game, ref runner_name, runner_id, base_stolen, blaserunning, ref free_refill } => {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::StolenBase,
                        category: EventCategory::special_if(blaserunning || free_refill.is_some()),
                        description: format!("{} steals {} base!{}{}", runner_name, base_name(base_stolen), blaserunning_str, free_refill_str),
                        player_tags: if blaserunning { vec![runner_id, runner_id] } else { vec![runner_id] },
                        ..Default::default()
                    })
                    .children(
                        free_refill.as_ref()
                            .map(|free_refill| self.make_free_refill_child(&game, free_refill))
                            .into_iter()
                    )
                    .build()
            }
            FedEventData::StrikeoutSwinging { ref game, ref batter_name, ref stopped_inhabiting, is_special } => {
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strikeout,
                        category: EventCategory::special_if(is_special),
                        description: format!("{} strikes out swinging.", batter_name),
                        ..Default::default()
                    })
                    .children(self.stopped_inhabiting_children(&game, &stopped_inhabiting))
                    .build()
            }
            FedEventData::StrikeoutLooking { ref game, ref batter_name, ref stopped_inhabiting, is_special } => {
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strikeout,
                        category: EventCategory::special_if(is_special),
                        description: format!("{} strikes out looking.", batter_name),
                        ..Default::default()
                    })
                    .children(self.stopped_inhabiting_children(&game, &stopped_inhabiting))
                    .build()
            }
            FedEventData::Walk { ref game, ref batter_name, batter_id, ref scores, ref stopped_inhabiting, ref base_instincts } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                let base_instincts_str = if let Some(base) = base_instincts {
                    format!("\nBase Instincts take them directly to {} base!", base_name(*base))
                } else {
                    String::new()
                };

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Walk,
                        category: EventCategory::special_if(has_any_refills || base_instincts.is_some()),
                        description: format!("{} draws a walk.{}{}", batter_name, base_instincts_str, score_text),
                        player_tags: iter::once(batter_id).chain(scores.scorer_ids()).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::StolenBase,
                        description: format!("{} gets caught stealing {} base.", runner_name, base_name(base_stolen)),
                        player_tags: vec![],
                        team_tags: vec![],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::InningEnd { ref game, inning_num, ref lost_triple_threat } => {
                let (children, suffix) = self.make_mod_change_sub_events(
                    game, lost_triple_threat, EventType::RemovedMod, "is no longer a Triple Threat.", "TRIPLE_THREAT");

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::Strikeout,
                        category: EventCategory::Special,
                        description: format!("{charmer_name} charmed {charmed_name}!\n{charmed_name} swings {num_swings} times to strike out willingly!"),
                        // I do not know why the charmer appears twice, but that seems to be accurate
                        player_tags: vec![charmer_id, charmer_id, charmed_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FieldersChoice { ref game, ref batter_name, ref runner_out_name, out_at_base, ref scores, ref stopped_inhabiting, ref cooled_off } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");
                let mut player_tags = scores.scorer_ids();

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                let suffix = self.push_cooled_off(&game, batter_name, cooled_off, &mut children, &mut player_tags);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::special_if(has_any_refills || cooled_off.is_some()),
                        description: format!("{runner_out_name} out at {} base.{score_text}\n{batter_name} reaches on fielder's choice.{suffix}",
                                             base_name(out_at_base)),
                        player_tags,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::StrikeZapped { game } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::StrikeZapped,
                        category: EventCategory::Special,
                        description: "The Electricity zaps a strike away!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PeanutFlavorText { game, message } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PeanutFlavorText,
                        category: EventCategory::Special,
                        description: message,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::DoublePlay { ref game, ref batter_name, ref scores, ref stopped_inhabiting } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::special_if(has_any_refills),
                        description: format!("{} hit into a double play!{}", batter_name, score_text),
                        player_tags: scores.scorer_ids(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::GameEnd { game, winner_id, winning_team_name, winning_team_score, losing_team_name, losing_team_score } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::GroundOut,
                        category: EventCategory::Outcomes,
                        description: format!("{winning_team_name} {winning_team_score}, {losing_team_name} {losing_team_score}"),
                        team_tags: vec![
                            // For some reason the teams are repeated like this? idk why
                            game.away_team, game.home_team, game.home_team, game.away_team,
                        ],
                        ..Default::default()
                    })
                    .metadata(json!({ "winner": winner_id }))
                    .build()
            }
            FedEventData::MildPitch { ref game, pitcher_id, ref pitcher_name, balls, strikes, runners_advance, ref scores, ref stopped_inhabiting } => {
                let (score_text, _, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                let runners_advance_str = if runners_advance {
                    "\nRunners advance on the pathetic play!"
                } else {
                    ""
                };

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\nBall, {balls}-{strikes}.{runners_advance_str}{score_text}"),
                        player_tags: iter::once(pitcher_id).chain(scores.scorer_ids()).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::CoffeeBean { ref game, player_id, ref player_name, ref roast, ref notes, ref which_mod, has_mod, ref sub_event, team_id, ref previous } => {
                let change_str = if has_mod { "is" } else { "is no longer" };
                let mod_str = match which_mod {
                    CoffeeBeanMod::Wired => { "Wired!" }
                    CoffeeBeanMod::Tired => { "Tired." }
                };
                let mod_id = which_mod.to_str();
                let child = EventBuilder::child(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: if previous.is_some() { EventType::ModChange } else { EventType::AddedMod },
                        category: EventCategory::Changes,
                        description: format!("{player_name} {change_str} {mod_str}"),
                        team_tags: vec![team_id],
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::CoffeeBean,
                        category: EventCategory::Special,
                        description: format!("{player_name} is Beaned by a {roast} roast with {notes}.\n{player_name} {change_str} {mod_str}"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BecameMagmatic { ref game, player_id, ref player_name, team_id, ref mod_add_event } => {
                let child = EventBuilder::child(mod_add_event)
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
                    .update(EventBuilderUpdate {
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
                let child = EventBuilder::child(sipped_event)
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::BlooddrainSiphon,
                        category: EventCategory::Special,
                        description: format!("The Blooddrain gurgled!\n{sipper_name}'s Siphon activates!\n{sipper_name} siphoned some of {sipped_name}'s {sipped_category} ability!\n{sipper_name} {action}!"),
                        player_tags: vec![sipper_id, sipped_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerModExpires { team_id, player_id, player_name, mods, mod_duration } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::ModExpires,
                        category: EventCategory::Special,
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
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::BirdsCircle,
                        category: EventCategory::Special,
                        description: "The Birds circle ... but they don't find what they're looking for.".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::AmbushedByCrows { ref game, batter_id, ref batter_name, ref pitcher } => {
                let prefix = if let Some(PlayerInfo { player_name, .. }) = pitcher {
                    format!("{player_name} calls upon their Friends!\n")
                } else {
                    String::new()
                };
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AmbushedByCrows,
                        category: EventCategory::Special,
                        description: format!("{prefix}A murder of Crows ambush {batter_name}!\nThey run to safety, resulting in an out."),
                        player_tags: if let Some(PlayerInfo { player_id, .. }) = pitcher { vec![*player_id, batter_id] } else { vec![batter_id] },
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2SetWin { team_id, team_nickname } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::BlackHoleSwallowedWin,
                        category: EventCategory::Outcomes,
                        description: format!("The Black Hole swallowed a Win from the {team_nickname}!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2 { game, team_nickname } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Sun2,
                        category: EventCategory::Special,
                        description: format!("The {team_nickname} collect 10! Sun 2 smiles.\nSun 2 set a Win upon the {team_nickname}."),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::BlackHole { game, scoring_team_nickname, victim_team_nickname } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::BlackHole,
                        category: EventCategory::Special,
                        description: format!("The {scoring_team_nickname} collect 10!\nThe Black Hole swallows the Runs and a {victim_team_nickname} Win."),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::TeamDidShame { shaming_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
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
            FedEventData::CharmWalk { game, batter_name, batter_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Walk,
                        category: EventCategory::Special,
                        description: format!("{batter_name} charms {pitcher_name}!\n{batter_name} walks to first base."),
                        player_tags: vec![batter_id, batter_id], // two of them
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::GainFreeRefill { ref game, player_id, ref player_name, ref roast, ref ingredient1, ref ingredient2, ref sub_event, team_id } => {
                let child = EventBuilder::child(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} got a Free Refill."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "COFFEE_RALLY",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
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
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::AllergicReaction,
                        category: EventCategory::Special,
                        description: format!("{player_name} swallowed a stray peanut and had an allergic reaction!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::MildPitchWalk { ref game, pitcher_id, ref pitcher_name, batter_id, ref batter_name, ref scores, ref stopped_inhabiting } => {
                let (score_text, _, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\n{batter_name} draws a walk.{score_text}"),
                        player_tags: [pitcher_id, batter_id].into_iter().chain(scores.scorer_ids()).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::PerkUp { ref game, ref players } => {
                let children = players.iter()
                    .enumerate()
                    .map(|(sub_play, player)| {
                        EventBuilder::child(&player.sub_event)
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
                    .update(EventBuilderUpdate {
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
                    (0, sipped, EventType::PlayerStatDecrease, format!("{} had blood drained by {}.", sipped.player_name, sipper.player_name)),
                    (1, sipper, EventType::PlayerStatIncrease, format!("{} drained blood from {}.", sipper.player_name, sipped.player_name)),
                ].into_iter().map(|(sub_play, change, event_type, description)| {
                    EventBuilder::child(&change.sub_event)
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
                    .update(EventBuilderUpdate {
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
                let child = EventBuilder::child(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerTraded,
                        category: EventCategory::Changes,
                        description: "Reality flickered in the Feedback.".to_string(),
                        team_tags: vec![player_a.team_id, player_b.team_id],
                        player_tags: vec![player_a.player_id, player_b.player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "aLocation": player_a.location,
                        "aPlayerId": player_a.player_id,
                        "aPlayerName": player_a.player_name,
                        "aTeamId": player_a.team_id,
                        "aTeamName": player_a.team_nickname,
                        "bLocation": player_b.location,
                        "bPlayerId": player_b.player_id,
                        "bPlayerName": player_b.player_name,
                        "bTeamId": player_b.team_id,
                        "bTeamName": player_b.team_nickname,
                    }));

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::FeedbackSwap,
                        category: EventCategory::Special,
                        description: format!("Reality flickers. Things look different ...\n{} and {} switch teams in the feedback!\n{} is now {position_type}.", player_a.player_name, player_b.player_name, player_b.player_name),
                        player_tags: vec![player_a.player_id, player_b.player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BestowReverberating { ref game, team_id, player_id, ref player_name, ref sub_event } => {
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::ReverbBestowsReverberating,
                        category: EventCategory::Special,
                        description: format!("Reverberations are at dangerous levels!\n{player_name} is now Reverberating wildly!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Reverb { ref game, team_id, ref team_nickname, ref reverb_type } => {
                let get_child = |sub_event, event_type, shuffle_location| {
                    EventBuilder::child(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: event_type,
                            category: EventCategory::Changes,
                            description: format!("The {team_nickname} had their {shuffle_location} shuffled in the Reverb!"),
                            team_tags: vec![team_id],
                            ..Default::default()
                        })
                        .metadata(json!({ "parent": self.id }))
                };

                match reverb_type {
                    ReverbType::Lineup(sub_event) => {
                        event_builder.for_game(game)
                            .update(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations hit unsafe levels!\nThe {team_nickname} had their lineup shuffled in the Reverb!"),
                                ..Default::default()
                            })
                            .child(get_child(sub_event, EventType::ReverbLineupShuffle, "lineup"))
                            .build()
                    }
                    ReverbType::Rotation(sub_event) => {
                        event_builder.for_game(game)
                            .update(EventBuilderUpdate {
                                r#type: EventType::ReverbRosterShuffle,
                                category: EventCategory::Special,
                                description: format!("Reverberations are at unsafe levels!\nThe {team_nickname} had their rotation shuffled in the Reverb!"),
                                ..Default::default()
                            })
                            .child(get_child(sub_event, EventType::ReverbRotationShuffle, "rotation"))
                            .build()
                    }
                    ReverbType::Full(_) => {
                        todo!()
                    }
                }
            }
            FedEventData::TarotReading { description, metadata, player_tags, team_tags } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::TarotReading,
                        category: EventCategory::Changes,
                        description,
                        ..Default::default()
                    })
                    .metadata(metadata)
                    .build()
            }
            FedEventData::AddedUnderOver { team_id, player_id, player_name } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("UNDER OVER, {player_name}"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "UNDEROVER",
                        "type": 0, // player
                    }))
                    .build()
            }
            FedEventData::AddedOverUnder { team_id, player_id, player_name } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("OVER UNDER, {player_name}"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERUNDER",
                        "type": 0, // player
                    }))
                    .build()
            }
            FedEventData::BecomeTripleThreat { ref game, pitchers: (ref pitcher_1, ref pitcher_2) } => {
                let children = [pitcher_1, pitcher_2].iter()
                    .enumerate()
                    .map(|(sub_play, pitcher)| {
                        EventBuilder::child(&pitcher.sub_event)
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
                    })
                    .collect::<Vec<_>>(); // Collect needed because of borrowing rules
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::BecomeTripleThreat,
                        category: EventCategory::Special,
                        description: format!("{} and {} chug a Third Wave of Coffee!\nThey are now Triple Threats!", pitcher_1.player_name, pitcher_2.player_name),
                        player_tags: vec![pitcher_1.player_id, pitcher_2.player_id],
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::UnderOver { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Under Over, {}.", if on { "On" } else { "Off" });
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::UnderOver,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::OverUnder { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Over Under, {}.", if on { "On" } else { "Off" });
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::OverUnder,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::TasteTheInfinite { ref game, sheller_id, ref sheller_name, shellee_team_id, shellee_id, ref shellee_name, ref sub_event } => {
                let child = EventBuilder::child(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: format!("{shellee_name} is Shelled!"),
                        team_tags: vec![shellee_team_id],
                        // Yes this makes no sense! but, it appears to be that way
                        player_tags: vec![sheller_id],
                        ..Default::default()
                    });

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::BatterSkipped,
                        category: EventCategory::Special,
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
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::FeedbackBlocked,
                        category: EventCategory::Special,
                        description: format!("Reality begins to flicker ...\nBut {resisted_name} resists!\n{tangled_name} is tangled in the flicker!"),
                        player_tags: vec![resisted_id, tangled_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::FlagPlanted { team_id, team_nickname, ballpark_name, prefab_name, votes } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::FlagPlanted,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} break ground on {ballpark_name}, selecting to build the {prefab_name} prefab!\nTHE FLAG IS PLANTED"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "renoId": "1",
                        "title": "Ground Broken",
                        "votes": votes,
                    }))
                    .build()
            }
            FedEventData::EmergencyAlert { message, team_tags } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
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
            FedEventData::FloodingSwept { ref game, ref swept_elsewhere } => {
                let (children, suffix) = self.make_mod_change_sub_events(
                    game, swept_elsewhere, EventType::AddedMod, "is swept Elsewhere!", "ELSEWHERE");

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::FloodingSwept,
                        category: EventCategory::Special,
                        description: format!("A surge of Immateria rushes up from Under!\nBaserunners are swept from play!{suffix}"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::ReturnFromElsewhere { ref game, team_id, player_id, ref player_name, ref sub_event, number_of_days, ref scattered } => {
                let description = if let Some(days) = number_of_days {
                    let s = if days == 1 { "" } else { "s" };
                    format!("{player_name} has returned from Elsewhere after {days} day{s}!")
                } else {
                    format!("{player_name} has returned from Elsewhere after one season!")
                };
                let elsewhere_child = EventBuilder::child(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::RemovedMod,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "ELSEWHERE",
                        "type": 0, // ?
                    }));

                let children = if let Some(Scattered { scattered_name, sub_event }) = scattered {
                    let scattered_child = EventBuilder::child(sub_event)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::AddedMod,
                            description: format!("{scattered_name} was Scattered..."),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
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
                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::ReturnFromElsewhere,
                        category: EventCategory::Special,
                        description,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Incineration { ref game, team_id, ref team_nickname, victim_id, ref victim_name, replacement_id, ref replacement_name, location, ref sub_events } => {
                let (incin_child, enter_hall_child, hatch_child, replace_child) = sub_events;
                let location_int: i64 = location.into();
                let children = vec![
                    EventBuilder::child(incin_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::Incineration,
                            description: format!("Rogue Umpire incinerated {victim_name}!"),
                            team_tags: vec![team_id],
                            player_tags: vec![victim_id],
                            ..Default::default()
                        })
                        .no_metadata(),
                    EventBuilder::child(enter_hall_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::EnterHallOfFlame,
                            description: format!("{victim_name} entered the Hall of Flame."),
                            player_tags: vec![victim_id],
                            ..Default::default()
                        })
                        .no_metadata(),
                    EventBuilder::child(hatch_child)
                        .update(EventBuilderUpdate {
                            category: EventCategory::Changes,
                            r#type: EventType::PlayerHatched,
                            description: format!("{replacement_name} has been hatched from the field of eggs."),
                            player_tags: vec![replacement_id],
                            ..Default::default()
                        })
                        .metadata(json!({ "id": replacement_id })),
                    EventBuilder::child(replace_child)
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

                event_builder.for_game(game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::Incineration,
                        category: EventCategory::Special,
                        description: format!("Rogue Umpire incinerated {victim_name}!\nThey're replaced by {replacement_name}."),
                        player_tags: vec![victim_id, replacement_id],
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::PitcherChange { game, team_nickname: team_name, pitcher_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .update(EventBuilderUpdate {
                        r#type: EventType::PitcherChange,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} is now pitching for the {team_name}."),
                        player_tags: vec![pitcher_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Party { ref game, team_id, player_id, ref player_name, ref sub_event, rating_before, rating_after } => {
                let description = format!("{player_name} is Partying!");
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerHatched,
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerAddedToTeam,
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::FinalStandings,
                        description: format!("The {team_nickname} finished {place_str} in the {division_name}."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "place": place }))
                    .build()
            }
            FedEventData::TeamLeftPartyTimeForPostseason { team_id, team_name } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        description: format!("The {team_name} have been removed from Party Time to join the Postseason!"),
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::EarnedPostseasonSlot,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} earned a spot in the Season {} Postseason.", self.season + 1),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PostseasonAdvance { team_id, team_nickname, round, season } => {
                let round_str = if let Some(round) = round {
                    format!("Round {round}")
                } else {
                    String::from("The Internet Series")
                };
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::PostseasonAdvance,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} advanced to {round_str} of the Season {season} Postseason."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PostseasonEliminated { team_id, team_nickname, season } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
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
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::WillRecieved,
                        category: EventCategory::Outcomes,
                        description: format!("Will Received: {will_title}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::BlessingWon { team_id, blessing_title, metadata } => {
                event_builder
                    .update(EventBuilderUpdate {
                        r#type: EventType::BlessingOrGiftWon,
                        category: EventCategory::Outcomes,
                        description: format!("Blessing Won: {blessing_title}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::Earlbirds { ref game, team_id, ref team_nickname, ref sub_event } => {
                let child = EventBuilder::child(sub_event)
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::Earlbird,
                        category: EventCategory::Special,
                        description: format!("Happy Earlseason!\nThe {team_nickname} are Earlbirds!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::DecreePassed { decree_title, metadata } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::PlayerDivisionMove,
                        description: format!("{player_name} has joined the ILB."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PlayerPermittedToStay { player_id, player_name } => {
                event_builder
                    .update(EventBuilderUpdate {
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
                    .update(EventBuilderUpdate {
                        r#type: EventType::IncinerationBlocked,
                        category: EventCategory::Special,
                        description: format!("Rogue Umpire tried to incinerate {player_name}, but they're Fireproof! The Umpire was incinerated instead!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
        }
    }

    fn make_mod_change_sub_events(&self, game: &GameEvent, mod_changes: &[ModChangeSubEventWithNamedPlayer], event_type: EventType, message: &str, mod_name: &str) -> (Vec<EventBuilderChildFullWithMetadata>, String) {
        let suffix = mod_changes.iter()
            .map(|e| format!("\n{} {message}", e.player_name))
            .join("");

        let children = mod_changes.iter()
            .enumerate()
            .map(|(sub_play, e)| {
                EventBuilder::child(&e.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: event_type,
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

    fn push_red_hot(&self, game: &GameEvent, player_name: &str, batter_id: Uuid, spicy_status: &SpicyStatus, children: &mut Vec<EventBuilderChildFullWithMetadata>) {
        if let SpicyStatus::RedHot(Some(red_hot)) = spicy_status {
            children.push(
                EventBuilder::child(&red_hot.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} is Red Hot!"),
                        team_tags: vec![red_hot.team_id],
                        player_tags: vec![batter_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "ON_FIRE",
                        "type": 0, // ?
                    })),
            )
        }
    }

    fn push_cooled_off(&self, game: &GameEvent, player_name: &str, cooled_off: &Option<ModChangeSubEventWithPlayer>, children: &mut Vec<EventBuilderChildFullWithMetadata>, player_tags: &mut Vec<Uuid>) -> String {
        if let Some(cooled_off) = cooled_off {
            children.push(
                EventBuilder::child(&cooled_off.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} cooled off."),
                        team_tags: vec![cooled_off.team_id],
                        player_tags: vec![cooled_off.player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "ON_FIRE",
                        "type": 0, // ?
                    }))
            );

            player_tags.push(cooled_off.player_id);
            format!("\n{player_name} cooled off.")
        } else {
            String::new()
        }
    }

    fn stopped_inhabiting_children(&self, game: &GameEvent, stopped_inhabiting: &Option<StoppedInhabiting>) -> Vec<EventBuilderChildFullWithMetadata> {
        let mut vec = Vec::new();
        self.push_stopped_inhabiting(game, stopped_inhabiting, &mut vec);
        vec
    }

    fn push_stopped_inhabiting(&self, game: &GameEvent, stopped_inhabiting: &Option<StoppedInhabiting>, children: &mut Vec<EventBuilderChildFullWithMetadata>) {
        if let Some(inh) = stopped_inhabiting {
            children.push(
                EventBuilder::child(&inh.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{} stopped Inhabiting.", inh.inhabiting_player_name),
                        player_tags: vec![inh.inhabiting_player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "INHABITING",
                        "type": 0, // ?
                    }))
            )
        }
    }

    fn get_score_data(&self, game: &GameEvent, scores: &ScoreInfo, score_text: &str) -> (String, bool, Vec<EventBuilderChildFullWithMetadata>) {
        let score_text = scores.to_description(score_text);
        let has_any_refills = !scores.free_refills.is_empty();
        let children: Vec<_> = scores.free_refills.iter()
            .map(|free_refill| self.make_free_refill_child(game, free_refill))
            .collect();
        (score_text, has_any_refills, children)
    }

    fn make_event_builder(&self) -> EventBuilderCommon {
        EventBuilderCommon {
            id: self.id,
            created: self.created,
            sim: self.sim.clone(),
            day: self.day,
            phase: self.phase.into(),
            season: self.season,
            tournament: self.tournament,
            nuts: self.nuts,
        }
    }

    fn make_free_refill_child(&self, game: &GameEvent, free_refill: &FreeRefill) -> EventBuilderChildFullWithMetadata {
        EventBuilder::child(&free_refill.sub_event)
            .update(EventBuilderUpdate {
                r#type: EventType::RemovedMod,
                category: EventCategory::Changes,
                description: format!("{} used their Free Refill.", free_refill.player_name),
                team_tags: vec![free_refill.team_id],
                player_tags: vec![free_refill.player_id],
                ..Default::default()
            })
            .metadata(json!({
                "mod": "COFFEE_RALLY",
                "type": 0, // ?
            }))
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
