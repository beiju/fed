use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::json;
use uuid::Uuid;
use fed_api::{EventMetadataBuilder, EventType, EventuallyEvent, EventuallyEventBuilder, Weather};
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

// Some effects in the game (e.g. Superyummy, Homebody) mean a player always has either
// Overperforming or Underperforming. Those players always have a AddedModFromOtherMod (146) event
// the first time their status changes and a ChangedModFromOtherMod (148) thereafter.
#[derive(Debug, Clone)]
pub enum PermaPerformingChange {
    Added(bool),
    Changed(bool),
}

impl Into<bool> for PermaPerformingChange {
    fn into(self) -> bool {
        match self {
            PermaPerformingChange::Added(p) => { p }
            PermaPerformingChange::Changed(p) => { p }
        }
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
    },

    SuperyummyGameStart {
        game: GameEvent,
        player_name: String,
        change: PermaPerformingChange,
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
                event_builder
                    .r#type(EventType::LetsGo)
                    .category(0)
                    .description("Let's Go!".to_string())
                    .metadata(
                        EventMetadataBuilder::default()
                            .other(json!({
                                "home": game.home_team,
                                "away": game.away_team,
                                "weather": weather_id,
                            }))
                            .build()
                            .unwrap())
            }
            FedEventData::PlayBall { game } => {
                event_builder
                    .r#type(EventType::PlayBall)
                    .category(0)
                    .description("Play ball!".to_string())
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name } => {
                event_builder
                    .r#type(EventType::HalfInning)
                    .category(0)
                    .description(format!("{} of {}, {} batting.",
                                         if top_of_inning { "Top" } else { "Bottom" },
                                         inning,
                                         batting_team_name))
            }
            FedEventData::BatterUp { game, batter_name, team_name } => {
                event_builder
                    .r#type(EventType::BatterUp)
                    .category(0)
                    .description(format!("{} batting for the {}.", batter_name, team_name))
            }
            FedEventData::SuperyummyGameStart { game, player_name, change } => {
                event_builder
                    .r#type(EventType::Superyummy)
                    .category(2)
                    .description(format!("{} {} Peanuts.", player_name,
                                         if change.into() { "loves" } else { "misses" }))

                // let mod_event =
                //
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

fn populate_game_event(event: &mut EventuallyEvent, game: &GameEvent) {
    event.game_tags.push(game.game_id);
    event.team_tags.push(game.away_team);
    event.team_tags.push(game.home_team);
    event.metadata.play = Some(game.play);
    event.metadata.sub_play = Some(game.sub_play);
}