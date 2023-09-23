use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use std::fmt::Write;
use crate::ItemDamaged;

use crate::fed_event::{FreeRefill, GameEvent, ModChangeSubEvent, ModChangeSubEventWithPlayer, Scores, SpicyStatus, StoppedInhabiting, SubEvent};

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
    pub fn fill(self, update: EventBuilderUpdate) -> EventBuilderFull<'static, 'static, 'static, 'static> {
        EventBuilderFull {
            common: self,
            game: None,
            update,
            children: Vec::new(),
            metadata: json!({}),
            scores: None,
            stopped_inhabiting: None,
            spicy_change: SpicyChange::None,
            item_damage_before_event: Vec::new(),
            item_damage_before_score: Vec::new(),
            item_damage_after_score: Vec::new(),
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
    pub fn fill(self, update: EventBuilderUpdate) -> EventBuilderFull<'static, 'static, 'static, 'static> {
        EventBuilderFull {
            common: self.common,
            game: Some(self.game),
            update,
            children: Vec::new(),
            metadata: json!({}),
            scores: None,
            stopped_inhabiting: None,
            spicy_change: SpicyChange::None,
            item_damage_before_event: Vec::new(),
            item_damage_before_score: Vec::new(),
            item_damage_after_score: Vec::new(),
        }
    }
}

pub struct EventBuilderFull<'s, 'i, 'c, 't> {
    pub common: EventBuilderCommon,
    pub game: Option<GameEvent>,
    pub update: EventBuilderUpdate,
    pub children: Vec<EventBuilderChildFull>,
    pub metadata: serde_json::Value,
    pub scores: Option<(&'s Scores, &'static str)>,
    pub stopped_inhabiting: Option<&'i StoppedInhabiting>,
    pub spicy_change: SpicyChange<'c>,
    pub item_damage_before_event: Vec<(&'t ItemDamaged, &'t str)>,
    pub item_damage_before_score: Vec<(&'t ItemDamaged, &'t str)>,
    pub item_damage_after_score: Vec<(&'t ItemDamaged, &'t str)>,
}

macro_rules! push_description {
    ($description:ident, $($t:tt)*) => {{
        if !$description.is_empty() { write!($description, "\n").unwrap() }
        write!($description, $($t)*).unwrap();
    }};
}

impl<'ts, 'ti, 'tc, 'tt> EventBuilderFull<'ts, 'ti, 'tc, 'tt> {
    pub fn scores<'s>(self, scores: &'s Scores, score_text: &'static str) -> EventBuilderFull<'s, 'ti, 'tc, 'tt> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: Some((scores, score_text)),
            stopped_inhabiting: self.stopped_inhabiting,
            spicy_change: self.spicy_change,
            item_damage_before_event: self.item_damage_before_event,
            item_damage_before_score: self.item_damage_before_score,
            item_damage_after_score: self.item_damage_after_score,
        }
    }

    pub fn stopped_inhabiting<'i>(self, stopped_inhabiting: &'i Option<StoppedInhabiting>) -> EventBuilderFull<'ts, 'i, 'tc, 'tt> {
        EventBuilderFull {
            common: self.common,
            game: self.game,
            update: self.update,
            children: self.children,
            metadata: self.metadata,
            scores: self.scores,
            stopped_inhabiting: stopped_inhabiting.as_ref(),
            spicy_change: self.spicy_change,
            item_damage_before_event: self.item_damage_before_event,
            item_damage_before_score: self.item_damage_before_score,
            item_damage_after_score: self.item_damage_after_score,
        }
    }

    pub fn cooled_off<'c>(self, cooled_off: &'c Option<ModChangeSubEventWithPlayer>, player_name: &'c str) -> EventBuilderFull<'ts, 'ti, 'c, 'tt> {
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
            item_damage_before_event: self.item_damage_before_event,
            item_damage_before_score: self.item_damage_before_score,
            item_damage_after_score: self.item_damage_after_score,
        }
    }

    pub fn spicy<'c>(self, spicy: &'c SpicyStatus, player_id: Uuid, player_name: &'c str) -> EventBuilderFull<'ts, 'ti, 'c, 'tt> {
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
            item_damage_before_event: self.item_damage_before_event,
            item_damage_before_score: self.item_damage_before_score,
            item_damage_after_score: self.item_damage_after_score,
        }
    }

    pub fn item_damage_before_event(mut self, item_damage: impl IntoIterator<Item=&'tt ItemDamaged>, player_name: &'tt str) -> Self {
        self.item_damage_before_event.extend(item_damage.into_iter().map(|d| (d, player_name)));
        self
    }

    pub fn item_damage_before_score(mut self, item_damage: impl IntoIterator<Item=&'tt ItemDamaged>, player_name: &'tt str) -> Self {
        self.item_damage_before_score.extend(item_damage.into_iter().map(|d| (d, player_name)));
        self
    }

    pub fn named_item_damage_before_score(mut self, ii: impl IntoIterator<Item=&'tt (String, ItemDamaged)>) -> Self {
        self.item_damage_before_score.extend(ii.into_iter().map(|(n, d)| (d, n.as_str())));
        self
    }

    pub fn named_item_damage_before_event(mut self, ii: impl IntoIterator<Item=&'tt (String, ItemDamaged)>) -> Self {
        self.item_damage_before_event.extend(ii.into_iter().map(|(n, d)| (d, n.as_str())));
        self
    }

    pub fn item_damage_after_score(mut self, item_damage: impl IntoIterator<Item=&'tt ItemDamaged>, player_name: &'tt str) -> Self {
        self.item_damage_after_score.extend(item_damage.into_iter().map(|d| (d, player_name)));
        self
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
        let mut description = String::new();
        let mut player_tags = Vec::new();

        // Just guessing that attractor is before unscatter
        let has_attractor = if let Some(attractor) = self.game.as_ref().and_then(|game| game.attractor_secret_base.as_ref()) {
            push_description!(description, "{} enters the Secret Base...", attractor.player_name);
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

        self.build_item_damage(&self.item_damage_before_event, &mut description, &mut children_builders);

        push_description!(description, "{}", self.update.description);

        self.build_item_damage(&self.item_damage_before_score, &mut description, &mut children_builders);

        if let Some((scores, score_text)) = self.scores {
            for score in &scores.scores {
                if let Some(item_damage) = &score.item_damage {
                    children_builders.push(make_item_damage_child(
                        possessive(score.player_name.clone()), item_damage, true)
                    )
                }
            }
            description += &*scores.to_description_with_text_between(score_text,
                                                                     &self.update.description_after_score,
                                                                     (self.common.season, self.common.day) < (15, 3));
            for score in &scores.scores {
                if let Some(attraction) = &score.attraction {
                    player_tags.push(score.player_id);
                    children_builders.push(EventBuilderChild::new(&attraction.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::PlayerAddedToTeam,
                            category: EventCategory::Changes,
                            description: format!("The {} Attracted {}!", attraction.team_nickname, score.player_name),
                            team_tags: vec![attraction.team_id],
                            player_tags: vec![score.player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "location": 2, // always shadows
                            "playerId": score.player_id,
                            "playerName": score.player_name,
                            "teamId": attraction.team_id,
                            "teamName": attraction.team_nickname,
                        })))
                }
            }

        } else {
            description += &*self.update.description_after_score;
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

        if let Some((scores, _)) = self.scores {
            children_builders.extend(scores.free_refills.iter()
                .map(|free_refill| make_free_refill_child(free_refill)));
            player_tags.extend(scores.scorer_ids());
        }

        self.build_item_damage(&self.item_damage_after_score, &mut description, &mut children_builders);

        match self.spicy_change {
            SpicyChange::None => {}
            SpicyChange::HeatingUp { player_id, player_name } => {
                player_tags.push(player_id);
                push_description!(description, "{player_name} is Heating Up!");
            }
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
                push_description!(description, "{player_name} is Red Hot!");
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
                push_description!(description, "{player_name} cooled off.")
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
                    player_tags: Some(child.update.player_tags),
                    election_option_id: None,
                    game_tags: Some(self.game.as_ref().map_or_else(|| Vec::new(), |g| vec![g.game_id])),
                    team_tags: Some(child.update.team_tags),
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

        build_final(self.common, self.game, self.update, metadata, description, player_tags, has_attractor)
    }

    fn build_item_damage(&self, v: &Vec<(&ItemDamaged, &str)>, description: &mut String, children_builders: &mut Vec<EventBuilderChildFull>) {
        for (item_damage, player_name) in v {
            let player_name_possessive = possessive(player_name.to_string());
            push_description!(description, "{}{player_name_possessive} {item_damage}",
                              if (self.common.season, self.common.day) < (15, 3) { " " } else { "" });
            children_builders.push(make_item_damage_child(player_name_possessive, item_damage,
                                                          (self.common.season, self.common.day) < (15, 3)));
        }
    }
}

pub fn make_free_refill_child(free_refill: &FreeRefill) -> EventBuilderChildFull {
    EventBuilderChild::new(&free_refill.sub_event)
        .update(EventBuilderUpdate {
            r#type: EventType::RemovedMod,
            category: EventCategory::Changes,
            description: format!("{} used their Free Refill.", free_refill.player_name),
            team_tags: free_refill.team_id.into_iter().collect(),
            player_tags: vec![free_refill.player_id],
            ..Default::default()
        })
        .metadata(json!({
                "mod": "COFFEE_RALLY",
                "type": 0, // ?
            }))
}

#[deprecated = "Use build_item_damage or push_item_damage instead"]
fn make_item_damage_child(player_name_possessive: String, item_damage: &ItemDamaged, extra_space: bool) -> EventBuilderChildFull {
    EventBuilderChild::new(&item_damage.sub_event)
        .update(EventBuilderUpdate {
            r#type: if item_damage.health == 0 { EventType::ItemBreaks } else { EventType::ItemDamaged },
            category: EventCategory::Changes,
            description: format!("{}{player_name_possessive} {item_damage}",
                                 if extra_space { " " } else { "" }),
            team_tags: vec![item_damage.team_id],
            player_tags: vec![item_damage.player_id],
            ..Default::default()
        })
        .metadata(json!({
            "itemDurability": item_damage.durability,
            "itemHealthAfter": item_damage.health,
            "itemHealthBefore": item_damage.health + 1,
            "itemId": item_damage.item_id,
            "itemName": item_damage.item_name,
            "mods": Vec::<String>::new(), // TODO vec of what?
            "playerItemRatingAfter": item_damage.player_item_rating_after.map(zero_int),
            "playerItemRatingBefore": item_damage.player_item_rating_before.map(zero_int),
            "playerRating": zero_int(item_damage.player_rating),
        }))
}


// Sometimes in the metadata, an 0 needs to be an int even if the value is a float. ballclark.
pub(crate) fn zero_int(value: f64) -> serde_json::Value {
    if value == 0.0 {
        serde_json::Value::from(0)
    } else {
        serde_json::Value::from(value)
    }
}


pub(crate) fn possessive(name: String) -> String {
    if name.chars().last().unwrap() == 's' {
        name + "'"
    } else {
        name + "'s"
    }
}

pub struct EventBuilderWithFullMetadata {
    pub common: EventBuilderCommon,
    pub game: Option<GameEvent>,
    pub update: EventBuilderUpdate,
    pub metadata: EventMetadata,
}

impl EventBuilderWithFullMetadata {
    pub fn build(self) -> EventuallyEvent {
        let description = self.update.description.clone();
        build_final(self.common, self.game, self.update, self.metadata, description, Vec::new(), false)
    }
}

fn build_final(
    common: EventBuilderCommon,
    game: Option<GameEvent>,
    update: EventBuilderUpdate,
    metadata: EventMetadata,
    description: String,
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
        description,
        election_option_id: None,
        player_tags: Some(update.player_tags.into_iter().chain(additional_player_tags.into_iter()).collect()),
        game_tags: Some(game.as_ref().map_or_else(|| Vec::new(), |g| vec![g.game_id])),
        team_tags: Some(team_tags),
        sim: common.sim,
        day: common.day,
        season: common.season,
        tournament: common.tournament,
        phase: common.phase,
        nuts: common.nuts,
    }
}

