use std::fmt::{Display, Formatter};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;
use eventually_api::{EventCategory, EventType, EventuallyEvent};
use crate::{Attraction, FreeRefill, GameEvent, ItemDamage, ItemGained, ItemRepaired, ModDuration, Scores, ScoringPlayer, SpicyStatus, StoppedInhabiting, SubEvent};

pub struct EventBuilder(EventuallyEvent);


// Newtype with Display implementation that prints the string using grammatically correct possessive
pub struct Possessive<'a>(pub &'a str);

impl Display for Possessive<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(l) = self.0.chars().last() && l == 's' {
            write!(f, "{}'", self.0)
        } else {
            write!(f, "{}'s", self.0)
        }
    }
}


impl EventBuilder {
    pub fn new(id: Uuid, created: DateTime<Utc>, sim: String, day: i32, season: i32, tournament: i32, phase: i32, nuts: i32) -> Self {
        let mut builder = Self(EventuallyEvent {
            id,
            created,
            r#type: Default::default(),
            category: Default::default(),
            metadata: Default::default(),
            blurb: "".to_string(),
            description: "".to_string(),
            player_tags: vec![],
            game_tags: vec![],
            team_tags: vec![],
            sim,
            day,
            season,
            tournament,
            phase,
            nuts,
        });

        builder.0.metadata.other = serde_json::json!({});

        builder
    }

    pub fn set_category(&mut self, category: EventCategory) {
        self.0.category = category;
    }

    pub fn set_game(&mut self, game: GameEvent) {
        self.0.game_tags = vec![game.game_id];
        self.0.team_tags = vec![game.away_team, game.home_team];
        self.0.metadata.play = Some(game.play);
        // Root events of games are always -1, non-games are null
        self.0.metadata.sub_play = Some(-1);

        if let Some(unscatter) = game.unscatter {
            self.push_child(unscatter.sub_event, |mut child| {
                child.push_description(&format!("{} was Unscattered.", unscatter.player_name));
                child.push_player_tag(unscatter.player_id);
                child.push_team_tag(unscatter.team_id);
                child.push_metadata_str("mod", "SCATTERED");
                child.push_metadata_i64("type", 0);
                child.build(EventType::RemovedMod)
            });
        }

        if let Some(attractor) = game.attractor_secret_base {
            self.push_description(&format!("{} enters the Secret Base...", attractor.player_name));
            self.push_player_tag(attractor.player_id)
        }
    }

    pub fn push_child<F>(&mut self, sub_event: SubEvent, build_func: F) where F: FnOnce(Self) -> EventuallyEvent {
        let mut child_builder = Self::new(sub_event.id, sub_event.created, self.0.sim.clone(), self.0.day, self.0.season, self.0.tournament, self.0.phase, sub_event.nuts);
        // Childrens' categories are usually Changes
        child_builder.0.category = EventCategory::Changes;
        child_builder.0.metadata.parent = Some(self.0.id);
        child_builder.0.game_tags = self.0.game_tags.clone();
        child_builder.0.metadata.play = self.0.metadata.play;
        child_builder.0.metadata.sub_play = Some(self.0.metadata.children.len() as i64);
        self.0.metadata.children.push(build_func(child_builder))
    }

    pub fn push_description(&mut self, desc: &str) {
        if !self.0.description.is_empty() {
            self.0.description.push('\n');
        }
        self.0.description += desc.into();
    }

    pub fn push_player_tag(&mut self, player_id: Uuid) {
        self.0.player_tags.push(player_id)
    }

    pub fn push_team_tag(&mut self, team_id: Uuid) {
        self.0.team_tags.push(team_id)
    }

    fn metadata_mut(&mut self) -> &mut Map<String, Value> {
        self.0.metadata.other
            .as_object_mut()
            .expect("Internal error: This metadata should always be an object")
    }

    pub fn push_metadata_str(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata_mut()
            .insert(key.into(), Value::String(value.into()));
    }

    pub fn push_metadata_str_vec(&mut self, key: impl Into<String>, value: Vec<String>) {
        self.metadata_mut()
            .insert(key.into(), value.into());
    }

    pub fn push_metadata_json_vec(&mut self, key: impl Into<String>, value: Vec<Value>) {
        self.metadata_mut()
            .insert(key.into(), value.into());
    }

    pub fn push_metadata_uuid(&mut self, key: impl Into<String>, value: Uuid) {
        self.metadata_mut()
            .insert(key.into(), Value::String(value.to_string()));
    }

    pub fn push_metadata_i64(&mut self, key: impl Into<String>, value: impl Into<i64>) {
        self.metadata_mut()
            .insert(key.into(), value.into().into());
    }

    pub fn push_metadata_f64_forced(&mut self, key: impl Into<String>, value: f64) {
        self.metadata_mut()
            .insert(key.into(), value.into());
    }

    pub fn push_metadata_f64(&mut self, key: impl Into<String>, value: f64) {
        // JS, or JSON, or Blaseball, or some layer in the stack does this annoying thing where
        // 0-valued floats are represented as ints, and my diffing cares about the difference
        if value == 0. {
            self.push_metadata_i64(key, 0)
        } else {
            self.push_metadata_f64_forced(key, value)
        }
    }

    pub fn push_gained_item(&mut self, player_name: String, gained_item: ItemGained) {
        if let Some(lost_item) = gained_item.dropped_item {
            let dropped_or_ditched = if lost_item.item_was_broken { "ditched" } else { "dropped" };
            self.push_description(&format!("{player_name} gained {} and {dropped_or_ditched} {}.",
                                           gained_item.item_name, lost_item.item_name));
            self.push_child(lost_item.sub_event, |mut child| {
                child.set_category(EventCategory::Changes);
                child.push_description(&format!("{player_name} dropped {}.", lost_item.item_name));
                child.push_player_tag(gained_item.player_id);
                child.push_team_tag(gained_item.team_id);
                child.push_metadata_uuid("itemId", lost_item.item_id);
                child.push_metadata_str("itemName", lost_item.item_name);
                child.push_metadata_str_vec("mods", lost_item.item_mods);
                child.push_metadata_f64("playerItemRatingAfter", lost_item.player_item_rating_after);
                child.push_metadata_f64("playerItemRatingBefore", lost_item.player_item_rating_before);
                child.push_metadata_f64("playerRating", gained_item.player_rating);
                child.build(EventType::PlayerLostItem)
            });
        } else {
            self.push_description(&format!("{player_name} gained {}.", gained_item.item_name));
        }

        self.push_child(gained_item.sub_event, |mut child| {
            child.set_category(EventCategory::Changes);
            child.push_description(&format!("{player_name} gained {}.", gained_item.item_name));
            child.push_player_tag(gained_item.player_id);
            child.push_team_tag(gained_item.team_id);
            child.push_metadata_uuid("itemId", gained_item.item_id);
            child.push_metadata_str("itemName", gained_item.item_name);
            child.push_metadata_str_vec("mods", gained_item.item_mods);
            child.push_metadata_f64("playerItemRatingAfter", gained_item.player_item_rating_after);
            child.push_metadata_f64("playerItemRatingBefore", gained_item.player_item_rating_before);
            child.push_metadata_f64("playerRating", gained_item.player_rating);
            child.build(EventType::PlayerGainedItem)
        });
    }

    pub fn push_named_item_damage(&mut self, item_damage: Option<(String, ItemDamage)>) {
        if let Some((player_name, dmg)) = item_damage {
            self.push_item_damage_impl(dmg, &player_name);
        }
    }

    pub fn push_item_damage(&mut self, item_damage: Option<ItemDamage>, player_name: &str) {
        if let Some(dmg) = item_damage {
            self.push_item_damage_impl(dmg, player_name);
        }
    }

    fn push_item_damage_impl(&mut self, dmg: ItemDamage, player_name: &str) {
        let description = format!("{} {} {}", Possessive(player_name), dmg.item_name,
                                  if dmg.health == 0 { "broke!" } else { "was damaged." });
        self.push_description(&description);
        self.push_child(dmg.sub_event, |mut child| {
            child.push_description(&description);
            child.push_player_tag(dmg.player_id);
            child.push_team_tag(dmg.team_id);
            child.push_metadata_i64("itemDurability", dmg.durability);
            child.push_metadata_i64("itemHealthAfter", dmg.health);
            child.push_metadata_i64("itemHealthBefore", dmg.health + 1);
            child.push_metadata_uuid("itemId", dmg.item_id);
            child.push_metadata_str("itemName", dmg.item_name);
            child.push_metadata_str_vec("mods", dmg.item_mods);
            child.push_metadata_f64("playerItemRatingAfter", dmg.player_item_rating_after);
            child.push_metadata_f64("playerItemRatingBefore", dmg.player_item_rating_before);
            child.push_metadata_f64("playerRating", dmg.player_rating);
            child.build(if dmg.health == 0 { EventType::ItemBreaks } else { EventType::ItemDamaged })
        })
    }

    pub fn push_stopped_inhabiting(&mut self, stopped_inhabiting: Option<StoppedInhabiting>) {
        let Some(si) = stopped_inhabiting else { return; };
        self.push_child(si.sub_event, |mut child| {
            child.push_description(&format!("{} stopped Inhabiting.", si.inhabiting_player_name));
            child.push_player_tag(si.inhabiting_player_id);
            if let Some(team_id) = si.inhabiting_player_team_id {
                child.push_team_tag(team_id);
            }
            child.push_metadata_str("mod", "INHABITING");
            child.push_metadata_i64("type", ModDuration::Permanent as i64);
            child.build(EventType::RemovedMod)
        })
    }

    pub fn push_free_refills(&mut self, free_refills: impl IntoIterator<Item=FreeRefill>) {
        for fr in free_refills.into_iter() {
            let common_description = format!("{} used their Free Refill.", fr.player_name);
            self.push_description(&common_description);
            self.push_description(&format!("{} Refills the In!", fr.player_name));
            self.push_child(fr.sub_event, |mut child| {
                child.push_description(&common_description);
                child.push_player_tag(fr.player_id);
                if let Some(t) = fr.team_id { child.push_team_tag(t) };
                child.push_metadata_str("mod", "COFFEE_RALLY");
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(EventType::RemovedMod)
            });

            // If there's any free refill, the event is Special
            self.set_category(EventCategory::Special);
        }
    }

    // This function only exists to make a more sensible name for the user. Option implements
    // IntoIterator so you could just call the plural form with an option.
    pub fn push_free_refill(&mut self, free_refills: Option<FreeRefill>) {
        self.push_free_refills(free_refills)
    }

    pub fn push_scores(&mut self, scores: Scores, score_label: &str, stopped_inhabiting: Option<StoppedInhabiting>) {
        self.push_scorers(scores.scores, score_label);
        self.push_stopped_inhabiting(stopped_inhabiting);
        self.push_free_refills(scores.free_refills);
    }

    pub fn push_attraction(&mut self, attraction: Option<Attraction>, player_name: &str, player_id: Uuid) {
        let Some(at) = attraction else { return };
        self.push_player_tag(player_id);
        self.push_description(&format!("The {} Attract {player_name}!", at.team_nickname));
        self.push_child(at.sub_event, |mut child| {
            child.push_description(&format!("The {} Attracted {player_name}!", at.team_nickname));
            child.push_player_tag(player_id);
            child.push_team_tag(at.team_id);
            child.push_metadata_i64("location", 2); // Shadows, I don't have an enum for that yet
            child.push_metadata_uuid("playerId", player_id);
            child.push_metadata_str("playerName", player_name);
            child.push_metadata_uuid("teamId", at.team_id);
            child.push_metadata_str("teamName", at.team_nickname);
            child.build(EventType::PlayerAddedToTeam)
        });
    }

    pub fn push_scorers(&mut self, scorers: Vec<ScoringPlayer>, score_label: &str) {
        for scorer in scorers {
            self.push_player_tag(scorer.player_id);
            self.push_description(&format!("{} {score_label}", scorer.player_name));
            self.push_attraction(scorer.attraction, &scorer.player_name, scorer.player_id);
        }
    }

    pub fn push_spicy(&mut self, spicy: SpicyStatus, player_name: &str, player_id: Uuid) {
        match spicy {
            SpicyStatus::None => {}
            SpicyStatus::HeatingUp => {
                self.push_description(&format!("{player_name} is Heating Up!"));
                self.push_player_tag(player_id);
            }
            SpicyStatus::RedHot(mod_added) => {
                let description = format!("{player_name} is Red Hot!");
                self.push_description(&description);
                self.push_player_tag(player_id);
                self.set_category(EventCategory::Special);
                if let Some(mod_added) = mod_added {
                    self.push_child(mod_added.sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_player_tag(player_id);
                        child.push_team_tag(mod_added.team_id);
                        child.push_metadata_str("mod", "ON_FIRE");
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(EventType::AddedMod)
                    })
                }
            }
        }
    }

    pub fn build_item_repaired(mut self, item_repaired: ItemRepaired) -> EventuallyEvent {
        self.push_player_tag(item_repaired.player_id);
        self.push_team_tag(item_repaired.team_id);
        self.push_metadata_i64("itemDurability", item_repaired.durability);
        self.push_metadata_i64("itemHealthAfter", item_repaired.health);
        self.push_metadata_i64("itemHealthBefore", item_repaired.health - 1);
        self.push_metadata_uuid("itemId", item_repaired.item_id);
        self.push_metadata_str("itemName", item_repaired.item_name);
        self.push_metadata_str_vec("mods", item_repaired.item_mods);
        self.push_metadata_f64("playerItemRatingAfter", item_repaired.player_item_rating_after);
        self.push_metadata_f64("playerItemRatingBefore", item_repaired.player_item_rating_before);
        self.push_metadata_f64("playerRating", item_repaired.player_rating);
        self.build(if item_repaired.health == 1 {
            EventType::BrokenItemRepaired
        } else {
            EventType::DamagedItemRepaired
        })
    }

    pub fn build(mut self, event_type: EventType) -> EventuallyEvent {
        self.0.r#type = event_type;
        self.0
    }
}