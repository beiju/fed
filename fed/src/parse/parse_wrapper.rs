use std::fmt::Display;
use chrono::{DateTime, Utc};
use nom::{Finish, Parser};
use nom::combinator::opt;
use nom::error::convert_error;
use uuid::Uuid;
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use crate::fed_event::*;
use crate::FeedParseError;
use crate::parse::{is_known_team_nickname, ParseOk};
use crate::parse::parsers::*;

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
        let (&id, rest) = self.team_ids.split_first()
            .ok_or_else(|| {
                FeedParseError::NotEnoughTags {
                    event_type: self.event_type,
                    tag_type: "team",
                    expected_at_least: self.consumed_team_id_count + 1,
                }
            })?;
        self.consumed_team_id_count += 1;
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
            return Ok(None);
        };
        if !expected_types.iter().any(|&t| child.r#type == t) {
            return Ok(None);
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

    pub fn get_metadata(&self, key: &'static str) -> Result<&'e serde_json::Value, FeedParseError> {
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
                    field: key.to_string(),
                }
            })
    }

    pub fn metadata_i64(&self, key: &'static str) -> Result<i64, FeedParseError> {
        self.get_metadata(key)?
            .as_i64()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key.to_string(),
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
                    field: key.to_string(),
                    ty: "f64",
                }
            })
    }

    pub fn metadata_f64_opt(&self, key: &'static str) -> Result<Option<f64>, FeedParseError> {
        let value = self.get_metadata(key)?;
        if value.is_null() {
            Ok(None)
        } else {
            value.as_f64()
                .ok_or_else(|| {
                    FeedParseError::MetadataTypeError {
                        event_type: self.event_type,
                        field: key.to_string(),
                        ty: "f64",
                    }
                })
                .map(|n| Some(n))
        }
    }

    pub fn metadata_str(&self, key: &'static str) -> Result<&'e str, FeedParseError> {
        self.get_metadata(key)?
            .as_str()
            .ok_or_else(|| {
                FeedParseError::MetadataTypeError {
                    event_type: self.event_type,
                    field: key.to_string(),
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
                    field: key.to_string(),
                    ty: "array",
                }
            })
            .and_then(|vec| {
                vec.iter()
                    .enumerate()
                    .map(|(i, item)| {
                        item.as_str()
                            .ok_or_else(|| {
                                FeedParseError::MetadataTypeError {
                                    event_type: self.event_type,
                                    field: format!("{key}[{i}]"),
                                    ty: "str",
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
                    field: key.to_string(),
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
            .map(|name| self.build_free_refill(name))
            .collect()
    }

    // Use when only one free refill is allowed
    pub fn parse_free_refill(&mut self) -> Result<Option<FreeRefill>, FeedParseError> {
        self.next_parse(opt(parse_free_refill))?
            .map(|name| self.build_free_refill(name))
            .transpose()
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

    pub fn parse_scores_with_scoring_players(&mut self, scoring_players: Vec<(Uuid, Option<(String, Option<bool>)>, String, Option<String>)>) -> Result<Scores, FeedParseError> {
        let scores = scoring_players.into_iter()
            .map(|(player_id, item_name, player_name, attraction)| {
                let item_damage = item_name
                    .map(|(_name, plural)| self.next_item_damage(plural))
                    .transpose()?;
                let attraction = attraction
                    .map(|team_nickname| {
                        assert!(is_known_team_nickname(&team_nickname));
                        let mut child = self.next_child(EventType::PlayerAddedToTeam)?;
                        ParseOk(Attraction {
                            team_nickname,
                            team_id: child.next_team_id()?,
                            sub_event: child.as_sub_event(),
                        })
                    })
                    .transpose()?;
                ParseOk(ScoringPlayer {
                    player_id,
                    player_name,
                    item_damage,
                    attraction,
                })
            })
            .collect::<Result<_, _>>()?;

        let free_refills = self.parse_free_refills()?;

        Ok(Scores {
            scores,
            free_refills,
        })
    }

    pub fn parse_scoring_players(&mut self, label: &'static str) -> Result<Vec<(Uuid, Option<(String, Option<bool>)>, String, Option<String>)>, FeedParseError> {
        let scorers = self.next_parse(parse_scores(label, (self.season, self.day) < (15, 3)))?;
        let scoring_players = scorers.into_iter()
            .map(|score| {
                ParseOk((self.next_player_id()?,
                         score.damaged_item_name.map(|(n, p)| (n.to_string(), p)),
                         score.player_name.to_string(),
                         score.attraction.map(str::to_string),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(scoring_players)
    }

    pub fn next_item_damage(&mut self, item_name_plural: Option<bool>) -> Result<ItemDamaged, FeedParseError> {
        let mut damage_child = self.next_child_any(&[EventType::ItemDamaged, EventType::ItemBreaks])?;

        Ok(ItemDamaged {
            item_id: damage_child.metadata_uuid("itemId")?,
            item_name: damage_child.metadata_str("itemName")?.to_string(),
            item_name_plural,
            item_mods: damage_child.metadata_str_vec("mods")?.into_iter().map(str::to_string).collect(),
            durability: damage_child.metadata_i64("itemDurability")?,
            health: damage_child.metadata_i64("itemHealthAfter")?,
            player_item_rating_before: damage_child.metadata_f64("playerItemRatingBefore")?,
            player_item_rating_after: damage_child.metadata_f64("playerItemRatingAfter")?,
            player_rating: damage_child.metadata_f64("playerRating")?,
            team_id: damage_child.next_team_id()?,
            player_id: damage_child.next_player_id()?,
            sub_event: damage_child.as_sub_event(),
        })
    }

    pub fn parse_item_damage(&mut self, batter_name: &str) -> Result<Option<ItemDamaged>, FeedParseError> {
        self.next_parse(opt(parse_item_damage(batter_name, (self.season, self.day) < (15, 3))))?
            .map(|(_item_name, item_name_pural)| {
                self.next_item_damage(item_name_pural)
            })
            .transpose()
    }

    pub fn parse_item_damage_and_name(&mut self, newline_before: bool) -> Result<Option<(String, ItemDamaged)>, FeedParseError> {
        self.next_parse(opt(parse_item_damage_unknown_name((self.season, self.day) < (15, 3), newline_before)))?
            .map(|(_item_name, item_name_plural, player_name)| {
                Ok((player_name.to_string(), self.next_item_damage(item_name_plural)?))
            })
            .transpose()
    }

    pub fn parse_item_damages_and_names(&mut self, newline_before: bool) -> Result<Vec<(String, ItemDamaged)>, FeedParseError> {
        let mut broken_items = Vec::new();
        while let Some(d) = self.parse_item_damage_and_name(newline_before)? {
            broken_items.push(d);
        }
        Ok(broken_items)
    }
    
    pub fn parse_pitch(&mut self) -> Result<GamePitch, FeedParseError> {
        let double_strike = self.next_parse_opt(parse_terminated(" fires a Double Strike!\n"))
            .map(|player_name| player_name.to_string());
        
        Ok(GamePitch {
            double_strike,
        })
    }

    pub fn parse_charge_blood(&mut self, batter_name: &str, a: &str) -> Result<Option<ModChangeSubEvent>, FeedParseError> {
        self.next_parse_opt(parse_charge_blood(batter_name, a))
            .map(|()| {
                let mut child = self.next_child(EventType::AddedModFromOtherMod)?;
                ParseOk(ModChangeSubEvent {
                    sub_event: child.as_sub_event(),
                    team_id: child.next_team_id()?,
                })
            })
            .transpose()
    }

    pub fn parse_birds(&mut self) -> Option<i32> {
        self.next_parse_opt(parse_birds)
    }

    pub fn parse_parasite(&mut self) -> Result<Option<Parasite>, FeedParseError> {
        self.next_parse_opt(parse_parasite)
            .map(|(sipper_name, sippee_name, sipped_attribute_name)| {
                // Both events have to be both increase and decrease because of negative attributes
                // (unless I want to check against sipped_attribute_name, which I don't)
                let mut batter_event = self.next_child_any(&[EventType::PlayerAttributeDecrease, EventType::PlayerAttributeIncrease])?;
                let mut pitcher_event = self.next_child_any(&[EventType::PlayerAttributeDecrease, EventType::PlayerAttributeIncrease])?;
                ParseOk(Parasite {
                    batter_team_id: batter_event.next_team_id()?,
                    batter_id: batter_event.next_player_id()?,
                    batter_name: sippee_name.to_string(),
                    pitcher_team_id: pitcher_event.next_team_id()?,
                    pitcher_id: pitcher_event.next_player_id()?,
                    pitcher_name: sipper_name.to_string(),
                    attribute_name: sipped_attribute_name.to_string(),
                    attribute_id: batter_event.metadata_i64("type")?,
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

    pub fn game(&mut self, unscatter: Option<ModChangeSubEventWithNamedPlayer>, attractor_secret_base: Option<PlayerNameId>) -> Result<GameEvent, FeedParseError> {
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
                        field: "play".to_string(),
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