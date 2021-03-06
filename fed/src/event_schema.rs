use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::json;
use uuid::Uuid;
use fed_api::{EventType, EventuallyEvent, Weather};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use crate::error::FeedParseError;

#[derive(Debug, IntoPrimitive, TryFromPrimitive)]
#[repr(i32)]
pub enum Being {
    EmergencyAlert = -1,
    TheShelledOne =  0,
    TheMonitor =  1,
    TheCoin =  2,
    TheReader =  3,
    TheMicrophone =  4,
    Lootcrates =  5,
    Namerifeht =  6,
}

#[derive(Debug)]
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
                })?
        })
    }
}

#[derive(Debug)]
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
}

#[derive(Debug)]
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
        let mut event = EventuallyEvent {
            id: self.id,
            created: self.created,
            r#type: EventType::Undefined,
            category: 0,
            metadata: Default::default(),
            blurb: "".to_string(),
            description: "".to_string(),
            player_tags: vec![],
            game_tags: vec![],
            team_tags: vec![],
            sim: self.sim,
            day: self.day,
            season: self.season,
            tournament: self.tournament,
            phase: self.phase,
            nuts: self.nuts,

        };

        match self.data {
            FedEventData::BeingSpeech { being, message } => {
                event.r#type = EventType::BigDeal;
                event.category = 4;
                event.description = message;
                let being_id: i32 = being.into();
                event.metadata.other = json!({
                    "being": being_id
                });
            }
            FedEventData::LetsGo { game, weather } => {
                populate_game_event(&mut event, &game);
                event.r#type = EventType::LetsGo;
                event.description = "Let's Go!".to_string();
                let weather_id: i32 = weather.into();
                event.metadata.other = json!({
                    "home": game.home_team,
                    "away": game.away_team,
                    "weather": weather_id,
                });
            }
            FedEventData::PlayBall { game } => {
                populate_game_event(&mut event, &game);
                event.r#type = EventType::PlayBall;
                event.description = "Play ball!".to_string();
            }
        }


        event
    }
}

fn populate_game_event(event: &mut EventuallyEvent, game: &GameEvent) {
    event.game_tags.push(game.game_id);
    event.team_tags.push(game.away_team);
    event.team_tags.push(game.home_team);
    event.metadata.play = Some(game.play);
    event.metadata.sub_play = Some(game.sub_play);
}