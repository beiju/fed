use std::fmt::Display;
use chrono::{DateTime, Utc};
use nom::{Finish, Parser};
use nom::error::convert_error;
use uuid::Uuid;
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use crate::{BatterDebt, FedEvent, FedEventData, FeedParseError, FreeRefill, GameEvent, ModChangeSubEvent, ModChangeSubEventWithPlayer, PlayerInfo, Scores, ScoringPlayer, SimPhase, SpicyStatus, StoppedInhabiting, SubEvent, Unscatter};
use crate::parse::ParseOk;
use crate::parse::parsers::{parse_batter_debt, parse_cooled_off, parse_free_refills, parse_scores, parse_spicy_status, parse_stopped_inhabiting, ParsedSpicyStatus, ParserError, ParserResult};

#[derive(Debug, Copy, Clone)]
pub struct EventParseWrapper<'e> {
    pub event_type: EventType,
    pub category: EventCategory,
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
    metadata: &'e EventMetadata,

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
            category: event.category,
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
            metadata: &event.metadata,
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
        where F: Parser<&'e str, Out, ParserError<'e>> {
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

    pub fn peek_player_id(&self) -> Option<Uuid> {
        self.player_ids.first().copied()
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

    pub fn next_team_id_opt(&mut self) -> Option<Uuid> {
        let (&id, rest) = self.team_ids.split_first()?;
        self.consumed_team_id_count += 1;
        self.team_ids = rest;
        Some(id)
    }

    pub fn next_player_id_opt(&mut self) -> Option<Uuid> {
        let (&id, rest) = self.player_ids.split_first()?;
        self.consumed_player_id_count += 1;
        self.player_ids = rest;
        Some(id)
    }

    fn next_game_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_game_id_count += 1;
        let (&id, rest) = self.game_ids.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "game",
                    expected_at_least: self.consumed_game_id_count,
                }
            })?;
        self.game_ids = rest;
        Ok(id)
    }

    pub fn next_child(&mut self, expected_type: EventType) -> Result<Self, FeedParseError> {
        self.next_child_any(&[expected_type])
    }

    pub fn next_child_any(&mut self, expected_types: &[EventType]) -> Result<Self, FeedParseError> {
        let (child, rest) = self.children.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughChildren {
                    event_type: self.event_type,
                    expected_at_least: self.consumed_children_count + 1,
                }
            })?;
        if !expected_types.iter().any(|&t| child.r#type == t) {
            return Err(FeedParseError::UnexpectedChildType {
                event_type: self.event_type,
                child_event_type: child.r#type,
                child_number: self.consumed_children_count,
            });
        }

        self.consumed_children_count += 1;
        self.children = rest;

        Self::new(child)
    }

    pub fn next_child_opt(&mut self, expected_type: EventType) -> Result<Option<Self>, FeedParseError> {
        self.next_child_any_opt(&[expected_type])
    }

    pub fn next_child_any_opt(&mut self, expected_types: &[EventType]) -> Result<Option<Self>, FeedParseError> {
        let Some((child, rest)) = self.children.split_first() else {
            return Ok(None)
        };
        if !expected_types.iter().any(|&t| child.r#type == t) {
            return Ok(None)
        }
        self.consumed_children_count += 1;
        self.children = rest;

        Self::new(child).map(Some)
    }

    pub fn next_child_if<F>(&mut self, expected_type: EventType, pred: F) -> Result<Option<Self>, FeedParseError>
        where F: Fn(Self) -> bool {
        self.next_child_if_any(&[expected_type], pred)
    }

    pub fn next_child_if_mod_effect(&mut self, expected_type: EventType, expected_mod: &str) -> Result<Option<Self>, FeedParseError> {
        self.next_child_if_any_mod_effect(&[expected_type], expected_mod)
    }

    pub fn next_child_if_any_mod_effect(&mut self, expected_types: &[EventType], expected_mod: &str) -> Result<Option<Self>, FeedParseError> {
        self.next_child_if_any(expected_types, |child| {
            expected_types.iter().any(|t| t == &child.event_type) &&
                child.metadata_str("mod").map_or(false, |m| {
                    m == expected_mod
                })
        })
    }

    pub fn next_child_if_mod_effect_and<F>(&mut self, expected_type: EventType, expected_mod: &str, pred: F) -> Result<Option<Self>, FeedParseError>
        where F: Fn(Self) -> bool {
        self.next_child_if_any_mod_effect_and(&[expected_type], expected_mod, pred)
    }

    pub fn next_child_if_any_mod_effect_and<F>(&mut self, expected_types: &[EventType], expected_mod: &str, pred: F) -> Result<Option<Self>, FeedParseError>
        where F: Fn(Self) -> bool {
        self.next_child_if_any(expected_types, |child| {
            expected_types.iter().any(|t| t == &child.event_type) &&
                child.metadata_str("mod").map_or(false, |m| m == expected_mod) &&
                pred(child)
        })
    }

    pub fn next_child_if_any<F>(&mut self, expected_types: &[EventType], pred: F) -> Result<Option<Self>, FeedParseError>
        where F: Fn(Self) -> bool {
        let Some((child, rest)) = self.children.split_first() else {
            return Ok(None);
        };

        let child = Self::new(child)?;
        if !pred(child) { return Ok(None); }

        if !expected_types.iter().any(|t| t == &child.event_type) {
            return Err(FeedParseError::UnexpectedChildType {
                event_type: self.event_type,
                child_event_type: child.event_type,
                child_number: self.consumed_children_count,
            });
        }

        self.consumed_children_count += 1;
        self.children = rest;

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
        self.metadata.other
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

    pub fn metadata_str_vec(&self, key: &'static str) -> Result<Vec<&'e str>, FeedParseError> {
        self.get_metadata(key)?
            .as_array()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key,
                    ty: "array",
                }
            })
            .and_then(|vec| {
                vec.iter()
                    .map(|item| {
                        item.as_str()
                            .ok_or_else(|| {
                                FeedParseError::MetadataTypeError {
                                    event_type: self.event_type,
                                    field: key,
                                    ty: "array[str]",
                                }
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
    }

    pub fn metadata_uuid(&self, key: &'static str) -> Result<Uuid, FeedParseError> {
        self.metadata_str(key)?
            .try_into()
            .map_err(|err| {
                FeedParseError::MetadataStrToUuidError {
                    event_type: self.event_type,
                    field: key,
                    err,
                }
            })
    }

    pub fn metadata_enum<T>(&self, key: &'static str) -> Result<T, FeedParseError>
        where i64: TryInto<T>, <i64 as TryInto<T>>::Error: Display {
        self.metadata_i64(key)?
            .try_into()
            .map_err(|err| {
                FeedParseError::MetadataIntToEnumError {
                    event_type: self.event_type,
                    field: key,
                    err: err.to_string(),
                }
            })
    }

    pub fn description(&self) -> &'e str {
        self.description
    }

    pub fn metadata(&self) -> &'e serde_json::Value {
        &self.metadata.other
    }

    pub fn full_metadata(&self) -> &'e EventMetadata {
        self.metadata
    }

    pub fn player_tags(&self) -> &'e [Uuid] {
        self.player_ids
    }

    pub fn team_tags(&self) -> &'e [Uuid] {
        self.team_ids
    }

    pub fn parse_spicy_status(&mut self, batter_name: &str) -> Result<SpicyStatus, FeedParseError> {
        Ok(match self.next_parse(parse_spicy_status(batter_name))? {
            ParsedSpicyStatus::None => { SpicyStatus::None }
            ParsedSpicyStatus::HeatingUp => { SpicyStatus::HeatingUp }
            ParsedSpicyStatus::RedHot => {
                let child = self.next_child_if_mod_effect(EventType::AddedMod, "ON_FIRE")?
                    .map(|mut spicy_event| {
                        ParseOk(ModChangeSubEvent {
                            sub_event: spicy_event.as_sub_event(),
                            team_id: spicy_event.next_team_id()?,
                        })
                    })
                    .transpose()?;
                SpicyStatus::RedHot(child)
            }
        })
    }

    pub fn parse_cooled_off(&mut self, batter_name: &str) -> Result<Option<ModChangeSubEventWithPlayer>, FeedParseError> {
        Ok(match self.next_parse(parse_cooled_off(batter_name))? {
            false => { None }
            true => {
                let mut cooled_off_event = self.next_child(EventType::RemovedMod)?;

                Some(ModChangeSubEventWithPlayer {
                    sub_event: cooled_off_event.as_sub_event(),
                    team_id: cooled_off_event.next_team_id()?,
                    player_id: cooled_off_event.next_player_id()?,
                })
            }
        })
    }

    pub fn parse_free_refills(&mut self) -> Result<Vec<FreeRefill>, FeedParseError> {
        self.next_parse(parse_free_refills)?.into_iter()
            .map(|name| {
                let mut child = self.next_child(EventType::RemovedMod)?;
                ParseOk(FreeRefill {
                    sub_event: child.as_sub_event(),
                    player_name: name.to_string(),
                    player_id: child.next_player_id()?,
                    team_id: child.next_team_id()?,
                })
            })
            .collect()
    }

    pub fn parse_batter_debt(&mut self, batter_name: &str, fielder_name: &str) -> Result<Option<BatterDebt>, FeedParseError> {
        self.next_parse_opt(parse_batter_debt(batter_name, fielder_name))
            .map(|()| {
                let sub_event = self.next_child_if_mod_effect(EventType::AddedMod, "COFFEE_PERIL")?
                    .map(|mut child| {
                        ParseOk(ModChangeSubEvent {
                            team_id: child.next_team_id()?,
                            sub_event: child.as_sub_event(),
                        })
                    })
                    .transpose()?;

                ParseOk(BatterDebt {
                    batter_id: self.next_player_id()?,
                    fielder_id: self.next_player_id()?,
                    sub_event,
                })
            })
            .transpose()
    }

    pub fn parse_stopped_inhabiting(&mut self, player_id: Option<Uuid>) -> Result<Option<StoppedInhabiting>, FeedParseError> {
        self
            .next_child_if_mod_effect_and(EventType::RemovedMod, "INHABITING", |child| {
                player_id.is_none() || child.peek_player_id() == player_id
            })?
            .map(|mut child| {
                let name = child.next_parse(parse_stopped_inhabiting)?;
                ParseOk(StoppedInhabiting {
                    sub_event: child.as_sub_event(),
                    inhabiting_player_name: name.to_string(),
                    inhabiting_player_id: child.next_player_id()?,
                    inhabiting_player_team_id: child.next_team_id_opt(),
                })
            })
            .transpose()
    }

    pub fn parse_scores(&mut self, label: &'static str) -> Result<Scores, FeedParseError> {
        let scoring_players = self.parse_scoring_players(label)?;
        self.parse_scores_with_scoring_players(scoring_players)
    }

    pub fn parse_scores_with_scoring_players(&mut self, scoring_players: Vec<(Uuid, String)>) -> Result<Scores, FeedParseError> {
        let free_refills = self.parse_free_refills()?;

        let scores = scoring_players.into_iter()
            .map(|(player_id, player_name)| {
                ParseOk(ScoringPlayer {
                    player_id,
                    player_name,
                })
            })
            .collect::<Result<_, _>>()?;

        Ok(Scores {
            scores,
            free_refills,
        })
    }

    pub fn parse_scoring_players(&mut self, label: &'static str) -> Result<Vec<(Uuid, String)>, FeedParseError> {
        let scorers = self.next_parse(parse_scores(label))?;
        let scoring_players = scorers.into_iter()
            .map(|scorer| {
                ParseOk((self.next_player_id()?, scorer.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(scoring_players)
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
        // if !self.description.is_empty() {
        //     return Err(FeedParseError::DescriptionNotFullyParsed {
        //         event_type: self.event_type,
        //         remaining: self.description.to_string(),
        //     });
        // }
        // if !self.player_ids.is_empty() {
        //     return Err(FeedParseError::TooManyTags {
        //         event_type: self.event_type,
        //         tag_type: "player",
        //         expected: self.consumed_player_id_count,
        //     });
        // }
        // if !self.team_ids.is_empty() {
        //     return Err(FeedParseError::TooManyTags {
        //         event_type: self.event_type,
        //         tag_type: "team",
        //         expected: self.consumed_team_id_count,
        //     });
        // }
        // if !self.children.is_empty() {
        //     return Err(FeedParseError::TooManyChildren {
        //         event_type: self.event_type,
        //         expected: self.consumed_children_count,
        //     });
        // }
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