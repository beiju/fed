use chrono::{DateTime, Utc};
use nom::{Finish, Parser};
use nom::error::convert_error;
use uuid::Uuid;
use eventually_api::{EventType, EventuallyEvent};
use crate::{FedEvent, FedEventData, FeedParseError, GameEvent, PlayerInfo, SimPhase, SubEvent, Unscatter};
use crate::parse::parsers::ParserResult;

#[derive(Debug, Copy, Clone)]
pub struct EventParseWrapper<'e> {
    pub event_type: EventType,
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: &'e str,
    pub tournament: i32,
    pub season: i32,
    pub day: i32,
    pub phase: SimPhase,
    pub nuts: i32,
    pub play: Option<i64>,

    // Managed specially
    description: &'e str,
    metadata: &'e serde_json::Value,

    consumed_player_id_count: usize,
    player_ids: &'e [Uuid],
    consumed_team_id_count: usize,
    team_ids: &'e [Uuid],
    consumed_game_id_count: usize,
    game_ids: &'e [Uuid],

    consumed_children_count: usize,
    children: &'e [EventuallyEvent],
}

impl<'e> EventParseWrapper<'e> {
    pub fn new(event: &'e EventuallyEvent) -> Result<Self, FeedParseError> {
        Ok(Self {
            event_type: event.r#type,
            id: event.id,
            created: event.created,
            sim: &event.sim,
            tournament: event.tournament,
            season: event.season,
            day: event.day,
            phase: event.phase.try_into()
                .map_err(|_| FeedParseError::UnknownPhase {
                    phase: event.phase,
                    event_type: event.r#type,
                })?,
            nuts: event.nuts,
            play: event.metadata.play,
            description: &event.description,
            metadata: &event.metadata.other,
            consumed_player_id_count: 0,
            player_ids: event.player_tags.as_slice(),
            consumed_team_id_count: 0,
            team_ids: event.team_tags.as_slice(),
            consumed_game_id_count: 0,
            game_ids: event.game_tags.as_slice(),
            consumed_children_count: 0,
            children: event.metadata.children.as_slice(),
        })
    }

    pub fn consume_description(&mut self) -> &'e str {
        let d = self.description;
        self.description = "";
        d
    }

    pub fn next_parse<F, Out>(&mut self, mut parser: F) -> Result<Out, FeedParseError>
        where F: Fn(&'e str) -> ParserResult<'e, Out> {
        let (rest, result) = parser.parse(&self.description)
            .finish()
            .map_err(|e| {
                FeedParseError::DescriptionParseError {
                    event_type: self.event_type,
                    err: convert_error(self.description, e),
                }
            })?;
        self.description = rest;
        Ok(result)
    }

    pub fn next_parse_tag(&mut self, tag: &str) -> Result<&str, FeedParseError> {
        self.next_parse(nom::bytes::complete::tag(tag))
    }

    // This could delegate to next_parse but I chose not to because that means that a breakpoint on
    // the map_err in next_parse will only be hit on actual errors
    pub fn next_parse_opt<F, Out>(&mut self, mut parser: F) -> Option<Out>
        where F: Fn(&'e str) -> ParserResult<'e, Out> {
        let (rest, result) = parser.parse(&self.description).ok()?;

        self.description = rest;
        Some(result)
    }

    pub fn next_player_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_player_id_count += 1;
        let (&id, rest) = self.player_ids.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "player",
                    expected_at_least: self.consumed_player_id_count,
                }
            })?;
        self.player_ids = rest;
        Ok(id)
    }

    pub fn next_team_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_team_id_count += 1;
        let (&id, rest) = self.team_ids.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "team",
                    expected_at_least: self.consumed_team_id_count,
                }
            })?;
        self.team_ids = rest;
        Ok(id)
    }

    fn next_game_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_team_id_count += 1;
        let (&id, rest) = self.team_ids.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "team",
                    expected_at_least: self.consumed_team_id_count,
                }
            })?;
        self.team_ids = rest;
        Ok(id)
    }

    pub fn next_child(&mut self, expected_type: EventType) -> Result<Self, FeedParseError> {
        self.consumed_children_count += 1;
        let (child, rest) = self.children.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughChildren {
                    event_type: self.event_type,
                    expected_at_least: self.consumed_children_count,
                }
            })?;
        self.children = rest;
        if child.r#type != expected_type {
            return Err(FeedParseError::UnexpectedChildType {
                event_type: self.event_type,
                child_event_type: child.r#type,
                child_number: self.consumed_children_count,
            });
        }

        Self::new(child)
    }

    pub fn next_child_if<F>(&mut self, expected_type: EventType, pred: F) -> Result<Option<Self>, FeedParseError>
        where F: Fn(Self) -> bool {
        let Some((child, rest)) = self.children.split_first() else {
            return Ok(None);
        };

        let child = Self::new(child)?;
        if !pred(child) { return Ok(None); }

        self.consumed_children_count += 1;
        self.children = rest;

        if child.event_type != expected_type {
            return Err(FeedParseError::UnexpectedChildType {
                event_type: self.event_type,
                child_event_type: child.event_type,
                child_number: self.consumed_children_count,
            });
        }

        Ok(Some(child))
    }

    pub fn as_sub_event(&self) -> SubEvent {
        SubEvent {
            id: self.id,
            created: self.created,
            nuts: self.nuts,
        }
    }

    fn get_metadata(&self, key: &'static str) -> Result<&'e serde_json::Value, FeedParseError> {
        self.metadata
            .as_object()
            .ok_or_else(|| {
                FeedParseError::MetadataWasNotAnObject {
                    event_type: self.event_type
                }
            })?
            .get(key)
            .ok_or_else(|| {
                FeedParseError::MissingMetadata {
                    event_type: self.event_type,
                    field: key,
                }
            })
    }

    pub fn metadata_i64(&self, key: &'static str) -> Result<i64, FeedParseError> {
        self.get_metadata(key)?
            .as_i64()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key,
                    ty: "i64",
                }
            })
    }

    pub fn metadata_f64(&self, key: &'static str) -> Result<f64, FeedParseError> {
        self.get_metadata(key)?
            .as_f64()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key,
                    ty: "f64",
                }
            })
    }

    pub fn metadata_str(&self, key: &'static str) -> Result<&'e str, FeedParseError> {
        self.get_metadata(key)?
            .as_str()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key,
                    ty: "str",
                }
            })
    }

    pub fn metadata_uuid(&self, key: &'static str) -> Result<Uuid, FeedParseError> {
        self.get_metadata(key)?
            .as_str()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key,
                    ty: "str",
                }
            })?
            .try_into()
            .map_err(|err| {
                FeedParseError::MetadataStrToUuidError {
                    event_type: self.event_type,
                    field: key,
                    err,
                }
            })
    }

    pub fn game(&mut self, unscatter: Option<Unscatter>, attractor_secret_base: Option<PlayerInfo>) -> Result<GameEvent, FeedParseError> {
        let game_id = self.next_game_id()?;

        // Order is very important here
        let away_team = self.next_team_id()?;
        let home_team = self.next_team_id()?;

        Ok(GameEvent {
            game_id,
            home_team,
            away_team,
            play: self.play
                .ok_or_else(|| {
                    FeedParseError::MissingMetadata {
                        event_type: self.event_type,
                        field: "play",
                    }
                })?,
            unscatter,
            attractor_secret_base,
        })
    }

    pub fn to_fed(&self, data: FedEventData) -> Result<FedEvent, FeedParseError> {
        if !self.description.is_empty() {
            return Err(FeedParseError::DescriptionNotFullyParsed {
                event_type: self.event_type,
                remaining: self.description.to_string(),
            });
        }
        if !self.player_ids.is_empty() {
            return Err(FeedParseError::TooManyTags {
                event_type: self.event_type,
                tag_type: "player",
                expected: self.consumed_player_id_count,
            });
        }
        if !self.team_ids.is_empty() {
            return Err(FeedParseError::TooManyTags {
                event_type: self.event_type,
                tag_type: "team",
                expected: self.consumed_team_id_count,
            });
        }
        if !self.children.is_empty() {
            return Err(FeedParseError::TooManyChildren {
                event_type: self.event_type,
                expected: self.consumed_children_count,
            });
        }
        Ok(FedEvent {
            id: self.id,
            created: self.created,
            sim: self.sim.to_string(),
            tournament: self.tournament,
            season: self.season,
            day: self.day,
            phase: self.phase,
            nuts: self.nuts,
            data,
        })
    }
}