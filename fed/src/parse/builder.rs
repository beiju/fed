use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use std::fmt::Write;

use crate::parse::event_schema::{FreeRefill, GameEvent, ModChangeSubEvent, ModChangeSubEventWithPlayer, Scores, SpicyStatus, StoppedInhabiting, SubEvent};

pub struct EventBuilderCommon {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: String,
    pub day: i32,
    pub phase: i32,
    pub season: i32,
    pub tournament: i32,
    pub nuts: i32,
}

impl EventBuilderCommon {
    pub fn fill(self, update: EventBuilderUpdate) -> EventBuilderFull<'static, 'static, 'static> {
        EventBuilderFull {
            common: self,
            game: None,
            update,
            children: Vec::new(),
            metadata: json!({}),
            scores: None,
            stopped_inhabiting: None,
            spicy_change: SpicyChange::None,
        }
    }

    pub fn for_game(self, game: &GameEvent) -> EventBuilderForGame {
        EventBuilderForGame {
            common: self,
            // TODO don't clone
            game: game.clone(),
        }
    }
}

pub struct EventBuilderChild {
    pub common: SubEvent,
}

impl EventBuilderChild {
    pub fn new(sub_event: &SubEvent) -> EventBuilderChild {
        EventBuilderChild {
            common: *sub_event,
        }
    }

    pub fn update(self, update: EventBuilderUpdate) -> EventBuilderChildFull {
        EventBuilderChildFull {
            common: self.common,
            update,
            metadata: json!({}),
        }
    }
}

#[derive(Debug)]
pub struct EventBuilderChildFull {
    pub common: SubEvent,
    pub update: EventBuilderUpdate,
    pub metadata: serde_json::Value,
}

impl EventBuilderChildFull {
    pub fn metadata(self, metadata: serde_json::Value) -> Self {
        Self {
            metadata,
            ..self
        }
    }
}

#[derive(Default, Debug)]
pub struct EventBuilderUpdate {
    pub r#type: EventType,
    pub category: EventCategory,
    pub description: String,
    pub description_after_score: String,
    pub player_tags: Vec<Uuid>,
    pub team_tags: Vec<Uuid>,
    pub override_team_tags: bool,
}

pub struct EventBuilderForGame {
    pub common: EventBuilderCommon,
    pub game: GameEvent,
}

pub enum SpicyChange<'s> {
    None,
    HeatingUp {
        player_id: Uuid,
        player_name: &'s str,
    },
    RedHot {
        // Don't like that this is an option :(
        red_hot: &'s Option<ModChangeSubEvent>,
        player_id: Uuid,
        player_name: &'s str,
    },
    CooledOff {
        cooled_off: &'s ModChangeSubEventWithPlayer,
        player_name: &'s str,
    },
}

impl EventBuilderForGame {
    pub fn fill(self, update: EventBuilderUpdate) -> EventBuilderFull<'static, 'static, 'static> {
        EventBuilderFull {
            common: self.common,
            game: Some(self.game),
            update,
            children: Vec::new(),
            metadata: json!({}),
            scores: None,
            stopped_inhabiting: None,
            spicy_change: SpicyChange::None,
        }
    }
}

pub struct EventBuilderFull<'s, 'i, 'c> {
    pub common: EventBuilderCommon,
    pub game: Option<GameEvent>,
    pub update: EventBuilderUpdate,
    pub children: Vec<EventBuilderChildFull>,
    pub metadata: serde_json::Value,
    pub scores: Option<(&'s Scores, &'static str)>,
    pub stopped_inhabiting: Option<&'i StoppedInhabiting>,
    pub spicy_change: SpicyChange<'c>,
}


impl<'ts, 'ti, 'tc> EventBuilderFull<'ts, 'ti, 'tc> {
    pub fn scores<'s>(self, scores: &'s Scores, score_text: &'static str) -> EventBuilderFull<'s, 'ti, 'tc> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: Some((scores, score_text)),
            stopped_inhabiting: self.stopped_inhabiting,
            spicy_change: self.spicy_change,
        }
    }

    pub fn stopped_inhabiting<'i>(self, stopped_inhabiting: &'i Option<StoppedInhabiting>) -> EventBuilderFull<'ts, 'i, 'tc> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: self.scores,
            stopped_inhabiting: stopped_inhabiting.as_ref(),
            spicy_change: self.spicy_change,
        }
    }

    pub fn cooled_off<'c>(self, cooled_off: &'c Option<ModChangeSubEventWithPlayer>, player_name: &'c str) -> EventBuilderFull<'ts, 'ti, 'c> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: self.scores,
            stopped_inhabiting: self.stopped_inhabiting,
            spicy_change: match cooled_off {
                None => { SpicyChange::None }
                Some(cooled_off) => { SpicyChange::CooledOff { cooled_off, player_name } }
            },

        }
    }

    pub fn spicy<'c>(self, spicy: &'c SpicyStatus, player_id: Uuid, player_name: &'c str) -> EventBuilderFull<'ts, 'ti, 'c> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: self.scores,
            stopped_inhabiting: self.stopped_inhabiting,
            spicy_change: match spicy {
                SpicyStatus::None => { SpicyChange::None }
                SpicyStatus::HeatingUp => { SpicyChange::HeatingUp { player_id, player_name } }
                SpicyStatus::RedHot(red_hot) => { SpicyChange::RedHot { red_hot, player_id, player_name } }
            },

        }
    }

    pub fn metadata(self, metadata: serde_json::Value) -> Self {
        Self {
            metadata,
            ..self
        }
    }

    pub fn full_metadata(self, metadata: EventMetadata) -> EventBuilderWithFullMetadata {
        EventBuilderWithFullMetadata {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata,
        }
    }

    pub fn child(self, child: impl Into<EventBuilderChildFull>) -> Self {
        let mut children = self.children;
        children.push(child.into());
        Self {
            children,
            ..self
        }
    }

    pub fn children<T: Into<EventBuilderChildFull>>(self, new_children: impl IntoIterator<Item=T>) -> Self {
        let mut children = self.children;
        children.extend(new_children.into_iter().map(Into::into));
        Self {
            children,
            ..self
        }
    }

    pub fn build(self) -> EventuallyEvent {
        let mut children_builders = Vec::new();
        let mut prefix = String::new();
        let mut suffix = String::new();
        let mut player_tags = Vec::new();

        // Just guessing that attractor is before unscatter
        let has_attractor = if let Some(attractor) = self.game.as_ref().and_then(|game| game.attractor_secret_base.as_ref()) {
            write!(prefix, "{} enters the Secret Base...\n", attractor.player_name).unwrap();
            player_tags.push(attractor.player_id);
            true
        } else {
            false
        };

        if let Some(unscatter) = self.game.as_ref().and_then(|game| game.unscatter.as_ref()) {
            children_builders.push(
                EventBuilderChild::new(&unscatter.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{} was Unscattered.", unscatter.player_name),
                        player_tags: vec![unscatter.player_id],
                        team_tags: vec![unscatter.team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SCATTERED",
                        "type": 0, // ?
                    }))
            );
        }

        if let Some((scores, score_text)) = self.scores {
            suffix += &*scores.to_description_with_text_between(score_text,
                                                                &self.update.description_after_score);
            children_builders.extend(scores.free_refills.iter()
                .map(|free_refill| make_free_refill_child(free_refill)));
            player_tags.extend(scores.scorer_ids());
        } else {
            suffix += &*self.update.description_after_score;
        }

        if let Some(inh) = self.stopped_inhabiting {
            children_builders.push(
                EventBuilderChild::new(&inh.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{} stopped Inhabiting.", inh.inhabiting_player_name),
                        player_tags: vec![inh.inhabiting_player_id],
                        team_tags: inh.inhabiting_player_team_id.into_iter().collect(),
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "INHABITING",
                        "type": 0, // ?
                    }))
            )
        }

        match self.spicy_change {
            SpicyChange::None => {}
            SpicyChange::HeatingUp { player_id, player_name } => {
                player_tags.push(player_id);
                suffix = format!("{suffix}\n{player_name} is Heating Up!")
            },
            SpicyChange::RedHot { red_hot, player_id, player_name } => {
                if let Some(red_hot) = red_hot {
                    children_builders.push(
                        EventBuilderChild::new(&red_hot.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedMod,
                                category: EventCategory::Changes,
                                description: format!("{player_name} is Red Hot!"),
                                team_tags: vec![red_hot.team_id],
                                player_tags: vec![player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                            "mod": "ON_FIRE",
                            "type": 0, // ?
                        })),
                    );
                }
                player_tags.push(player_id);
                suffix = format!("{suffix}\n{player_name} is Red Hot!")
            }
            SpicyChange::CooledOff { cooled_off, player_name } => {
                children_builders.push(
                    EventBuilderChild::new(&cooled_off.sub_event)
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
                suffix = format!("{suffix}\n{player_name} cooled off.")
            }
        }

        children_builders.extend(self.children.into_iter());
        let children = children_builders.into_iter()
            .enumerate()
            // This type can be inferred but code completion has a hard time with it
            .map(|(sub_play, child): (_, EventBuilderChildFull)| {
                let child_metadata = EventMetadata {
                    children: vec![],
                    siblings: vec![],
                    ingest_time: None,
                    ingest_source: None,
                    play: self.game.as_ref().map(|game| game.play),
                    sub_play: Some(sub_play as i64),
                    sibling_ids: None,
                    parent: Some(self.common.id),
                    other: child.metadata,
                };

                EventuallyEvent {
                    id: child.common.id,
                    created: child.common.created,
                    r#type: child.update.r#type,
                    category: child.update.category,
                    metadata: child_metadata,
                    blurb: "".to_string(),
                    description: child.update.description,
                    player_tags: child.update.player_tags,
                    game_tags: self.game.as_ref().map_or_else(|| Vec::new(), |g| vec![g.game_id]),
                    team_tags: child.update.team_tags,
                    sim: self.common.sim.clone(),
                    day: self.common.day,
                    season: self.common.season,
                    tournament: self.common.tournament,
                    phase: self.common.phase,
                    nuts: child.common.nuts,
                }
            })
            .collect();

        let metadata = EventMetadata {
            play: self.game.as_ref().map(|game| game.play),
            // Root events of games are always -1, non-games are null
            sub_play: self.game.as_ref().map(|_| -1),
            children,
            other: self.metadata,
            ..Default::default()
        };

        let suffix = &suffix;

        build_final(self.common, self.game, self.update, metadata, prefix, suffix, player_tags, has_attractor)
    }
}


pub fn make_free_refill_child(free_refill: &FreeRefill) -> EventBuilderChildFull {
    EventBuilderChild::new(&free_refill.sub_event)
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


pub struct EventBuilderWithFullMetadata {
    pub common: EventBuilderCommon,
    pub game: Option<GameEvent>,
    pub update: EventBuilderUpdate,
    pub metadata: EventMetadata,
}

impl EventBuilderWithFullMetadata {
    pub fn build(self) -> EventuallyEvent {
        build_final(self.common, self.game, self.update, self.metadata, String::new(), "", Vec::new(), false)
    }
}

fn build_final(
    common: EventBuilderCommon,
    game: Option<GameEvent>,
    update: EventBuilderUpdate,
    metadata: EventMetadata,
    prefix: String,
    suffix: &str,
    additional_player_tags: impl IntoIterator<Item=Uuid>,
    override_category: bool,
) -> EventuallyEvent {
    let team_tags = if update.override_team_tags {
        update.team_tags
    } else if let Some(ref g) = game {
        [g.away_team, g.home_team].into_iter()
            .chain(update.team_tags)
            .collect()
    } else {
        update.team_tags
    };

    EventuallyEvent {
        id: common.id,
        created: common.created,
        r#type: update.r#type,
        category: if override_category { EventCategory::Special } else { update.category },
        metadata,
        blurb: "".to_string(),
        description: prefix + &update.description + suffix,
        player_tags: update.player_tags.into_iter().chain(additional_player_tags.into_iter()).collect(),
        game_tags: game.as_ref().map_or_else(|| Vec::new(), |g| vec![g.game_id]),
        team_tags,
        sim: common.sim,
        day: common.day,
        season: common.season,
        tournament: common.tournament,
        phase: common.phase,
        nuts: common.nuts,
    }
}
