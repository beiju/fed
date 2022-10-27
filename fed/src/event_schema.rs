use std::fmt::{Display, Formatter, Write};
use std::iter;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::json;
use uuid::Uuid;
use fed_api::{EventMetadata, EventMetadataBuilder, EventType, EventuallyEvent, EventuallyEventBuilder, Weather};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use derive_builder::Builder;
use crate::error::FeedParseError;

#[derive(Debug, Clone, IntoPrimitive, TryFromPrimitive)]
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

#[derive(Debug, Clone)]
pub struct GameEvent {
    pub game_id: Uuid,
    pub home_team: Uuid,
    pub away_team: Uuid,
    pub play: i64,
    pub sub_play: i64,
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
                .ok_or_else(|| FeedParseError::MissingMetadata {
                    event_type: event.r#type,
                    field: "play",
                })?,
            sub_play: event.metadata.sub_play
                .ok_or_else(|| FeedParseError::MissingMetadata {
                    event_type: event.r#type,
                    field: "sub_play",
                })?,
        })
    }
}

// This contains only the event properties that will differ from the parent, including id, created,
// and nuts; but not properties that will be the same, like day, season, and tournament.
#[derive(Debug, Clone)]
pub struct SubEvent {
    pub id: Uuid,
    pub created: DateTime<Utc>,
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

#[derive(Debug, Clone)]
pub struct FreeRefill {
    pub sub_event: SubEvent,
    pub player_name: String,
    pub player_id: Uuid,
    pub team_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ScoringPlayer {
    pub player_id: Uuid,
    pub player_name: String,
}

#[derive(Debug, Clone)]
pub struct ScoreInfo {
    pub scoring_players: Vec<ScoringPlayer>,
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

#[derive(Debug, Clone)]
pub struct Inhabiting {
    pub sub_event: SubEvent,
    pub inhabited_player_name: String,
    pub inhabiting_player_id: Uuid,
    pub inhabited_player_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct StoppedInhabiting {
    pub sub_event: SubEvent,
    pub inhabiting_player_name: String,
    pub inhabiting_player_id: Uuid,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy)]
pub enum AttrCategory {
    Batting,
    Baserunning,
    Pitching,
    Defense,
}

impl Display for AttrCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AttrCategory::Batting => { write!(f, "hitting") }
            AttrCategory::Baserunning => { write!(f, "baserunning") }
            AttrCategory::Pitching => { write!(f, "pitching") }
            AttrCategory::Defense => { write!(f, "defensive") }
        }
    }
}

impl AttrCategory {
    pub fn metadata_type(&self) -> i32 {
        match self {
            AttrCategory::Batting => { 0 }
            AttrCategory::Baserunning => { -1 } // TODO
            AttrCategory::Pitching => { 1 }
            AttrCategory::Defense => { 2 }
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
#[repr(i32)]
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

#[derive(Debug, Clone)]
pub struct ModChangeSubEvent {
    pub sub_event: SubEvent,
    pub team_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ModChangeSubEventWithPlayer {
    pub sub_event: SubEvent,
    pub team_id: Uuid,
    pub player_id: Uuid,
}

#[derive(Debug, Clone)]
pub enum SpicyStatus {
    None,
    HeatingUp,
    RedHot(ModChangeSubEvent),
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

#[derive(Debug, Clone)]
pub struct PlayerStatChange {
    pub team_id: Uuid,
    pub player_id: Uuid,
    pub player_name: String,
    pub rating_before: f64,
    pub rating_after: f64,
    pub sub_event: SubEvent,
}

#[derive(Debug, Clone, Copy)]
pub enum PositionType {
    Batter,
    Pitcher,
}

impl Display for PositionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionType::Batter => { write!(f, "batting") }
            PositionType::Pitcher => { write!(f, "pitching") }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeedbackPlayerData {
    pub team_id: Uuid,
    pub team_nickname: String,
    pub player_id: Uuid,
    pub player_name: String,
    pub location: i64,
}

#[derive(Debug, Clone)]
pub struct PerkPlayers {
    pub team_id: Uuid,
    pub player_id: Uuid,
    pub player_name: String,
    pub sub_event: SubEvent,
    pub sub_play: i64,
}

#[derive(Debug, Clone)]
pub enum FedEventData {
    BeingSpeech {
        being: Being,
        message: String,
    },

    LetsGo {
        game: GameEvent,
        weather: Weather,
    },

    PlayBall {
        game: GameEvent,
    },

    HalfInningStart {
        game: GameEvent,
        top_of_inning: bool,
        inning: i32,
        batting_team_name: String,
    },

    BatterUp {
        game: GameEvent,
        batter_name: String,
        team_name: String,
        wielding_item: Option<String>,
        inhabiting: Option<Inhabiting>,
        is_repeating: bool,
    },

    SuperyummyGameStart {
        game: GameEvent,
        player_id: Uuid,
        team_id: Uuid,
        player_name: String,
        peanuts: bool,
        is_first_proc: bool,
        sub_event: SubEvent,
    },

    Ball {
        game: GameEvent,
        balls: i32,
        strikes: i32,
    },

    FoulBall {
        game: GameEvent,
        balls: i32,
        strikes: i32,
    },

    StrikeSwinging {
        game: GameEvent,
        balls: i32,
        strikes: i32,
    },

    StrikeLooking {
        game: GameEvent,
        balls: i32,
        strikes: i32,
    },

    StrikeFlinching {
        game: GameEvent,
        balls: i32,
        strikes: i32,
    },

    Flyout {
        game: GameEvent,
        batter_name: String,
        fielder_name: String,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
        cooled_off: Option<ModChangeSubEventWithPlayer>,
    },

    GroundOut {
        game: GameEvent,
        batter_name: String,
        fielder_name: String,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
        cooled_off: Option<ModChangeSubEventWithPlayer>,
        // In Season 12, Tired/Wired scoring ground outs were special but didn't have an associated
        // child event
        is_special: bool,
    },

    FieldersChoice {
        game: GameEvent,
        batter_name: String,
        runner_out_name: String,
        out_at_base: i32,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    DoublePlay {
        game: GameEvent,
        batter_name: String,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    Hit {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
        num_bases: i32,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
        spicy_status: SpicyStatus,
    },

    HomeRun {
        game: GameEvent,
        magmatic: Option<(SubEvent, Uuid)>,
        batter_name: String,
        batter_id: Uuid,
        num_runs: i32,
        stopped_inhabiting: Option<StoppedInhabiting>,
        free_refills: Vec<FreeRefill>,
        spicy_status: SpicyStatus,
    },

    StolenBase {
        game: GameEvent,
        runner_name: String,
        runner_id: Uuid,
        base_stolen: i32,
        blaserunning: bool,
        free_refill: Option<FreeRefill>,
    },

    CaughtStealing {
        game: GameEvent,
        runner_name: String,
        base_stolen: i32,
    },

    StrikeoutSwinging {
        game: GameEvent,
        batter_name: String,
        stopped_inhabiting: Option<StoppedInhabiting>,
    },

    StrikeoutLooking {
        game: GameEvent,
        batter_name: String,
        stopped_inhabiting: Option<StoppedInhabiting>,
        // In Season 12, Unrun strikeouts were special but didn't have an associated child event
        is_special: bool,
    },

    Walk {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
        scores: ScoreInfo,
        stopped_inhabiting: Option<StoppedInhabiting>,
        base_instincts: Option<i32>,
    },

    InningEnd {
        game: GameEvent,
        inning_num: i32,
    },

    CharmStrikeout {
        game: GameEvent,
        charmer_id: Uuid,
        charmer_name: String,
        charmed_id: Uuid,
        charmed_name: String,
        num_swings: i32,
    },

    StrikeZapped {
        game: GameEvent,
    },

    PeanutFlavorText {
        game: GameEvent,
        message: String,
    },

    GameEnd {
        game: GameEvent,
        winner_id: Uuid,
        winning_team_name: String,
        winning_team_score: f32,
        losing_team_name: String,
        losing_team_score: f32,
    },

    MildPitch {
        game: GameEvent,
        pitcher_id: Uuid,
        pitcher_name: String,
        balls: i32,
        strikes: i32,
        runners_advance: bool,
    },

    MildPitchWalk {
        game: GameEvent,
        pitcher_id: Uuid,
        pitcher_name: String,
        batter_id: Uuid,
        batter_name: String,
    },

    CoffeeBean {
        game: GameEvent,
        player_id: Uuid,
        player_name: String,
        roast: String,
        notes: String,
        which_mod: CoffeeBeanMod,
        has_mod: bool,
        sub_event: SubEvent,
        team_id: Uuid,
        previous: Option<CoffeeBeanMod>,
    },

    BecameMagmatic {
        game: GameEvent,
        player_id: Uuid,
        player_name: String,
        team_id: Uuid,
        mod_add_event: SubEvent,
    },

    Blooddrain {
        game: GameEvent,
        is_siphon: bool,
        sipper: PlayerStatChange,
        sipped: PlayerStatChange,
        sipped_category: AttrCategory,
    },

    SpecialBlooddrain {
        game: GameEvent,
        sipper_id: Uuid,
        sipper_name: String,
        sipped_id: Uuid,
        sipped_team_id: Uuid,
        sipped_name: String,
        sipped_category: AttrCategory,
        action: BlooddrainAction,
        sipped_event: SubEvent,
        rating_before: f64,
        rating_after: f64,
    },

    ModExpires {
        team_id: Uuid,
        player_id: Uuid,
        player_name: String,
        mods: Vec<String>,
        mod_duration: ModDuration,
    },

    BirdsCircle {
        game: GameEvent,
    },

    FriendOfCrows {
        game: GameEvent,
        batter_id: Uuid,
        batter_name: String,
        pitcher_id: Uuid,
        pitcher_name: String,
    },

    BlackHoleSwallowedWin {
        team_id: Uuid,
        team_nickname: String,
    },

    Sun2SetWin {
        team_id: Uuid,
        team_nickname: String,
    },

    Sun2 {
        game: GameEvent,
        team_nickname: String,
    },

    BlackHole {
        game: GameEvent,
        scoring_team_nickname: String,
        victim_team_nickname: String,
    },

    TeamDidShame {
        shaming_team_id: Uuid,
        shaming_team_nickname: String,
        shamed_team_nickname: String,
        total_shames: i64,
        total_shamings: i64,
    },

    TeamWasShamed {
        shamed_team_id: Uuid,
        shaming_team_nickname: String,
        shamed_team_nickname: String,
        total_shames: i64,
        total_shamings: i64,
    },

    CharmWalk {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
        pitcher_name: String,
    },

    GainFreeRefill {
        game: GameEvent,
        player_id: Uuid,
        player_name: String,
        roast: String,
        ingredient1: String,
        ingredient2: String,
        sub_event: SubEvent,
        team_id: Uuid,
    },

    AllergicReaction {
        game: GameEvent,
        team_id: Uuid,
        player_id: Uuid,
        player_name: String,
        sub_event: SubEvent,
        rating_before: f64,
        rating_after: f64,
    },

    PerkUp {
        game: GameEvent,
        players: Vec<PerkPlayers>,
    },

    Feedback {
        game: GameEvent,
        players: (FeedbackPlayerData, FeedbackPlayerData),
        position_type: PositionType,
        sub_event: SubEvent,
    },
}

#[derive(Debug, Builder)]
pub struct FedEvent {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: String,
    pub tournament: i32,
    pub season: i32,
    pub day: i32,
    pub phase: i32,
    pub nuts: i32,
    pub data: FedEventData,
}

trait GameEventForBuilder {
    fn for_game(self, game: &GameEvent) -> Self;
    fn for_sub_event(self, sub: &SubEvent) -> Self;
}

impl GameEventForBuilder for EventuallyEventBuilder {
    fn for_game(self, game: &GameEvent) -> Self {
        self
            .category(0)
            .game_tags(vec![game.game_id])
            .team_tags(vec![game.away_team, game.home_team])
            .metadata(make_game_event_metadata(&game))
    }

    fn for_sub_event(self, sub: &SubEvent) -> Self {
        self
            .id(sub.id)
            .created(sub.created)
            .nuts(sub.nuts)
    }
}

impl FedEvent {
    pub fn into_feed_event(self) -> EventuallyEvent {
        let event_builder = self.make_event_builder();

        match self.data {
            FedEventData::BeingSpeech { being, message } => {
                let being_id: i32 = being.into();
                event_builder
                    .r#type(EventType::BigDeal)
                    .category(4)
                    .description(message)
                    .metadata(
                        EventMetadataBuilder::default()
                            .other(json!({ "being": being_id }))
                            .build()
                            .unwrap())
            }
            FedEventData::LetsGo { game, weather } => {
                let weather_id: i32 = weather.into();
                event_builder.for_game(&game)
                    .r#type(EventType::LetsGo)
                    .description("Let's Go!".to_string())
                    .metadata(
                        make_game_event_metadata_builder(&game)
                            .other(json!({
                                "home": game.home_team,
                                "away": game.away_team,
                                "weather": weather_id,
                            }))
                            .build()
                            .unwrap())
            }
            FedEventData::PlayBall { game } => {
                event_builder.for_game(&game)
                    .r#type(EventType::PlayBall)
                    .description("Play ball!".to_string())
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::HalfInning)
                    .description(format!("{} of {}, {} batting.",
                                         if top_of_inning { "Top" } else { "Bottom" },
                                         inning,
                                         batting_team_name))
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
                event_builder.for_game(&game)
                    .r#type(EventType::BatterUp)
                    .category(if inhabiting.is_some() || is_repeating { 2 } else { 0 })
                    .description(if let Some(inhabiting) = &inhabiting {
                        format!("{prefix}{} is Inhabiting {}!\n{} batting for the {}{}.", batter_name,
                                inhabiting.inhabited_player_name, batter_name, team_name, item_suffix)
                    } else {
                        format!("{prefix}{} batting for the {}{}.", batter_name, team_name, item_suffix)
                    })
                    .player_tags(if let Some(inhabiting) = inhabiting {
                        vec![inhabiting.inhabiting_player_id, inhabiting.inhabited_player_id]
                    } else {
                        vec![]
                    })
                    .metadata(
                        make_game_event_metadata_builder(&game)
                            .children(inhabiting.iter()
                                .map(|inhabiting| {
                                    self.make_event_builder()
                                        .for_game(&game)
                                        .for_sub_event(&inhabiting.sub_event)
                                        .category(1)
                                        .r#type(EventType::AddedMod)
                                        .description(format!("{} is Inhabiting {}!",
                                                             batter_name, inhabiting.inhabited_player_name))
                                        .player_tags(vec![inhabiting.inhabiting_player_id])
                                        .team_tags(vec![]) // need to clear it
                                        .metadata(EventMetadataBuilder::default()
                                            .play(game.play)
                                            .sub_play(0) // not sure if this is hardcoded
                                            .other(json!({
                                            "mod": "INHABITING",
                                            "type": 0, // ?
                                            "parent": self.id
                                        }))
                                            .build()
                                            .unwrap()
                                        )
                                        .build()
                                        .unwrap()
                                })
                                .collect())
                            .build()
                            .unwrap())
            }
            FedEventData::SuperyummyGameStart { ref game, ref player_name, peanuts, is_first_proc, ref sub_event, player_id, team_id } => {
                let description = format!("{} {} Peanuts.", player_name,
                                          if peanuts { "loves" } else { "misses" });
                let mod_name = if peanuts { "OVERPERFORMING" } else { "UNDERPERFORMING" };
                let change_event = if is_first_proc {
                    self.make_event_builder()
                        .for_game(&game)
                        .for_sub_event(&sub_event)
                        .category(1)
                        .r#type(EventType::AddedModFromOtherMod)
                        .description(description.clone())
                        .team_tags(vec![team_id])
                        .player_tags(vec![player_id])
                        .metadata(EventMetadataBuilder::default()
                            .play(game.play)
                            .sub_play(0) // not sure if this is hardcoded
                            .other(json!({
                                "mod": mod_name,
                                "source": "SUPERYUMMY",
                                "type": 0, // ?
                                "parent": self.id
                            }))
                            .build()
                            .unwrap()
                        )
                        .build()
                        .unwrap()
                } else {
                    todo!()
                };
                event_builder.for_game(&game)
                    .category(2)
                    .r#type(EventType::Superyummy)
                    .description(description)
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![change_event])
                        .build()
                        .unwrap())
            }
            FedEventData::Ball { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Ball)
                    .description(format!("Ball. {}-{}", balls, strikes))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeSwinging { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strike)
                    .description(format!("Strike, swinging. {}-{}", balls, strikes))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeLooking { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strike)
                    .description(format!("Strike, looking. {}-{}", balls, strikes))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeFlinching { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strike)
                    .description(format!("Strike, flinching. {}-{}", balls, strikes))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::FoulBall { game, balls, strikes } => {
                event_builder.for_game(&game)
                    .r#type(EventType::FoulBall)
                    .description(format!("Foul Ball. {}-{}", balls, strikes))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::Flyout { ref game, ref batter_name, ref fielder_name, ref scores, ref stopped_inhabiting, ref cooled_off } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " tags up and scores!");
                let mut player_tags = scores.scorer_ids();

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                let suffix = self.push_cooled_off(&game, batter_name, cooled_off, &mut children, &mut player_tags);

                event_builder.for_game(&game)
                    .r#type(EventType::FlyOut)
                    .category(if has_any_refills || cooled_off.is_some() { 2 } else { 0 })
                    .description(format!("{} hit a flyout to {}.{}{}", batter_name, fielder_name, score_text, suffix))
                    .player_tags(player_tags)
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::Hit { ref game, ref batter_name, batter_id, num_bases, ref scores, ref stopped_inhabiting, ref spicy_status } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                self.push_red_hot(&game, batter_name, batter_id, spicy_status, &mut children);

                let spicy_text = match spicy_status {
                    SpicyStatus::None => String::new(),
                    SpicyStatus::HeatingUp => format!("\n{} is Heating Up!", batter_name),
                    SpicyStatus::RedHot { .. } => format!("\n{} is Red Hot!", batter_name),
                };
                event_builder.for_game(&game)
                    .r#type(EventType::Hit)
                    .category(if has_any_refills || spicy_status.is_special() { 2 } else { 0 })
                    .description(format!("{} hits a {}!{}{}", batter_name, match num_bases {
                        1 => "Single",
                        2 => "Double",
                        3 => "Triple",
                        4 => "Quadruple",
                        // TODO Turn this into a Result error
                        _ => panic!("Unknown hit type")
                    }, score_text, spicy_text))
                    .player_tags(
                        iter::once(batter_id)
                            .chain(scores.scorer_ids())
                            .chain(if spicy_status.is_none() { None } else { Some(batter_id) }.into_iter())
                            .collect()
                    )
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::HomeRun { ref game, ref magmatic, ref batter_name, batter_id, num_runs, ref free_refills, ref spicy_status, ref stopped_inhabiting } => {
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

                let mut children = if let Some((sub_event, team_id)) = magmatic {
                    vec![self.make_event_builder()
                        .for_game(&game)
                        .for_sub_event(&sub_event)
                        .category(1)
                        .r#type(EventType::RemovedMod)
                        .description(format!("{batter_name} hit a Magmatic home run!"))
                        .team_tags(vec![*team_id])
                        .player_tags(vec![batter_id])
                        .metadata(EventMetadataBuilder::default()
                            .play(game.play)
                            .sub_play(0) // not sure if this is hardcoded
                            .other(json!({
                                "mod": "MAGMATIC",
                                "type": 0, // ?
                                "parent": self.id
                            }))
                            .build()
                            .unwrap()
                        )
                        .build()
                        .unwrap()]
                } else {
                    Vec::new()
                };

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                self.push_red_hot(&game, batter_name, batter_id, spicy_status, &mut children);

                event_builder.for_game(&game)
                    .r#type(EventType::HomeRun)
                    .category(if magmatic.is_some() || !free_refills.is_empty() || spicy_status.is_special() { 2 } else { 0 })
                    .description(format!("{}{} hits a {}!{}",
                                         if magmatic.is_some() { format!("{batter_name} is Magmatic!\n") } else { String::new() },
                                         batter_name,
                                         match num_runs {
                                             1 => "solo home run",
                                             2 => "2-run home run",
                                             3 => "3-run home run",
                                             4 => "grand slam",
                                             // TODO Turn this into a Result error
                                             _ => panic!("Unknown num runs in home run")
                                         },
                                         suffix
                    ))
                    .player_tags(if spicy_status.is_none() { vec![batter_id] } else { vec![batter_id, batter_id] })
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(
                            free_refills.iter()
                                .map(|free_refill| self.make_free_refill_child(&game, free_refill))
                                .chain(children.into_iter())
                                .collect()
                        )

                        .build()
                        .unwrap())
            }
            FedEventData::GroundOut { ref game, ref batter_name, ref fielder_name, ref scores, ref stopped_inhabiting, ref cooled_off, is_special } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " advances on the sacrifice.");
                let mut player_tags = scores.scorer_ids();

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);
                let suffix = self.push_cooled_off(&game, batter_name, cooled_off, &mut children, &mut player_tags);

                event_builder.for_game(&game)
                    .r#type(EventType::GroundOut)
                    .category(if has_any_refills || cooled_off.is_some() || is_special { 2 } else { 0 })
                    .description(format!("{} hit a ground out to {}.{}{}",
                                         batter_name, fielder_name, score_text, suffix))
                    .player_tags(player_tags)
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
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
                event_builder.for_game(&game)
                    .r#type(EventType::StolenBase)
                    .category(if blaserunning || free_refill.is_some() { 2 } else { 0 })
                    .description(format!("{} steals {} base!{}{}", runner_name, base_name(base_stolen), blaserunning_str, free_refill_str))
                    .player_tags(if blaserunning { vec![runner_id, runner_id] } else { vec![runner_id] })
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(
                            free_refill.as_ref()
                                .map(|free_refill| self.make_free_refill_child(&game, free_refill))
                                .into_iter()
                                .collect()
                        )
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeoutSwinging { ref game, ref batter_name, ref stopped_inhabiting } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strikeout)
                    .description(format!("{} strikes out swinging.", batter_name))
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(self.stopped_inhabiting_children(&game, &stopped_inhabiting))
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeoutLooking { ref game, ref batter_name, ref stopped_inhabiting, is_special } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strikeout)
                    .category(if is_special { 2 } else { 0 })
                    .description(format!("{} strikes out looking.", batter_name))
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(self.stopped_inhabiting_children(&game, &stopped_inhabiting))
                        .build()
                        .unwrap())
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

                event_builder.for_game(&game)
                    .r#type(EventType::Walk)
                    .category(if has_any_refills || base_instincts.is_some() { 2 } else { 0 })
                    .description(format!("{} draws a walk.{}{}", batter_name, base_instincts_str, score_text))
                    .player_tags(iter::once(batter_id).chain(scores.scorer_ids()).collect())
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen } => {
                event_builder.for_game(&game)
                    .r#type(EventType::StolenBase)
                    .description(format!("{} gets caught stealing {} base.", runner_name, base_name(base_stolen)))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::InningEnd { game, inning_num } => {
                event_builder.for_game(&game)
                    .r#type(EventType::InningEnd)
                    .description(format!("Inning {} is now an Outing.", inning_num))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::CharmStrikeout { game, charmer_id, charmer_name, charmed_id, charmed_name, num_swings } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strikeout)
                    .category(2)
                    .description(format!("{} charmed {}!\n{} swings {} times to strike out willingly!",
                                         charmer_name, charmed_name, charmed_name, num_swings))
                    // I do not know why the charmer appears twice, but that seems to be accurate
                    .player_tags(vec![charmer_id, charmer_id, charmed_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::FieldersChoice { ref game, ref batter_name, ref runner_out_name, out_at_base, ref scores, ref stopped_inhabiting } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                event_builder.for_game(&game)
                    .r#type(EventType::GroundOut)
                    .category(if has_any_refills { 2 } else { 0 })
                    .description(format!("{} out at {} base.{}\n{} reaches on fielder's choice.",
                                         runner_out_name, base_name(out_at_base), score_text, batter_name))
                    .player_tags(scores.scorer_ids())
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeZapped { game } => {
                event_builder.for_game(&game)
                    .r#type(EventType::StrikeZapped)
                    .category(2)
                    .description("The Electricity zaps a strike away!".to_string())
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::PeanutFlavorText { game, message } => {
                event_builder.for_game(&game)
                    .r#type(EventType::PeanutFlavorText)
                    .category(2)
                    .description(message)
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::DoublePlay { ref game, ref batter_name, ref scores, ref stopped_inhabiting } => {
                let (score_text, has_any_refills, mut children) =
                    self.get_score_data(game, scores, " scores!");

                self.push_stopped_inhabiting(game, stopped_inhabiting, &mut children);

                event_builder.for_game(&game)
                    .r#type(EventType::GroundOut)
                    .category(if has_any_refills { 2 } else { 0 })
                    .description(format!("{} hit into a double play!{}", batter_name, score_text))
                    .player_tags(scores.scorer_ids())
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::GameEnd { game, winner_id, winning_team_name, winning_team_score, losing_team_name, losing_team_score } => {
                event_builder.for_game(&game)
                    .r#type(EventType::GameEnd)
                    .category(3)
                    .description(format!("{} {}, {} {}", winning_team_name, winning_team_score, losing_team_name, losing_team_score))
                    .team_tags(vec![
                        // For some reason the teams are repeated like this? idk why
                        game.away_team, game.home_team, game.home_team, game.away_team,
                    ])
                    .metadata(make_game_event_metadata_builder(&game)
                        .other(json!({ "winner": winner_id }))
                        .build()
                        .unwrap())
            }
            FedEventData::MildPitch { game, pitcher_id, pitcher_name, balls, strikes, runners_advance } => {
                let runners_advance_str = if runners_advance {
                    "\nRunners advance on the pathetic play!"
                } else {
                    ""
                };

                event_builder.for_game(&game)
                    .r#type(EventType::MildPitch)
                    .category(2)
                    .description(format!("{} throws a Mild pitch!\nBall, {}-{}.{}", pitcher_name, balls, strikes, runners_advance_str))
                    .player_tags(vec![pitcher_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::CoffeeBean { ref game, player_id, ref player_name, ref roast, ref notes, ref which_mod, has_mod, ref sub_event, team_id, ref previous } => {
                let change_str = if has_mod { "is" } else { "is no longer" };
                let mod_str = match which_mod {
                    CoffeeBeanMod::Wired => { "Wired!" }
                    CoffeeBeanMod::Tired => { "Tired." }
                };
                let mod_id = which_mod.to_str();
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&sub_event)
                    .category(1)
                    .r#type(if previous.is_some() { EventType::ModChange } else { EventType::AddedMod })
                    .description(format!("{} {} {}", player_name, change_str, mod_str))
                    .team_tags(vec![team_id])
                    .player_tags(vec![player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(if let Some(prev_mod) = previous {
                            let prev_mod_id = prev_mod.to_str();
                            json!({
                                "from": prev_mod_id,
                                "to": mod_id,
                                "type": 3, // ?
                                "parent": self.id
                            })
                        } else {
                            json!({
                                "mod": mod_id,
                                "type": 3, // ?
                                "parent": self.id
                            })
                        })
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();

                event_builder.for_game(&game)
                    .r#type(EventType::CoffeeBean)
                    .category(2)
                    .description(format!("{} is Beaned by a {} roast with {}.\n{} {} {}",
                                         player_name, roast, notes, player_name, change_str, mod_str))
                    .player_tags(vec![player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
            FedEventData::BecameMagmatic { ref game, player_id, ref player_name, team_id, ref mod_add_event } => {
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&mod_add_event)
                    .category(1)
                    .r#type(EventType::AddedMod)
                    .description(format!("{} ate some flame.", player_name))
                    .team_tags(vec![team_id])
                    .player_tags(vec![player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                                "mod": "MAGMATIC",
                                "type": 0, // ?
                                "parent": self.id
                            }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();
                event_builder.for_game(&game)
                    .r#type(EventType::IncinerationBlocked)
                    .category(2)
                    .description(format!("Rogue Umpire tried to incinerate {}, but {} ate the flame! They became Magmatic!",
                                         player_name, player_name))
                    .player_tags(vec![player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
            FedEventData::SpecialBlooddrain { ref game, sipper_id, ref sipper_name, sipped_id, sipped_team_id, ref sipped_name, ref sipped_category, ref action, ref sipped_event, rating_before, rating_after } => {
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&sipped_event)
                    .category(1)
                    .r#type(EventType::PlayerStatDecrease)
                    .description(format!("{sipped_name} had blood drained by {sipper_name}."))
                    .team_tags(vec![sipped_team_id])
                    .player_tags(vec![sipped_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                                "type": sipped_category.metadata_type(), // ?
                                "parent": self.id,
                                "before": rating_before,
                                "after": rating_after,
                            }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();
                event_builder.for_game(&game)
                    .r#type(EventType::BlooddrainSiphon)
                    .category(2)
                    .description(format!("The Blooddrain gurgled!\n{sipper_name}'s Siphon activates!\n{sipper_name} siphoned some of {sipped_name}'s {sipped_category} ability!\n{sipper_name} {action}!"))
                    .player_tags(vec![sipper_id, sipped_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
            FedEventData::ModExpires { team_id, player_id, player_name, mods, mod_duration } => {
                let player_name_possessive = if player_name.chars().last().unwrap() == 's' {
                    player_name + "'"
                } else {
                    player_name + "'s"
                };
                let duration_str = mod_duration.to_string();
                event_builder
                    .r#type(EventType::ModExpires)
                    .category(1)
                    .description(format!("{player_name_possessive} {duration_str} mods wore off."))
                    .team_tags(vec![team_id])
                    .player_tags(vec![player_id])
                    .metadata(EventMetadataBuilder::default()
                        .other(json!({
                            "mods": mods,
                            "type": mod_duration as i32
                        }))
                        .build()
                        .unwrap())
            }
            FedEventData::BirdsCircle { game } => {
                event_builder.for_game(&game)
                    .r#type(EventType::BirdsCircle)
                    .category(2)
                    .description("The Birds circle ... but they don't find what they're looking for.".to_string())
            }
            FedEventData::FriendOfCrows { game, batter_id, batter_name, pitcher_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::FriendOfCrows)
                    .category(2)
                    .description(format!("{pitcher_name} calls upon their Friends!\nA murder of Crows ambush {batter_name}!\nThey run to safety, resulting in an out."))
                    .player_tags(vec![pitcher_id, batter_id])
            }
            FedEventData::Sun2SetWin { team_id, team_nickname } => {
                event_builder
                    .r#type(EventType::Sun2SetWin)
                    .category(3)
                    .description(format!("Sun 2 set a Win upon the {team_nickname}."))
                    .team_tags(vec![team_id])
            }
            FedEventData::BlackHoleSwallowedWin { team_id, team_nickname } => {
                event_builder
                    .r#type(EventType::BlackHoleSwallowedWin)
                    .category(3)
                    .description(format!("The Black Hole swallowed a Win from the {team_nickname}!"))
                    .team_tags(vec![team_id])
            }
            FedEventData::Sun2 { game, team_nickname } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Sun2)
                    .category(2)
                    .description(format!("The {team_nickname} collect 10! Sun 2 smiles.\nSun 2 set a Win upon the {team_nickname}."))
            }
            FedEventData::BlackHole { game, scoring_team_nickname, victim_team_nickname } => {
                event_builder.for_game(&game)
                    .r#type(EventType::BlackHole)
                    .category(2)
                    .description(format!("The {scoring_team_nickname} collect 10!\nThe Black Hole swallows the Runs and a {victim_team_nickname} Win."))
            }
            FedEventData::TeamDidShame { shaming_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .r#type(EventType::TeamDidShame)
                    .category(3)
                    .description(format!("The {shaming_team_nickname} shamed the {shamed_team_nickname}."))
                    .team_tags(vec![shaming_team_id])
                    .metadata(EventMetadataBuilder::default()
                        .other(json!({
                            "totalShames": total_shames,
                            "totalShamings": total_shamings,
                        }))
                        .build()
                        .unwrap())
            }
            FedEventData::TeamWasShamed { shamed_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .r#type(EventType::TeamWasShamed)
                    .category(3)
                    .description(format!("The {shamed_team_nickname} were shamed by the {shaming_team_nickname}."))
                    .team_tags(vec![shamed_team_id])
                    .metadata(EventMetadataBuilder::default()
                        .other(json!({
                            "totalShames": total_shames,
                            "totalShamings": total_shamings,
                        }))
                        .build()
                        .unwrap())
            }
            FedEventData::CharmWalk { game, batter_name, batter_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Walk)
                    .category(2)
                    .description(format!("{batter_name} charms {pitcher_name}!\n{batter_name} walks to first base."))
                    .player_tags(vec![batter_id, batter_id]) // two of them
            }
            FedEventData::GainFreeRefill { ref game, player_id, ref player_name, ref roast, ref ingredient1, ref ingredient2, ref sub_event, team_id } => {
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&sub_event)
                    .category(1)
                    .r#type(EventType::AddedMod)
                    .description(format!("{player_name} got a Free Refill."))
                    .team_tags(vec![team_id])
                    .player_tags(vec![player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                            "mod": "COFFEE_RALLY",
                            "type": 0, // ?
                            "parent": self.id
                        }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();

                event_builder.for_game(&game)
                    .r#type(EventType::GainFreeRefill)
                    .category(2)
                    .description(format!("{player_name} is Poured Over with a {roast} roast blending {ingredient1} and {ingredient2}!\n{player_name} got a Free Refill."))
                    .player_tags(vec![player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
            FedEventData::AllergicReaction { ref game, team_id, player_id, ref player_name, ref sub_event, rating_before, rating_after } => {
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&sub_event)
                    .category(1)
                    .r#type(EventType::PlayerStatDecrease)
                    .description(format!("{player_name} had an allergic reaction."))
                    .team_tags(vec![team_id])
                    .player_tags(vec![player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                            "type": 4, // ?
                            "before": rating_before,
                            "after": rating_after,
                            "parent": self.id
                        }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();

                event_builder.for_game(&game)
                    .r#type(EventType::AllergicReaction)
                    .category(2)
                    .description(format!("{player_name} swallowed a stray peanut and had an allergic reaction!"))
                    .player_tags(vec![player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
            FedEventData::MildPitchWalk { game, pitcher_id, pitcher_name, batter_id, batter_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::MildPitch)
                    .category(2)
                    .description(format!("{pitcher_name} throws a Mild pitch!\n{batter_name} draws a walk."))
                    .player_tags(vec![pitcher_id, batter_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::PerkUp { ref game, ref players } => {
                let children = players.iter().map(|player| {
                    self.make_event_builder()
                        .for_game(&game)
                        .for_sub_event(&player.sub_event)
                        .category(1)
                        .r#type(EventType::AddedModFromOtherMod)
                        .description(format!("{} Perks up.", player.player_name))
                        .team_tags(vec![player.team_id])
                        .player_tags(vec![player.player_id])
                        .metadata(EventMetadataBuilder::default()
                            .play(game.play)
                            .sub_play(player.sub_play)
                            .other(json!({
                                "mod": "OVERPERFORMING",
                                "source": "PERK",
                                "type": 3, // ?
                                "parent": self.id,
                            }))
                            .build()
                            .unwrap()
                        )
                        .build()
                        .unwrap()
                })
                    .collect();

                event_builder.for_game(&game)
                    .r#type(EventType::Perk)
                    .category(2)
                    .description(players.iter()
                        .sorted_by_key(|player| player.sub_play)
                        .map(|player| format!("{} Perks up.", player.player_name))
                        .join("\n"))
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::Blooddrain { ref game, is_siphon, ref sipper, ref sipped, sipped_category } => {
                let children: Vec<_> = [
                    // why are the sub-events backwards??
                    (1, 2, sipper, EventType::PlayerStatIncrease, format!("{} drained blood from {}.", sipper.player_name, sipped.player_name)),
                    (0, 2, sipped, EventType::PlayerStatDecrease, format!("{} had blood drained by {}.", sipped.player_name, sipper.player_name))
                ].into_iter().map(|(sub_play, metadata_type, change, event_type, description)| {
                    self.make_event_builder()
                        .for_game(&game)
                        .for_sub_event(&change.sub_event)
                        .category(1)
                        .r#type(event_type)
                        .description(description)
                        .team_tags(vec![change.team_id])
                        .player_tags(vec![change.player_id])
                        .metadata(EventMetadataBuilder::default()
                            .play(game.play)
                            .sub_play(sub_play)
                            .other(json!({
                                "type": metadata_type, // ?
                                "parent": self.id,
                                "before": change.rating_before,
                                "after": change.rating_after,
                            }))
                            .build()
                            .unwrap()
                        )
                        .build()
                        .unwrap()
                })
                    .collect();

                let siphon_text = if is_siphon {
                    format!("\n{}'s Siphon activates!", sipper.player_name)
                } else {
                    String::new()
                };

                event_builder.for_game(&game)
                    .r#type(if is_siphon { EventType::BlooddrainSiphon } else { EventType::Blooddrain })
                    .category(2)
                    .description(format!("The Blooddrain gurgled!{siphon_text}\n{} siphoned some of {}'s {sipped_category} ability!\n{} increased their {sipped_category} ability!", sipper.player_name, sipped.player_name, sipper.player_name))
                    .player_tags(vec![sipper.player_id, sipped.player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::Feedback { ref game, players: (ref player_a, ref player_b), position_type, ref sub_event } => {
                let child = self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&sub_event)
                    .category(1)
                    .r#type(EventType::PlayerTraded)
                    .description("Reality flickered in the Feedback.".to_string())
                    .team_tags(vec![player_a.team_id, player_b.team_id])
                    .player_tags(vec![player_a.player_id, player_b.player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
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
                            "parent": self.id,
                        }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap();

                event_builder.for_game(&game)
                    .r#type(EventType::FeedbackSwap)
                    .category(2)
                    .description(format!("Reality flickers. Things look different ...\n{} and {} switch teams in the feedback!\n{} is now {position_type}.", player_a.player_name, player_b.player_name, player_b.player_name))
                    .player_tags(vec![player_a.player_id, player_b.player_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(vec![child])
                        .build()
                        .unwrap())
            }
        }
            .build()
            .unwrap()
    }

    fn push_red_hot(&self, game: &GameEvent, player_name: &str, batter_id: Uuid, spicy_status: &SpicyStatus, children: &mut Vec<EventuallyEvent>) {
        if let SpicyStatus::RedHot(red_hot) = spicy_status {
            children.push(
                self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&red_hot.sub_event)
                    .category(1)
                    .r#type(EventType::AddedMod)
                    .description(format!("{player_name} is Red Hot!"))
                    .team_tags(vec![red_hot.team_id])
                    .player_tags(vec![batter_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                                    "mod": "ON_FIRE",
                                    "type": 0, // ?
                                    "parent": self.id
                                }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap()
            )
        }
    }

    fn push_cooled_off(&self, game: &GameEvent, player_name: &str, cooled_off: &Option<ModChangeSubEventWithPlayer>, children: &mut Vec<EventuallyEvent>, player_tags: &mut Vec<Uuid>) -> String {
        if let Some(cooled_off) = cooled_off {
            children.push(
                self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&cooled_off.sub_event)
                    .category(1)
                    .r#type(EventType::RemovedMod)
                    .description(format!("{player_name} cooled off."))
                    .team_tags(vec![cooled_off.team_id])
                    .player_tags(vec![cooled_off.player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                                    "mod": "ON_FIRE",
                                    "type": 0, // ?
                                    "parent": self.id
                                }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap()
            );

            player_tags.push(cooled_off.player_id);
            format!("\n{player_name} cooled off.")
        } else {
            String::new()
        }
    }

    fn stopped_inhabiting_children(&self, game: &GameEvent, stopped_inhabiting: &Option<StoppedInhabiting>) -> Vec<EventuallyEvent> {
        let mut vec = Vec::new();
        self.push_stopped_inhabiting(game, stopped_inhabiting, &mut vec);
        vec
    }

    fn push_stopped_inhabiting(&self, game: &GameEvent, stopped_inhabiting: &Option<StoppedInhabiting>, children: &mut Vec<EventuallyEvent>) {
        if let Some(inh) = stopped_inhabiting {
            children.push(
                self.make_event_builder()
                    .for_game(&game)
                    .for_sub_event(&inh.sub_event)
                    .category(1)
                    .r#type(EventType::RemovedMod)
                    .description(format!("{} stopped Inhabiting.", inh.inhabiting_player_name))
                    .team_tags(vec![])
                    .player_tags(vec![inh.inhabiting_player_id])
                    .metadata(EventMetadataBuilder::default()
                        .play(game.play)
                        .sub_play(0) // not sure if this is hardcoded
                        .other(json!({
                                "mod": "INHABITING",
                                "type": 0, // ?
                                "parent": self.id
                            }))
                        .build()
                        .unwrap()
                    )
                    .build()
                    .unwrap()
            )
        }
    }

    fn get_score_data(&self, game: &GameEvent, scores: &ScoreInfo, score_text: &str) -> (String, bool, Vec<EventuallyEvent>) {
        let score_text = scores.to_description(score_text);
        let has_any_refills = !scores.free_refills.is_empty();
        let children: Vec<_> = scores.free_refills.iter()
            .map(|free_refill| self.make_free_refill_child(game, free_refill))
            .collect();
        (score_text, has_any_refills, children)
    }

    fn make_event_builder(&self) -> EventuallyEventBuilder {
        EventuallyEventBuilder::default()
            .id(self.id)
            .created(self.created)
            // TODO What is blurb?
            .blurb("".to_string())
            .sim(self.sim.clone())
            .day(self.day)
            .phase(self.phase)
            .season(self.season)
            .tournament(self.tournament)
            .nuts(self.nuts)
    }

    fn make_free_refill_child(&self, game: &GameEvent, free_refill: &FreeRefill) -> EventuallyEvent {
        self.make_event_builder()
            .for_game(&game)
            .for_sub_event(&free_refill.sub_event)
            .category(1)
            .r#type(EventType::RemovedMod)
            .description(format!("{} used their Free Refill.", free_refill.player_name))
            .team_tags(vec![free_refill.team_id])
            .player_tags(vec![free_refill.player_id])
            .metadata(EventMetadataBuilder::default()
                .play(game.play)
                .sub_play(0) // not sure if this is hardcoded
                .other(json!({
                                "mod": "COFFEE_RALLY",
                                "type": 0, // ?
                                "parent": self.id
                            }))
                .build()
                .unwrap()
            )
            .build()
            .unwrap()
    }
}


fn make_game_event_metadata_builder(game: &GameEvent) -> EventMetadataBuilder {
    EventMetadataBuilder::default()
        .play(game.play)
        .sub_play(game.sub_play)
}

fn make_game_event_metadata(game: &GameEvent) -> EventMetadata {
    make_game_event_metadata_builder(game)
        .build()
        .unwrap()
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
