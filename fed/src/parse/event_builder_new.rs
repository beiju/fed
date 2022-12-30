use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;
use eventually_api::{EventCategory, EventType, EventuallyEvent};
use crate::{GameEvent, ItemGained, SubEvent};

pub struct EventBuilder(EventuallyEvent);


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

    pub fn push_metadata_uuid(&mut self, key: impl Into<String>, value: Uuid) {
        self.metadata_mut()
            .insert(key.into(), Value::String(value.to_string()));
    }

    pub fn push_metadata_i64(&mut self, key: impl Into<String>, value: i64) {
        self.metadata_mut()
            .insert(key.into(), value.into());
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
            self.push_description(&format!("{player_name} gained {} and dropped {}.",
                                          gained_item.item_name, lost_item.item_name));
            self.push_child(lost_item.sub_event, |mut child| {
                child.set_category(EventCategory::Changes);
                child.push_description(&format!("{player_name} dropped {}.", lost_item.item_name));
                child.push_player_tag(gained_item.player_id);
                child.push_team_tag(gained_item.team_id);
                child.push_metadata_uuid("itemId",lost_item.item_id);
                child.push_metadata_str("itemName",lost_item.item_name);
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
            child.push_metadata_uuid("itemId",gained_item.item_id);
            child.push_metadata_str("itemName",gained_item.item_name);
            child.push_metadata_str_vec("mods", gained_item.item_mods);
            child.push_metadata_f64("playerItemRatingAfter", gained_item.player_item_rating_after);
            child.push_metadata_f64("playerItemRatingBefore", gained_item.player_item_rating_before);
            child.push_metadata_f64("playerRating", gained_item.player_rating);
            child.build(EventType::PlayerGainedItem)
        });

    }

    pub fn build(mut self, event_type: EventType) -> EventuallyEvent {
        self.0.r#type = event_type;
        self.0
    }
}