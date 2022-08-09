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
        peanuts: bool,
        is_first_proc: bool,
        sub_event: SubEvent,
        player_id: Uuid,
        team_id: Uuid,
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
            FedEventData::BatterUp { game, batter_name, team_name } => {
                event_builder.for_game(&game)
                    .r#type(EventType::BatterUp)
                    .description(format!("{} batting for the {}.", batter_name, team_name))
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
