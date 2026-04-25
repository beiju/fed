mod ledger_parsers;

use crate::FeedParseError;
use crate::fed_event::*;
use crate::parse::parse_wrapper::ledger_parsers::ParseableLedger;
use crate::parse::parsers::*;
use crate::parse::{InterEventState, ParseOk, is_known_team_nickname};
use chrono::{DateTime, Utc};
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use nom::bytes::complete::tag;
use nom::combinator::opt;
use nom::{Finish, Parser};
use nom_language::error::{VerboseError, convert_error};
use std::fmt::Display;
use uuid::Uuid;

#[derive(Debug, Copy, Clone)]
pub struct EventParseWrapper<'e> {
    pub event_type: EventType,
    pub category: EventCategory,
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: &'e str,
    pub tournament: i64,
    pub season: i64,
    pub day: i64,
    pub phase: SimPhase,
    pub nuts: i64,
    pub play: Option<i64>,

    // Managed specially
    description: &'e str,
    metadata: &'e EventMetadata,

    consumed_player_id_count: usize,
    player_ids: Option<&'e [Uuid]>,
    consumed_team_id_count: usize,
    team_ids: Option<&'e [Uuid]>,
    consumed_game_id_count: usize,
    game_ids: Option<&'e [Uuid]>,

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
            phase: event
                .phase
                .try_into()
                .map_err(|_| FeedParseError::UnknownPhase {
                    phase: event.phase,
                    event_type: event.r#type,
                })?,
            nuts: event.nuts,
            play: event.metadata.play,
            description: &event.description,
            metadata: &event.metadata,
            consumed_player_id_count: 0,
            player_ids: event.player_tags.as_ref().map(|v| v.as_slice()),
            consumed_team_id_count: 0,
            team_ids: event.team_tags.as_ref().map(|v| v.as_slice()),
            consumed_game_id_count: 0,
            game_ids: event.game_tags.as_ref().map(|v| v.as_slice()),
            consumed_children_count: 0,
            children: event.metadata.children.as_slice(),
        })
    }

    pub fn is_post_semicentennial(&self) -> bool {
        self.season > 22 || (self.season == 22 && self.day == 116)
    }

    pub fn consume_description(&mut self) -> &'e str {
        let d = self.description;
        self.description = "";
        d
    }

    pub fn next_parse<F, Out>(&mut self, mut parser: F) -> Result<Out, FeedParseError>
    where
        F: Parser<&'e str, Output = Out, Error = ParserError<'e>>,
    {
        let (rest, result) = parser.parse(&self.description).finish().map_err(|e| {
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
    where
        F: Fn(&'e str) -> ParserResult<'e, Out>,
    {
        let (rest, result) = parser.parse(&self.description).ok()?;

        self.description = rest;
        Some(result)
    }

    pub fn next_player_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_player_id_count += 1;
        let (&id, rest) = self
            .player_ids
            .ok_or_else(|| FeedParseError::MissingTags {
                event_type: self.event_type,
                tag_type: "player",
            })?
            .split_first()
            .ok_or_else(|| {
                // This is in a block to facilitate breakpoints
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "player",
                    expected_at_least: self.consumed_player_id_count,
                }
            })?;
        self.player_ids = Some(rest);
        Ok(id)
    }

    // I decided that the semantics of peek would be to error if the ids list is None. You could
    // argue that returning None would be better.
    pub fn peek_player_id(&self) -> Result<Option<Uuid>, FeedParseError> {
        Ok(self
            .player_ids
            .ok_or_else(|| FeedParseError::MissingTags {
                event_type: self.event_type,
                tag_type: "player",
            })?
            .first()
            .copied())
    }

    pub fn next_team_id(&mut self) -> Result<Uuid, FeedParseError> {
        let (&id, rest) = self
            .team_ids
            .ok_or_else(|| FeedParseError::MissingTags {
                event_type: self.event_type,
                tag_type: "team",
            })?
            .split_first()
            .ok_or_else(|| FeedParseError::NotEnoughTags {
                event_type: self.event_type,
                tag_type: "team",
                expected_at_least: self.consumed_team_id_count + 1,
            })?;
        self.consumed_team_id_count += 1;
        self.team_ids = Some(rest);
        Ok(id)
    }

    pub fn next_team_id_opt(&mut self) -> Option<Uuid> {
        if let Some((&id, rest)) = self.team_ids?.split_first() {
            self.consumed_team_id_count += 1;
            self.team_ids = Some(rest);
            Some(id)
        } else {
            None
        }
    }

    pub fn next_player_id_opt(&mut self) -> Option<Uuid> {
        if let Some((&id, rest)) = self.player_ids?.split_first() {
            self.consumed_player_id_count += 1;
            self.player_ids = Some(rest);
            Some(id)
        } else {
            None
        }
    }

    fn next_game_id(&mut self) -> Result<Uuid, FeedParseError> {
        self.consumed_game_id_count += 1;
        let (&id, rest) = self
            .game_ids
            .ok_or_else(|| FeedParseError::MissingTags {
                event_type: self.event_type,
                tag_type: "game",
            })?
            .split_first()
            .ok_or_else(|| FeedParseError::NotEnoughTags {
                event_type: self.event_type,
                tag_type: "game",
                expected_at_least: self.consumed_game_id_count,
            })?;
        self.game_ids = Some(rest);
        Ok(id)
    }

    pub fn has_more_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn next_child_any(&mut self, expected_types: &[EventType]) -> Result<Self, FeedParseError> {
        let (child, rest) =
            self.children
                .split_first()
                .ok_or_else(|| FeedParseError::NotEnoughChildren {
                    event_type: self.event_type,
                    expected_at_least: self.consumed_children_count + 1,
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

    pub fn next_child(&mut self, expected_type: EventType) -> Result<Self, FeedParseError> {
        self.next_child_any(&[expected_type])
    }

    pub fn next_child_opt(
        &mut self,
        expected_type: EventType,
    ) -> Result<Option<Self>, FeedParseError> {
        self.next_child_any_opt(&[expected_type])
    }

    pub fn next_child_any_opt(
        &mut self,
        expected_types: &[EventType],
    ) -> Result<Option<Self>, FeedParseError> {
        let Some((child, rest)) = self.children.split_first() else {
            return Ok(None);
        };
        if !expected_types.iter().any(|&t| child.r#type == t) {
            return Ok(None);
        }
        self.consumed_children_count += 1;
        self.children = rest;

        Self::new(child).map(Some)
    }

    pub fn next_child_if<F>(
        &mut self,
        expected_type: EventType,
        pred: F,
    ) -> Result<Option<Self>, FeedParseError>
    where
        F: Fn(Self) -> bool,
    {
        self.next_child_if_any(&[expected_type], pred)
    }

    pub fn next_child_if_mod_effect(
        &mut self,
        expected_type: EventType,
        expected_mod: &str,
    ) -> Result<Option<Self>, FeedParseError> {
        self.next_child_if_any_mod_effect(&[expected_type], expected_mod)
    }

    pub fn next_child_if_any_mod_effect(
        &mut self,
        expected_types: &[EventType],
        expected_mod: &str,
    ) -> Result<Option<Self>, FeedParseError> {
        self.next_child_if_any(expected_types, |child| {
            expected_types.iter().any(|t| t == &child.event_type)
                && child
                    .metadata_str("mod")
                    .map_or(false, |m| m == expected_mod)
        })
    }

    pub fn next_child_if_mod_effect_and<F>(
        &mut self,
        expected_type: EventType,
        expected_mod: &str,
        pred: F,
    ) -> Result<Option<Self>, FeedParseError>
    where
        F: Fn(Self) -> bool,
    {
        self.next_child_if_any_mod_effect_and(&[expected_type], expected_mod, pred)
    }

    pub fn next_child_if_any_mod_effect_and<F>(
        &mut self,
        expected_types: &[EventType],
        expected_mod: &str,
        pred: F,
    ) -> Result<Option<Self>, FeedParseError>
    where
        F: Fn(Self) -> bool,
    {
        self.next_child_if_any(expected_types, |child| {
            expected_types.iter().any(|t| t == &child.event_type)
                && child
                    .metadata_str("mod")
                    .map_or(false, |m| m == expected_mod)
                && pred(child)
        })
    }

    pub fn next_child_if_any<F>(
        &mut self,
        expected_types: &[EventType],
        pred: F,
    ) -> Result<Option<Self>, FeedParseError>
    where
        F: Fn(Self) -> bool,
    {
        let Some((child, rest)) = self.children.split_first() else {
            return Ok(None);
        };

        let child = Self::new(child)?;
        if !pred(child) {
            return Ok(None);
        }

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

    // I decided the API ergonomics are better if the result and option are transposed
    pub fn peek_child(&self) -> Result<Option<Self>, FeedParseError> {
        self.metadata.children.first().map(Self::new).transpose()
    }

    pub fn as_sub_event(&self) -> SubEvent {
        SubEvent {
            id: self.id,
            created: self.created,
            nuts: self.nuts,
        }
    }

    pub fn as_known_player_boost(&self) -> Result<KnownPlayerStatChange, FeedParseError> {
        Ok(KnownPlayerStatChange {
            rating_before: self.metadata_f64("before")?,
            rating_after: self.metadata_f64("after")?,
            sub_event: self.as_sub_event(),
        })
    }

    pub fn as_item_dropped(
        &self,
        item_was_broken: bool,
    ) -> Result<ItemDroppedForNewItem, FeedParseError> {
        Ok(ItemDroppedForNewItem {
            item_id: self.metadata_uuid("itemId")?,
            item_name: self.metadata_str("itemName")?.to_string(),
            item_mods: self
                .metadata_str_vec("mods")?
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            player_item_rating_before: self.metadata_f64_opt("playerItemRatingBefore")?,
            player_item_rating_after: self.metadata_f64("playerItemRatingAfter")?,
            item_was_broken,
            sub_event: self.as_sub_event(),
        })
    }

    pub fn get_metadata(&self, key: &'static str) -> Result<&'e serde_json::Value, FeedParseError> {
        self.metadata
            .other
            .as_object()
            .ok_or_else(|| FeedParseError::MetadataWasNotAnObject {
                event_type: self.event_type,
            })?
            .get(key)
            .ok_or_else(|| FeedParseError::MissingMetadata {
                event_type: self.event_type,
                field: key.to_string(),
            })
    }

    pub fn metadata_i64(&self, key: &'static str) -> Result<i64, FeedParseError> {
        self.get_metadata(key)?
            .as_i64()
            .ok_or_else(|| FeedParseError::MetadataTypeError {
                event_type: self.event_type,
                field: key.to_string(),
                ty: "i64",
            })
    }

    pub fn metadata_f64(&self, key: &'static str) -> Result<f64, FeedParseError> {
        self.get_metadata(key)?
            .as_f64()
            .ok_or_else(|| FeedParseError::MetadataTypeError {
                event_type: self.event_type,
                field: key.to_string(),
                ty: "f64",
            })
    }

    pub fn metadata_f64_opt(&self, key: &'static str) -> Result<Option<f64>, FeedParseError> {
        let value = self.get_metadata(key)?;
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_f64()
                .ok_or_else(|| FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key.to_string(),
                    ty: "f64",
                })
                .map(|n| Some(n))
        }
    }

    pub fn metadata_str(&self, key: &'static str) -> Result<&'e str, FeedParseError> {
        self.get_metadata(key)?
            .as_str()
            .ok_or_else(|| FeedParseError::MetadataTypeError {
                event_type: self.event_type,
                field: key.to_string(),
                ty: "str",
            })
    }

    pub fn metadata_str_vec(&self, key: &'static str) -> Result<Vec<&'e str>, FeedParseError> {
        self.get_metadata(key)?
            .as_array()
            .ok_or_else(|| FeedParseError::MetadataTypeError {
                event_type: self.event_type,
                field: key.to_string(),
                ty: "array",
            })
            .and_then(|vec| {
                vec.iter()
                    .enumerate()
                    .map(|(i, item)| {
                        item.as_str()
                            .ok_or_else(|| FeedParseError::MetadataTypeError {
                                event_type: self.event_type,
                                field: format!("{key}[{i}]"),
                                ty: "str",
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
    }

    pub fn metadata_uuid(&self, key: &'static str) -> Result<Uuid, FeedParseError> {
        self.metadata_str(key)?
            .try_into()
            .map_err(|err| FeedParseError::MetadataStrToUuidError {
                event_type: self.event_type,
                field: key.to_string(),
                err,
            })
    }

    pub fn metadata_uuid_vec(&self, key: &'static str) -> Result<Vec<Uuid>, FeedParseError> {
        self.get_metadata(key)?
            .as_array()
            .ok_or_else(|| FeedParseError::MetadataTypeError {
                event_type: self.event_type,
                field: key.to_string(),
                ty: "array",
            })
            .and_then(|vec| {
                vec.iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let item_str =
                            item.as_str()
                                .ok_or_else(|| FeedParseError::MetadataTypeError {
                                    event_type: self.event_type,
                                    field: format!("{key}[{i}]"),
                                    ty: "str",
                                })?;

                        Uuid::parse_str(item_str).map_err(|err| {
                            FeedParseError::MetadataStrToUuidError {
                                event_type: self.event_type,
                                field: format!("{key}[{i}]"),
                                err,
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
    }

    pub fn metadata_enum<T>(&self, key: &'static str) -> Result<T, FeedParseError>
    where
        i64: TryInto<T>,
        <i64 as TryInto<T>>::Error: Display,
    {
        self.metadata_i64(key)?
            .try_into()
            .map_err(|err| FeedParseError::MetadataIntToEnumError {
                event_type: self.event_type,
                field: key.to_string(),
                err: err.to_string(),
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

    pub fn player_tags(&self) -> Result<&'e [Uuid], FeedParseError> {
        self.player_ids.ok_or_else(|| FeedParseError::MissingTags {
            event_type: self.event_type,
            tag_type: "player",
        })
    }

    pub fn team_tags(&self) -> Result<&'e [Uuid], FeedParseError> {
        self.team_ids.ok_or_else(|| FeedParseError::MissingTags {
            event_type: self.event_type,
            tag_type: "team",
        })
    }

    pub fn parse_newline(&mut self) -> Result<(), FeedParseError> {
        self.next_parse(tag("\n"))?;
        Ok(())
    }

    pub fn parse_spicy_status(&mut self, batter_name: &str) -> Result<SpicyStatus, FeedParseError> {
        Ok(match self.next_parse(parse_spicy_status(batter_name))? {
            ParsedSpicyStatus::None => SpicyStatus::None,
            ParsedSpicyStatus::HeatingUp => SpicyStatus::HeatingUp,
            ParsedSpicyStatus::RedHot => {
                let child = self
                    .next_child_if_mod_effect(EventType::AddedMod, "ON_FIRE")?
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

    pub fn parse_cooled_off(
        &mut self,
        batter_name: &str,
    ) -> Result<Option<ModChangeSubEventWithPlayer>, FeedParseError> {
        Ok(match self.next_parse(parse_cooled_off(batter_name))? {
            false => None,
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
        self.next_parse(parse_free_refills)?
            .into_iter()
            .map(|name| self.build_free_refill(name))
            .collect()
    }

    // Use when only one free refill is allowed
    pub fn parse_free_refill(&mut self) -> Result<Option<FreeRefill>, FeedParseError> {
        self.next_parse(opt(parse_free_refill))?
            .map(|name| self.build_free_refill(name))
            .transpose()
    }

    pub fn parse_batter_debt(
        &mut self,
        batter_name: &str,
        fielder_name: &str,
    ) -> Result<Option<BatterDebt>, FeedParseError> {
        self.next_parse_opt(parse_batter_debt(batter_name, fielder_name))
            .map(|debt_type| {
                let sub_event = self
                    .next_child_if_mod_effect(EventType::AddedMod, debt_type.mod_id())?
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
                    debt_type,
                })
            })
            .transpose()
    }

    pub fn parse_stopped_inhabiting(
        &mut self,
        player_id: Option<Uuid>,
    ) -> Result<Option<StoppedInhabiting>, FeedParseError> {
        self.next_child_if_mod_effect_and(EventType::RemovedMod, "INHABITING", |child| {
            player_id.is_none() || child.peek_player_id().map_or(false, |id| id == player_id)
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

    pub fn parse_scores<LedgerT: LedgerV2 + ParseableLedger<Ledger = LedgerT>>(
        &mut self,
        label: &'static str,
    ) -> Result<Scores<LedgerT>, FeedParseError> {
        let (scoring_players, attractions) = self.parse_scoring_players(label)?;
        self.parse_scores_with_scoring_players(scoring_players, attractions)
    }

    pub fn parse_scores_without_summary<LedgerT: LedgerV2 + ParseableLedger<Ledger = LedgerT>>(
        &mut self,
        label: &'static str,
    ) -> Result<Scores<LedgerT>, FeedParseError> {
        let (scoring_players, attractions) = self.parse_scoring_players(label)?;
        self.parse_scores_with_scoring_players_without_summary(scoring_players, attractions, false)
    }

    pub fn parse_scores_with_scoring_players<
        LedgerT: LedgerV2 + ParseableLedger<Ledger = LedgerT>,
    >(
        &mut self,
        scoring_players: Vec<(
            Uuid,
            Option<(String, Option<bool>)>,
            String,
            Option<Option<String>>,
            bool,
            Option<Option<String>>,
        )>,
        attractions: Vec<(Uuid, String, String)>,
    ) -> Result<Scores<LedgerT>, FeedParseError> {
        let mut scores = self.parse_scores_with_scoring_players_without_summary(
            scoring_players,
            attractions,
            false,
        )?;
        scores.score_summary = self.parse_score_summary()?;
        scores.balloons = self.parse_balloons_from_score_summary(scores.score_summary.as_ref())?;
        Ok(scores)
    }

    // This is unfortunately not well type-encoded. This function always returns a Scores item with
    // .score_summary set to None, and subsequent functions may parse and set the score summary
    pub fn parse_scores_with_scoring_players_without_summary<
        LedgerT: LedgerV2 + ParseableLedger<Ledger = LedgerT>,
    >(
        &mut self,
        scoring_players: Vec<(
            Uuid,
            Option<(String, Option<bool>)>,
            String,
            Option<Option<String>>,
            bool,
            Option<Option<String>>,
        )>,
        attractions: Vec<(Uuid, String, String)>,
        is_fc: bool, // If this is an FC, we need to parse hotel motel parties here and ignore the input
    ) -> Result<Scores<LedgerT>, FeedParseError> {
        let scores = self.parse_base_scores(scoring_players, attractions, is_fc)?;

        let free_refills = self.parse_free_refills()?;

        Ok(Scores {
            scores,
            free_refills,
            // TODO Consider changing around the types to make these Nones unnecessary (see TODO
            //   comment on `Scores` struct
            score_summary: None, // Filled in by a later function
            balloons: None,      // Filled in by a later function
        })
    }

    fn parse_base_scores(
        &mut self,
        scoring_players: Vec<(
            Uuid,
            Option<(String, Option<bool>)>,
            String,
            Option<Option<String>>,
            bool,
            Option<Option<String>>,
        )>,
        attractions: Vec<(Uuid, String, String)>,
        is_fc: bool,
    ) -> Result<Vec<ScoringPlayer>, FeedParseError> {
        let mut attractions = attractions.into_iter().peekable();
        let scores: Vec<_> = scoring_players.into_iter()
            .map(|(player_id, item_name, player_name, hotel_motel_party, slippery, shame)| {
                let item_damage = item_name
                    .map(|(_name, plural)| self.next_item_damage(plural))
                    .transpose()?;
                // TODO Change back to a let chain
                let attraction = if let Some((attracted_player_id, _, _)) = attractions.peek() {
                    if attracted_player_id == &player_id {
                        let (_, attracted_team_nickname, attracted_player_name) = attractions.next()
                            .expect("This code should only run when there is a next item in the iterator");
                        assert!(is_known_team_nickname(&attracted_team_nickname));
                        // If these ever don't match that will be fun
                        assert_eq!(player_name, attracted_player_name);
                        let mut child = self.next_child(EventType::PlayerAddedToTeam)?;
                        let boost = self.next_child_opt(EventType::PlayerStatIncrease)?
                            .map(|child| {
                                ParseOk(PlayerBoostSubEvent {
                                    rating_before: child.metadata_f64("before")?,
                                    rating_after: child.metadata_f64("after")?,
                                    sub_event: child.as_sub_event(),
                                })
                            })
                            .transpose()?;
                        Some(Attraction {
                            team_nickname: attracted_team_nickname,
                            team_id: child.next_team_id()?,
                            sub_event: child.as_sub_event(),
                            boost,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                let hotel_motel_party = if is_fc {
                    self.next_parse_opt(parse_hotel_motel_party_with_name(&player_name))
                        .map(|o| o.map(str::to_string))
                } else {
                    hotel_motel_party
                };

                let hotel_motel_party = if let Some(birds) = hotel_motel_party {
                    Some(HotelMotelParty {
                        birds,
                        boost: self.next_boost_child_with_team()?,
                    })
                } else {
                    None
                };

                let shame = if self.season < 17 {
                    assert!(shame.is_none(), "My understanding is that the Shame message was introduced in s18");
                    Shame::Unknown
                } else {
                    self.parse_shame_from_parsed(shame)?
                };

                ParseOk(ScoringPlayer {
                    player_id,
                    player_name,
                    item_damage,
                    attraction,
                    hotel_motel_party,
                    is_slippery: slippery,
                    shame,
                })
            })
            .collect::<Result<_, _>>()?;

        // The above code should always drain the attractions iterator
        assert_eq!(attractions.peek(), None);
        Ok(scores)
    }

    pub fn parse_score_summary<LedgerT: LedgerV2 + ParseableLedger<Ledger = LedgerT>>(
        &mut self,
    ) -> Result<Option<ScoreSummary<LedgerT>>, FeedParseError> {
        let Some(mut score_child) = self.next_child_opt(EventType::RunsScored)? else {
            return Ok(None);
        };

        let team_nickname = score_child.next_parse(parse_team_scored)?;

        let score_update = score_child.metadata_str("update")?;
        let (_, runs_scored) = parse_score_update
            .parse(score_update)
            .finish()
            .map_err(|e| FeedParseError::ScoreUpdateParseError {
                event_type: score_child.event_type,
                err: convert_error(score_update, e),
            })?;

        let ledger = if self.season < 19 {
            Ledger::None
        } else if self.season < 21 {
            let score_ledger = score_child.metadata_str("ledger")?;
            let (_, parsed) = parse_score_ledger_v1
                .parse(score_ledger)
                .finish()
                .map_err(|e| FeedParseError::ScoreLedgerParseError {
                    event_type: score_child.event_type,
                    err: convert_error(score_ledger, e),
                    original: score_ledger.to_string(),
                })?;

            if let Some((base_runs, lines)) = parsed {
                Ledger::V1 {
                    base_runs,
                    lines: lines
                        .into_iter()
                        .map(|line| match line {
                            ParsedLedgerLineV1::NegativePolarity => LedgerLineV1::NegativePolarity,
                            ParsedLedgerLineV1::Underachiever => LedgerLineV1::Underachiever,
                            ParsedLedgerLineV1::Underhanded => LedgerLineV1::Underhanded,
                            ParsedLedgerLineV1::Subtractor => LedgerLineV1::Subtractor,
                            ParsedLedgerLineV1::Tired(name) => {
                                LedgerLineV1::Tired(name.to_string())
                            }
                            ParsedLedgerLineV1::Wired(name) => {
                                LedgerLineV1::Wired(name.to_string())
                            }
                            ParsedLedgerLineV1::AcidicPitch => LedgerLineV1::AcidicPitch,
                            ParsedLedgerLineV1::Magnified => LedgerLineV1::Magnified,
                        })
                        .collect(),
                }
            } else {
                Ledger::None
            }
        } else {
            let score_ledger = score_child.metadata_str("ledger")?;
            let (_, ledger) = LedgerT::parse(score_ledger)?;
            Ledger::V2(ledger)
        };

        Ok(Some(ScoreSummary {
            away_emoji: score_child.metadata_str("awayEmoji")?.to_string(),
            away_score: score_child.metadata_f64("awayScore")?,
            home_emoji: score_child.metadata_str("homeEmoji")?.to_string(),
            home_score: score_child.metadata_f64("homeScore")?,
            runs_scored,
            ledger,
            team_id: score_child.next_team_id()?,
            team_nickname: team_nickname.to_string(),
            sub_event: score_child.as_sub_event(),
        }))
    }

    pub fn parse_balloons_from_score_summary<LedgerT: LedgerV2>(
        &mut self,
        score_summary: Option<&ScoreSummary<LedgerT>>,
    ) -> Result<Option<String>, FeedParseError> {
        // The number of balloons isn't `ledger.base_runs`, because balloons take Magnified into
        // account: "55abf086-150f-47da-97cf-dd674e65f572"
        // The number of runs isn't unrounded or truncated runs, because 1 balloon is inflated for
        // an 0.9-run Acidic Pitch score: "d97cbebe-4357-4765-b221-c941878ef26e"
        // Simplest remaining explanation is that it's rounded runs
        let runs_scored = score_summary
            .as_ref()
            .map_or(1, |s| s.runs_scored.round() as i64);
        self.parse_balloons(runs_scored)
    }

    pub fn parse_balloons(&mut self, runs_scored: i64) -> Result<Option<String>, FeedParseError> {
        let before_s20d81 = (self.season, self.day) < (19, 80);
        let stadium_name = self.next_parse(opt(parse_balloons(runs_scored, before_s20d81)))?;

        Ok(stadium_name.map(str::to_string))
    }

    pub fn parse_unknown_number_of_balloons(&mut self) -> Result<Option<Balloons>, FeedParseError> {
        let before_s20d81 = (self.season, self.day) < (19, 80);
        self.next_parse(opt(parse_unknown_number_of_balloons(before_s20d81)))
            // map the Result
            .map(|info| {
                // map the Option
                info.map(|(stadium_name, num_balloons)| Balloons {
                    stadium_name: stadium_name.to_string(),
                    num_balloons,
                })
            })
    }

    pub fn parse_scoring_players(
        &mut self,
        label: &'static str,
    ) -> Result<
        (
            Vec<(
                Uuid,
                Option<(String, Option<bool>)>,
                String,
                Option<Option<String>>,
                bool,
                Option<Option<String>>,
            )>,
            Vec<(Uuid, String, String)>,
        ),
        FeedParseError,
    > {
        let (scorers, attractions) = self.next_parse(parse_scores(
            label,
            (self.season, self.day) < (15, 3),
            // TODO Should I reference event types here or should I add another argument?
            // (if this is even the right thing to check)
            self.season < 21 || self.event_type == EventType::Walk,
        ))?;

        let scoring_players = scorers
            .into_iter()
            .map(|score| {
                ParseOk((
                    self.next_player_id()?,
                    score.damaged_item_name.map(|(n, p)| (n.to_string(), p)),
                    score.player_name.to_string(),
                    score.hotel_motel_party.map(|n| n.map(str::to_string)),
                    score.is_slippery,
                    score.shame.map(|s| s.map(str::to_string)),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let attracted_players = attractions
            .into_iter()
            .map(|attraction| {
                ParseOk((
                    self.next_player_id()?,
                    attraction.team_nickname.to_string(),
                    attraction.player_name.to_string(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((scoring_players, attracted_players))
    }

    // This is only for parsing standalone attractions. Attractions usually
    // get parsed as part of parse_scoring_players. As of this writing, this
    // is only used in fielder's choices
    pub fn parse_attractions(&mut self) -> Result<Vec<(Uuid, String, String)>, FeedParseError> {
        self.next_parse(parse_attractions)?
            .into_iter()
            .map(|attraction| {
                ParseOk((
                    self.next_player_id()?,
                    attraction.team_nickname.to_string(),
                    attraction.player_name.to_string(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn parse_scoring_players_fc(
        &mut self,
        label: &'static str,
    ) -> Result<
        Vec<(
            Uuid,
            Option<(String, Option<bool>)>,
            String,
            Option<Option<String>>,
            bool,
            Option<Option<String>>,
        )>,
        FeedParseError,
    > {
        let scorers = self.next_parse(parse_scores_fc(
            label,
            (self.season, self.day) < (15, 3),
            // TODO Should I reference event types here or should I add another argument?
            // (if this is even the right thing to check)
            self.season < 21 || self.event_type == EventType::Walk,
        ))?;

        scorers
            .into_iter()
            .map(|score| {
                ParseOk((
                    self.next_player_id()?,
                    score.damaged_item_name.map(|(n, p)| (n.to_string(), p)),
                    score.player_name.to_string(),
                    score.hotel_motel_party.map(|n| n.map(str::to_string)),
                    score.is_slippery,
                    score.shame.map(|s| s.map(str::to_string)),
                ))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn next_item_damage(
        &mut self,
        item_name_plural: Option<bool>,
    ) -> Result<ItemDamaged, FeedParseError> {
        // Ambitious seems to have been accidentally used for some item damages in s17
        // TODO: Only accept Ambitious on the days it was incorrectly used
        let mut damage_child = self.next_child_any(&[
            EventType::ItemDamaged,
            EventType::ItemBreaks,
            EventType::Ambitious,
        ])?;

        Ok(ItemDamaged {
            item_id: damage_child.metadata_uuid("itemId")?,
            item_name: damage_child.metadata_str("itemName")?.to_string(),
            item_name_plural,
            item_mods: damage_child
                .metadata_str_vec("mods")?
                .into_iter()
                .map(str::to_string)
                .collect(),
            durability: damage_child.metadata_i64("itemDurability")?,
            health: damage_child.metadata_i64("itemHealthAfter")?,
            player_item_rating_before: damage_child.metadata_f64_opt("playerItemRatingBefore")?,
            player_item_rating_after: damage_child.metadata_f64_opt("playerItemRatingAfter")?,
            player_rating: damage_child.metadata_f64("playerRating")?,
            team_id: damage_child.next_team_id()?,
            player_id: damage_child.next_player_id()?,
            sub_event: damage_child.as_sub_event(),
        })
    }

    pub fn next_item_repaired(
        &mut self,
        player_name: String,
    ) -> Result<ItemRepaired, FeedParseError> {
        // Coasting was used for a time, possibly by mistake
        let mut child = self.next_child_any(&[
            EventType::BrokenItemRepaired,
            EventType::DamagedItemRepaired,
            EventType::Coasting,
        ])?;
        Ok(ItemRepaired {
            item_id: child.metadata_uuid("itemId")?,
            item_name: child.metadata_str("itemName")?.to_string(),
            item_mods: child
                .metadata_str_vec("mods")?
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            durability: child.metadata_i64("itemDurability")?,
            health_before: child.metadata_i64("itemHealthBefore")?,
            health_after: child.metadata_i64("itemHealthAfter")?,
            player_item_rating_before: child.metadata_f64_opt("playerItemRatingBefore")?,
            player_item_rating_after: child.metadata_f64_opt("playerItemRatingAfter")?,
            player_rating: child.metadata_f64("playerRating")?,
            team_id: child.next_team_id()?,
            player_id: child.next_player_id()?,
            player_name,
            sub_event: child.as_sub_event(),
        })
    }

    pub fn parse_item_damage(
        &mut self,
        batter_name: &str,
    ) -> Result<Option<ItemDamaged>, FeedParseError> {
        self.next_parse(opt(parse_item_damage(
            batter_name,
            (self.season, self.day) < (15, 3),
        )))?
        .map(|(_item_name, item_name_pural)| self.next_item_damage(item_name_pural))
        .transpose()
    }

    pub fn parse_item_damage_and_name(
        &mut self,
        newline_before: bool,
    ) -> Result<Option<(String, ItemDamaged)>, FeedParseError> {
        self.next_parse(opt(parse_item_damage_unknown_name(
            (self.season, self.day) < (15, 3),
            newline_before,
        )))?
        .map(|(_item_name, item_name_plural, player_name)| {
            Ok((
                player_name.to_string(),
                self.next_item_damage(item_name_plural)?,
            ))
        })
        .transpose()
    }

    pub fn parse_item_damages_and_names(
        &mut self,
        newline_before: bool,
    ) -> Result<Vec<(String, ItemDamaged)>, FeedParseError> {
        let mut broken_items = Vec::new();
        while let Some(d) = self.parse_item_damage_and_name(newline_before)? {
            broken_items.push(d);
        }
        Ok(broken_items)
    }

    pub fn parse_pitch(&mut self) -> Result<GamePitch, FeedParseError> {
        let double_strike = self
            .next_parse_opt(parse_terminated(" fires a Double Strike!\n"))
            .map(|player_name| player_name.to_string());

        let acidic_pitch = self
            .next_parse_opt(parse_terminated(" throws an Acidic pitch!\n"))
            .map(|player_name| player_name.to_string());

        Ok(GamePitch {
            double_strike,
            acidic_pitch,
        })
    }

    // Outer option: was the charge blood message present in the description
    // Inner option: was the charge blood sub-event present in the metadata
    pub fn parse_charge_blood(
        &mut self,
        batter_name: &str,
        a: &str,
    ) -> Result<Option<Option<ModChangeSubEvent>>, FeedParseError> {
        self.next_parse_opt(parse_charge_blood(batter_name, a))
            .map(|()| {
                // At least once (event 6bbe5107-9123-42fd-8269-7416d6adaa7b),
                // the charge blood text was in the event but the child event
                // was not present
                let child = self.next_child_opt(EventType::AddedModFromOtherMod)?;
                ParseOk(
                    child
                        .map(|mut child| {
                            ParseOk(ModChangeSubEvent {
                                sub_event: child.as_sub_event(),
                                team_id: child.next_team_id()?,
                            })
                        })
                        .transpose()?,
                )
            })
            .transpose()
    }

    pub fn parse_birds(&mut self) -> Option<i64> {
        self.next_parse_opt(parse_birds)
    }

    pub fn parse_parasite(&mut self) -> Result<Option<Parasite>, FeedParseError> {
        self.next_parse_opt(parse_parasite)
            .map(|(sipper_name, sippee_name, sipped_attribute_name)| {
                // Both events have to be both increase and decrease because of negative attributes
                // (unless I want to check against sipped_attribute_name, which I don't)
                let mut batter_event = self.next_child_any(&[
                    EventType::PlayerAttributeDecrease,
                    EventType::PlayerAttributeIncrease,
                ])?;
                let maintenance_mode = self.parse_maintenance_mode_opt()?;

                let mut pitcher_event = self.next_child_any(&[
                    EventType::PlayerAttributeDecrease,
                    EventType::PlayerAttributeIncrease,
                ])?;
                ParseOk(Parasite {
                    batter_team_id: batter_event.next_team_id()?,
                    batter_id: batter_event.next_player_id()?,
                    batter_name: sippee_name.to_string(),
                    pitcher_team_id: pitcher_event.next_team_id()?,
                    pitcher_id: pitcher_event.next_player_id()?,
                    pitcher_name: sipper_name.to_string(),
                    attribute_name: sipped_attribute_name.to_string(),
                    attribute_id: batter_event.metadata_i64("type")?,
                    maintenance_mode,
                    batter_rating_before: batter_event.metadata_f64("before")?,
                    batter_rating_after: batter_event.metadata_f64("after")?,
                    batter_sub_event: batter_event.as_sub_event(),
                    pitcher_rating_before: pitcher_event.metadata_f64("before")?,
                    pitcher_rating_after: pitcher_event.metadata_f64("after")?,
                    pitcher_sub_event: pitcher_event.as_sub_event(),
                })
            })
            .transpose()
    }

    pub fn parse_maintenance_mode_opt(
        &mut self,
    ) -> Result<Option<MaintenanceMode>, FeedParseError> {
        self.next_child_opt(EventType::AddedMod)?
            .map(|mut mm_event| {
                // Make sure this is a maintenance mode event by verifying the description
                mm_event.next_parse_tag("Impairment Detected. Entering Maintenance Mode.")?;

                ParseOk(MaintenanceMode {
                    sub_event: mm_event.as_sub_event(),
                    team_id: mm_event.next_team_id()?,
                })
            })
            .transpose()
    }

    pub fn next_boost_child(&mut self) -> Result<PlayerBoostSubEvent, FeedParseError> {
        let child = self.next_child(EventType::PlayerStatIncrease)?;
        Ok(PlayerBoostSubEvent {
            rating_before: child.metadata_f64("before")?,
            rating_after: child.metadata_f64("after")?,
            sub_event: child.as_sub_event(),
        })
    }

    pub fn next_boost_child_with_team(
        &mut self,
    ) -> Result<PlayerBoostSubEventWithTeam, FeedParseError> {
        let mut child = self.next_child(EventType::PlayerStatIncrease)?;
        Ok(PlayerBoostSubEventWithTeam {
            team_id: child.next_team_id()?,
            rating_before: child.metadata_f64("before")?,
            rating_after: child.metadata_f64("after")?,
            sub_event: child.as_sub_event(),
        })
    }

    pub fn parse_hotel_motel_parties(
        &mut self,
    ) -> Result<Vec<HotelMotelScoringPlayer>, FeedParseError> {
        let mut parties = Vec::new();
        while let Some((player_name, birds)) = self.next_parse_opt(parse_hotel_motel_party) {
            let mut child = self.next_child(EventType::PlayerStatIncrease)?;
            parties.push(HotelMotelScoringPlayer {
                player_id: child.next_player_id()?,
                player_name: player_name.to_string(),
                party: HotelMotelParty {
                    birds: birds.map(str::to_string),
                    boost: PlayerBoostSubEventWithTeam {
                        team_id: child.next_team_id()?,
                        rating_before: child.metadata_f64("before")?,
                        rating_after: child.metadata_f64("after")?,
                        sub_event: child.as_sub_event(),
                    },
                },
            });
        }
        Ok(parties)
    }

    pub fn parse_hype_from_stadium(
        &mut self,
        stadium_name: String,
    ) -> Result<Hype, FeedParseError> {
        let hype_child = self.next_child(EventType::HypeBuilds)?;

        Ok(Hype {
            stadium_name,
            hype_before: hype_child.metadata_f64("before")?,
            hype_after: hype_child.metadata_f64("after")?,
            sub_event: hype_child.as_sub_event(),
        })
    }

    // This needs a better name. The first `parse` is meant to clue you in to the
    // fact that it's popping a child event, and the second is meant to indicate
    // that it's from a previously-parsed input value
    pub fn parse_shame_from_parsed<T: Into<String>>(
        &mut self,
        shame: Option<Option<T>>,
    ) -> Result<Shame, FeedParseError> {
        Ok(match shame {
            // Outer None: no shame
            None => Shame::No,
            // Inner None: Shame without Hype
            Some(None) => Shame::Yes { hype: None },
            // All-Some: Shame and Hype
            Some(Some(stadium_name)) => {
                let hype = self.parse_hype_from_stadium(stadium_name.into())?;
                Shame::Yes { hype: Some(hype) }
            }
        })
    }

    fn parse_shame_from_parser(
        &mut self,
        parser: impl Parser<&'e str, Output = Option<&'e str>, Error = VerboseError<&'e str>>,
    ) -> Result<Shame, FeedParseError> {
        if self.season < 17 {
            return Ok(Shame::Unknown);
        }
        match self.next_parse(opt(parser))? {
            None => Ok(Shame::No),
            Some(None) => Ok(Shame::Yes { hype: None }),
            Some(Some(stadium)) => {
                let hype = self.parse_hype_from_stadium(stadium.to_string())?;
                Ok(Shame::Yes { hype: Some(hype) })
            }
        }
    }

    pub fn parse_shame(&mut self) -> Result<Shame, FeedParseError> {
        self.parse_shame_from_parser(parse_shame_suffix)
    }

    pub fn parse_prefixed_shame(&mut self) -> Result<Shame, FeedParseError> {
        self.parse_shame_from_parser(parse_shame_prefix)
    }

    pub fn parse_ambush(
        &mut self,
        player_name: &str,
        team_name: &str,
    ) -> Result<Ambush, FeedParseError> {
        // If the player is currently on an incinerated team, this is the event about removing them
        // from that team
        let exit_team_child = self.next_child_opt(EventType::PlayerRemovedFromTeam)?;
        let exit_hall_child = self.next_child(EventType::ExitHallOfFlame)?;
        let mut join_team_child = self.next_child(EventType::PlayerAddedToTeam)?;
        let shadow_boost_child = self.next_child(EventType::PlayerStatIncrease)?;

        let former_team = exit_team_child
            .map(|child| {
                ParseOk(KnownPlayerRemovedFromTeam {
                    team_id: child.metadata_uuid("teamId")?,
                    team_nickname: child.metadata_str("teamName")?.to_string(),
                    sub_event: child.as_sub_event(),
                })
            })
            .transpose()?;

        Ok(Ambush {
            team_id: join_team_child.next_team_id()?,
            team_nickname: team_name.to_string(),
            player_id: join_team_child.next_player_id()?,
            player_name: player_name.to_string(),
            former_team,
            exit_hall_event: exit_hall_child.as_sub_event(),
            added_to_team_event: join_team_child.as_sub_event(),
            shadow_boost_event: shadow_boost_child.as_sub_event(),
            player_rating_before: shadow_boost_child.metadata_f64("before")?,
            player_rating_after: shadow_boost_child.metadata_f64("after")?,
        })
    }

    pub fn parse_flipped_negative(
        &mut self,
        undertaker_name: Option<&str>,
    ) -> Result<Option<FlipNegative>, FeedParseError> {
        undertaker_name
            .map(|flipper_name| {
                let mut undertaker_elsewhere_event = self.next_child(EventType::AddedMod)?;
                let mut negative_event = self.next_child(EventType::AddedMod)?;

                // The player tag on negative_event is for the person who got flipped and the
                // flipper id is on the parent event. Dunno why.
                let undertaker_player_id = self.next_player_id()?;
                // Schlorp the flippee's player id too. We don't need it in here, and when we do need it
                // we get it from a sub-event, but we need to get rid of it so future calls to
                // next_player_id get the right id.
                let _ = self.next_player_id()?;
                ParseOk(FlipNegative {
                    undertaker_player_id,
                    undertaker_player_name: flipper_name.to_string(),
                    undertaker_elsewhere_sub_event: undertaker_elsewhere_event.as_sub_event(),
                    flip_negative_sub_event: negative_event.as_sub_event(),
                })
            })
            .transpose()
    }

    pub fn parse_team_subseasonal_mod_changes(
        &mut self,
        state: &InterEventState,
    ) -> Result<(Vec<SubseasonalModChange<TeamModChangeSubject>>, bool), FeedParseError> {
        let results = self
            .next_parse(parse_team_subseasonal_mod_changes)?
            .into_iter()
            .map(|(team_nickname, source_mod, active)| {
                let mut child = self.next_child_any_opt(&[
                    EventType::AddedModFromOtherMod,
                    EventType::RemovedModFromOtherMod,
                ])?;
                // Team ID is normally on the child, but if the child doesn't have one, I'm trying
                // out falling back to the first ID listed on the parent. I'm almost certain this
                // will be wrong and need to be changed, but I want proof that that's the case
                // first.
                let team_id = child
                    .as_mut()
                    .map_or_else(|| self.next_team_id(), |c| c.next_team_id())?;

                if let Some(nick) = team_nickname {
                    assert!(is_known_team_nickname(nick));
                }

                ParseOk(SubseasonalModChange {
                    source_mod,
                    active,
                    subject: TeamModChangeSubject {
                        team_id,
                        team_nickname: team_nickname.map(str::to_string),
                    },
                    details: Some(SubseasonalModChangeDetails {
                        subject: (),
                        sub_event: child.as_ref().map(EventParseWrapper::as_sub_event),
                        // There's probably a way to get around the to_string here, but it's not
                        // important enough to worry about
                        dependent_mod_change: state
                            .extract_dependent_mod(&(team_id, source_mod.mod_id().to_string())),
                    }),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let is_terminal = if results.is_empty() {
            // If there were no team mod changes, this can't be the end of the event because that
            // would make it an empty event (assuming there was nothing parsed before this, which so
            // far has always been true)
            false
        } else {
            // If there were some team mod changes, there's a newline iff there is more text after
            // this, i.e. this section is terminal iff there was no newline
            self.next_parse(opt(tag("\n")))?.is_none()
        };

        Ok((results, is_terminal))
    }

    pub fn parse_player_subseasonal_mod_change(
        &mut self,
        state: &InterEventState,
        source_mod: SubseasonalMod,
    ) -> Result<SubseasonalModChange<PlayerModChangeSubject>, FeedParseError> {
        let (player_name, is_active) =
            self.next_parse(parse_player_subseasonal_mod_change(source_mod))?;
        self.parse_player_subseasonal_mod_change_internal(state, source_mod, player_name, is_active)
    }

    pub fn parse_player_subseasonal_mod_change_opt(
        &mut self,
        state: &InterEventState,
        source_mod: SubseasonalMod,
    ) -> Result<Option<SubseasonalModChange<PlayerModChangeSubject>>, FeedParseError> {
        self.next_parse_opt(parse_player_subseasonal_mod_change(source_mod))
            .map(|(player_name, is_active)| {
                self.parse_player_subseasonal_mod_change_internal(
                    state,
                    source_mod,
                    player_name,
                    is_active,
                )
            })
            .transpose()
    }

    fn parse_player_subseasonal_mod_change_internal(
        &mut self,
        state: &InterEventState,
        source_mod: SubseasonalMod,
        player_name: &str,
        is_active: bool,
    ) -> Result<SubseasonalModChange<PlayerModChangeSubject>, FeedParseError> {
        // Sandy Crossing once didn't have a child event: e704e4ae-e453-403d-bb8c-b1584503967e
        let player_id = self.next_player_id()?;
        if let Some(mut child) = self.next_child_opt(if is_active {
            EventType::AddedModFromOtherMod
        } else {
            EventType::RemovedModFromOtherMod
        })? {
            let team_id = child.next_team_id()?;

            ParseOk(SubseasonalModChange {
                source_mod,
                active: child.event_type == EventType::AddedModFromOtherMod,
                subject: PlayerModChangeSubject {
                    player_id,
                    player_name: player_name.to_string(),
                },
                details: Some(SubseasonalModChangeDetails {
                    subject: PlayerModChangeSubjectDetails { team_id },
                    sub_event: Some(child.as_sub_event()),
                    // There's probably a way to get around the to_string here, but it's not
                    // important enough to worry about
                    dependent_mod_change: state
                        .extract_dependent_mod(&(team_id, source_mod.mod_id().to_string())),
                }),
            })
        } else {
            ParseOk(SubseasonalModChange {
                source_mod,
                active: is_active,
                subject: PlayerModChangeSubject {
                    player_id,
                    player_name: player_name.to_string(),
                },
                details: None,
            })
        }
    }

    pub fn parse_win_event(&mut self) -> Result<Option<(WinSubEvent, i64)>, FeedParseError> {
        let win_child = self.next_child_any_opt(&[
            EventType::WinCollectedRegular,
            EventType::WinCollectedPostseason,
        ])?;
        // This function shall be called when event exists iff it's season 20 or later
        assert_eq!(win_child.is_some(), self.season >= 19);
        win_child
            .map(|mut child| {
                let before_s20d81 = (self.season, self.day) < (19, 80);
                let balloons = self.next_parse_opt(parse_balloons(10, before_s20d81));
                let win = WinSubEvent {
                    team_id: child.next_team_id()?,
                    wins_after: child.metadata_i64("after")?,
                    sub_event: child.as_sub_event(),
                    balloons: balloons.map(str::to_string),
                };
                let amount = child.metadata_i64("amount")?;
                ParseOk((win, amount))
            })
            .transpose()
    }

    pub fn parse_earned_win(&mut self) -> Result<EarnedWin, FeedParseError> {
        Self::make_earned_win(self.next_child_any(&[
            EventType::WinCollectedRegular,
            EventType::WinCollectedPostseason,
        ])?)
    }

    pub fn parse_earned_win_opt(&mut self) -> Result<Option<EarnedWin>, FeedParseError> {
        self.next_child_any_opt(&[
            EventType::WinCollectedRegular,
            EventType::WinCollectedPostseason,
        ])?
        .map(Self::make_earned_win)
        .transpose()
    }

    pub fn parse_scattered(&mut self) -> Result<Option<Scattered>, FeedParseError> {
        self.next_child_if_mod_effect(EventType::AddedMod, "SCATTERED")?
            .map(|mut scattered_sub_event| {
                let scattered_name =
                    scattered_sub_event.next_parse(parse_terminated(" was Scattered..."))?;

                ParseOk(Scattered {
                    scattered_name: scattered_name.to_string(),
                    sub_event: scattered_sub_event.as_sub_event(),
                })
            })
            .transpose()
    }

    pub fn parse_player_moved_teams(&mut self) -> Result<PlayerMovedTeams, FeedParseError> {
        Ok(PlayerMovedTeams {
            player_id: self.metadata_uuid("playerId")?,
            player_name: self.metadata_str("playerName")?.to_string(),
            location: self.metadata_enum("location")?,
            previous_team_id: self.metadata_uuid("sendTeamId")?,
            previous_team_nickname: self.metadata_str("sendTeamName")?.to_string(),
            new_team_id: self.metadata_uuid("receiveTeamId")?,
            new_team_nickname: self.metadata_str("receiveTeamName")?.to_string(),
            sub_event: self.as_sub_event(),
        })
    }

    pub fn parse_flood_balloon_popped(&mut self) -> Option<BalloonsPopped> {
        self.next_parse_opt(parse_flooding_balloons_popped)
            .map(|(name, birds)| BalloonsPopped {
                stadium_name: name.to_string(),
                birds_scared_away: birds,
            })
    }

    fn make_earned_win(mut win_event: EventParseWrapper) -> Result<EarnedWin, FeedParseError> {
        let (winning_team_nickname, _is_unwin) = win_event.next_parse(parse_team_earned_win)?;
        assert!(is_known_team_nickname(winning_team_nickname));

        let lines = win_event.metadata_str_vec("lines")?;
        // Don't need to worry about the s24 postseason because there wasn't one
        let bracket_type = if lines.len() == 2 && win_event.season < 23 {
            if lines[0] == "Loss: -1" {
                Some(BracketType::Underbracket)
            } else if lines[0] == "Non-Loss: 1" {
                Some(BracketType::Overbracket)
            } else {
                None
            }
        } else {
            None
        };

        Ok(EarnedWin {
            winning_team_nickname: winning_team_nickname.to_string(),
            winning_team_id: win_event.next_team_id()?,
            wins_after: win_event.metadata_i64("after")?,
            sub_event: win_event.as_sub_event(),
            bracket_type,
            turntables: lines.iter().any(|n| n.starts_with("Turntables:")),
            sun_sun: lines.iter().any(|n| n.starts_with("Sun(Sun):")),
        })
    }

    pub fn game(
        &mut self,
        unscatter: Option<ModChangeSubEventWithNamedPlayer>,
        attractor_secret_base: Option<PlayerNameId>,
    ) -> Result<GameEvent, FeedParseError> {
        let game_id = self.next_game_id()?;

        // Order is very important here
        let away_team = self.next_team_id()?;
        let home_team = self.next_team_id()?;

        // I'm taking a guess that Trader stuff is always at the end of an event. This does mean
        // that `game()` has to be called last, but in practice I think I do that already.
        let trader_trade = self
            .next_child_opt(EventType::PlayerLostItem)?
            .map(|mut victim_lost_event| {
                let victim_name =
                    victim_lost_event.next_parse(parse_terminated(" traded away "))?;
                // If there was a PlayerLostItem event, there must also be a PlayerGainedItem event
                let mut trader_gained_event = self.next_child(EventType::PlayerGainedItem)?;
                let trader_name =
                    trader_gained_event.next_parse(parse_terminated(" traded their "))?;

                ParseOk(TraderTrade {
                    victim_id: victim_lost_event.next_player_id()?,
                    victim_name: victim_name.to_string(),
                    victim_team_id: victim_lost_event.next_team_id()?,
                    victim_item_rating_before: victim_lost_event
                        .metadata_f64("playerItemRatingBefore")?,
                    victim_item_rating_after: victim_lost_event
                        .metadata_f64("playerItemRatingAfter")?,
                    victim_rating: victim_lost_event.metadata_f64("playerRating")?,
                    trader_id: trader_gained_event.next_player_id()?,
                    trader_name: trader_name.to_string(),
                    trader_team_id: trader_gained_event.next_team_id()?,
                    trader_item_rating_before: trader_gained_event
                        .metadata_f64("playerItemRatingBefore")?,
                    trader_item_rating_after: trader_gained_event
                        .metadata_f64("playerItemRatingAfter")?,
                    trader_rating: trader_gained_event.metadata_f64("playerRating")?,
                    stolen_item_id: victim_lost_event.metadata_uuid("itemId")?,
                    stolen_item_name: victim_lost_event.metadata_str("itemName")?.to_string(),
                    stolen_item_mods: victim_lost_event
                        .metadata_str_vec("mods")?
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    exchanged_item_name: None, // TODO
                    victim_lost_item_sub_event: victim_lost_event.as_sub_event(),
                    trader_gained_item_sub_event: trader_gained_event.as_sub_event(),
                })
            })
            .transpose()?;

        Ok(GameEvent {
            game_id,
            home_team,
            away_team,
            play: self.play.ok_or_else(|| FeedParseError::MissingMetadata {
                event_type: self.event_type,
                field: "play".to_string(),
            })?,
            unscatter,
            attractor_secret_base,
            trader_trade,
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

    fn build_free_refill(&mut self, name: &str) -> Result<FreeRefill, FeedParseError> {
        let mut child = self.next_child(EventType::RemovedMod)?;
        Ok(FreeRefill {
            sub_event: child.as_sub_event(),
            player_name: name.to_string(),
            player_id: child.next_player_id()?,
            team_id: child.next_team_id_opt(),
        })
    }
}
