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
    pub team_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct Score {
    pub player_id: Uuid,
    pub player_name: String,
    pub free_refill: Option<FreeRefill>,
}

impl Score {
    pub fn to_description(&self, score_text: &str) -> String {
        let refill_text = if self.free_refill.is_some() {
            format!("\n{} used their Free Refill.\n{} Refills the In!", self.player_name, self.player_name)
        } else {
            String::new()
        };
        format!("\n{} {}!{}", self.player_name, score_text, refill_text)
    }
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
        scores: Vec<Score>,
    },

    GroundOut {
        game: GameEvent,
        batter_name: String,
        fielder_name: String,
    },

    Hit {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
        num_bases: i32,
        scores: Vec<Score>,
    },

    HomeRun {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
        num_runs: i32,
    },

    StolenBase {
        game: GameEvent,
        runner_name: String,
        runner_id: Uuid,
        base_stolen: i32,
    },

    CaughtStealing {
        game: GameEvent,
        runner_name: String,
        base_stolen: i32,
    },

    StrikeoutSwinging {
        game: GameEvent,
        batter_name: String,
    },

    StrikeoutLooking {
        game: GameEvent,
        batter_name: String,
    },

    Walk {
        game: GameEvent,
        batter_name: String,
        batter_id: Uuid,
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
            FedEventData::BatterUp { game, batter_name, team_name, wielding_item: wielding_item_name } => {
                let item_suffix = if let Some(item_name) = wielding_item_name {
                    format!(", wielding {}", item_name)
                } else {
                    String::default()
                };
                event_builder.for_game(&game)
                    .r#type(EventType::BatterUp)
                    .description(format!("{} batting for the {}{}.", batter_name, team_name, item_suffix))
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
            FedEventData::Flyout { game, batter_name, fielder_name, scores } => {
                let score_text = scores.iter()
                    .map(|score| score.to_description("tags up and scores"))
                    // the \n is in each element since it needs to be before the first element too
                    .join("");
                event_builder.for_game(&game)
                    .r#type(EventType::FlyOut)
                    .description(format!("{} hit a flyout to {}.{}", batter_name, fielder_name, score_text))
                    .player_tags(scores.iter().map(|score| score.player_id).collect())
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::Hit { ref game, ref batter_name, batter_id, num_bases, ref scores } => {
                let score_text = scores.iter()
                    .map(|score| score.to_description("scores"))
                    // the \n is in each element since it needs to be before the first element too
                    .join("");
                let has_any_refills = scores.iter().any(|score| score.free_refill.is_some());
                let children: Vec<_> = scores.iter()
                    .filter_map(|score| {
                        score.free_refill.as_ref().map(|free_refill| {
                            self.make_event_builder()
                                .for_game(&game)
                                .for_sub_event(&free_refill.sub_event)
                                .category(1)
                                .r#type(EventType::RemovedMod)
                                .description(format!("{} used their Free Refill.", score.player_name))
                                .team_tags(vec![free_refill.team_id])
                                .player_tags(vec![score.player_id])
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
                        })
                    })
                    .collect();

                event_builder.for_game(&game)
                    .r#type(EventType::Hit)
                    .category(if has_any_refills { 2 } else { 0 })
                    .description(format!("{} hits a {}!{}", batter_name, match num_bases {
                        1 => "Single",
                        2 => "Double",
                        3 => "Triple",
                        4 => "Quadruple",
                        // TODO Turn this into a Result error
                        _ => panic!("Unknown hit type")
                    }, score_text))
                    .player_tags(iter::once(batter_id).chain(scores.iter().map(|score| score.player_id)).collect())
                    .metadata(make_game_event_metadata_builder(&game)
                        .children(children)
                        .build()
                        .unwrap())
            }
            FedEventData::HomeRun { game, batter_name, batter_id, num_runs } => {
                event_builder.for_game(&game)
                    .r#type(EventType::HomeRun)
                    .description(format!("{} hits a {}!", batter_name, match num_runs {
                        1 => "solo home run",
                        2 => "2-run home run",
                        3 => "3-run home run",
                        4 => "grand slam",
                        // TODO Turn this into a Result error
                        _ => panic!("Unknown num runs in home run")
                    }))
                    .player_tags(vec![batter_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::GroundOut { game, batter_name, fielder_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::GroundOut)
                    .description(format!("{} hit a ground out to {}.", batter_name, fielder_name))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StolenBase { game, runner_name, runner_id, base_stolen } => {
                event_builder.for_game(&game)
                    .r#type(EventType::StolenBase)
                    .description(format!("{} steals {} base!", runner_name, match base_stolen {
                        2 => "second",
                        3 => "third",
                        4 => "fourth",
                        5 => "fifth",
                        _ => panic!("What base is this")
                    }))
                    .player_tags(vec![runner_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeoutSwinging { game, batter_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strikeout)
                    .description(format!("{} strikes out swinging.", batter_name))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::StrikeoutLooking { game, batter_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Strikeout)
                    .description(format!("{} strikes out looking.", batter_name))
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::Walk { game, batter_name, batter_id } => {
                event_builder.for_game(&game)
                    .r#type(EventType::Walk)
                    .description(format!("{} draws a walk.", batter_name))
                    .player_tags(vec![batter_id])
                    .metadata(make_game_event_metadata_builder(&game)
                        .build()
                        .unwrap())
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen } => {
                event_builder.for_game(&game)
                    .r#type(EventType::StolenBase)
                    .description(format!("{} gets caught stealing {} base.", runner_name, match base_stolen {
                        2 => "second",
                        3 => "third",
                        4 => "fourth",
                        5 => "fifth",
                        _ => panic!("What base is this")
                    }))
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
        }
            .build()
            .unwrap()
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
