use std::fmt::{Display, Formatter};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use crate::{Attraction, AttractionWithPlayer, BatterDebt, DetectiveActivity, FreeRefill, GameEvent, GamePitch, HotelMotelScoringPlayer, Hype, ItemDamaged, ItemGained, ItemRepaired, KnownPlayerStatChange, MaintenanceMode, ModChangeSubEvent, ModChangeSubEventWithPlayer, ModDuration, Parasite, PlayerBoostSubEvent, PlayerBoostSubEventWithTeam, PlayerNameId, PlayerSentElsewhere, ScoreEvent, Scores, ScoringPlayer, SpicyStatus, StoppedInhabiting, SubEvent};

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
            election_option_id: None,
            player_tags: Some(vec![]),
            game_tags: Some(vec![]),
            team_tags: Some(vec![]),
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

    pub fn connected_event(&self, sub_event: SubEvent) -> Self {
        Self(EventuallyEvent {
            id: sub_event.id,
            created: sub_event.created,
            nuts: sub_event.nuts,
            ..self.0.clone()
        })
    }

    pub fn description(&self) -> &str {
        &self.0.description
    }

    pub fn set_description(&mut self, description: String) {
        self.0.description = description;
    }

    pub fn set_category(&mut self, category: EventCategory) {
        self.0.category = category;
    }

    pub fn set_game(&mut self, game: GameEvent) {
        self.0.game_tags = Some(vec![game.game_id]);
        self.0.team_tags = Some(vec![game.away_team, game.home_team]);
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
            self.set_category(EventCategory::Special);
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

    pub fn clear_sub_play(&mut self) {
        self.0.metadata.sub_play = None;
    }

    pub fn push_description(&mut self, desc: &str) {
        if !self.0.description.is_empty() {
            self.0.description.push('\n');
        }
        self.0.description += desc.into();
    }

    pub fn push_player_tag(&mut self, player_id: Uuid) {
        self.0.player_tags.as_mut()
            .expect("Builder should not be used for events with no player tags")
            .push(player_id)
    }

    pub fn push_team_tag(&mut self, team_id: Uuid) {
        self.0.team_tags.as_mut()
            .expect("Builder should not be used for events with no team tags")
            .push(team_id)
    }

    pub fn set_team_tags(&mut self, team_tags: Vec<Uuid>) {
        self.0.team_tags = Some(team_tags);
    }

    fn metadata_mut(&mut self) -> &mut Map<String, Value> {
        self.0.metadata.other
            .as_object_mut()
            .expect("Internal error: This metadata should always be an object")
    }

    pub fn set_full_metadata(&mut self, metadata: EventMetadata) {
        self.0.metadata = metadata;
    }

    pub fn push_metadata_null(&mut self, key: impl Into<String>) {
        self.metadata_mut()
            .insert(key.into(), Value::Null);
    }

    pub fn push_metadata_str(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata_mut()
            .insert(key.into(), Value::String(value.into()));
    }

    pub fn push_metadata_str_vec(&mut self, key: impl Into<String>, value: Vec<String>) {
        self.metadata_mut()
            .insert(key.into(), value.into());
    }

    pub fn push_metadata_json(&mut self, key: impl Into<String>, value: Value) {
        self.metadata_mut().insert(key.into(), value);
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

    pub fn push_metadata_i32(&mut self, key: impl Into<String>, value: impl Into<i32>) {
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

    pub fn push_metadata_i64_or_f64(&mut self, key: impl Into<String>, value: f64) {
        // JS, or JSON, or Blaseball, or some layer in the stack does this annoying thing where
        // int-valued floats are represented as ints, and my diffing cares about the difference
        let value_int = value as i64;
        if value == value_int as f64 {
            self.push_metadata_i64(key, value_int)
        } else {
            self.push_metadata_f64_forced(key, value)
        }
    }

    pub fn push_metadata_f64_opt(&mut self, key: impl Into<String>, value: Option<f64>) {
        if let Some(n) = value {
            self.push_metadata_f64(key, n)
        } else {
            self.push_metadata_null(key)
        }
    }

    pub fn push_known_boost(&mut self, boost: &KnownPlayerStatChange) {
        self.push_metadata_f64("before", boost.rating_before);
        self.push_metadata_f64("after", boost.rating_after);
        self.push_metadata_i64("type", 4); // TODO what does this mean?
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

    pub fn push_named_item_damage(&mut self, item_damage: Option<(&str, &ItemDamaged)>) {
        if let Some((player_name, dmg)) = item_damage {
            self.push_item_damage(dmg, player_name);
        }
    }

    pub fn push_named_item_damages<'a>(&mut self, item_damages: impl IntoIterator<Item=(&'a str, &'a ItemDamaged)>) {
        for (player_name, dmg) in item_damages {
            self.push_item_damage(dmg, player_name);
        }
    }

    pub fn push_opt_item_damage(&mut self, dmg: Option<&ItemDamaged>, player_name: &str) {
        if let Some(d) = dmg {
            self.push_item_damage(d, player_name)
        }
    }

    pub fn push_item_damage(&mut self, dmg: &ItemDamaged, player_name: &str) {
        let description = format!("{}{} {dmg}",
                                  // bug-for-bug compatibility :)
                                  if (self.0.season, self.0.day) < (15, 3) { " " } else { "" },
                                  Possessive(player_name));
        self.push_description(&description);
        // In season 17 days 7-10 inclusive, the Ambitious event type was accidentally used instead
        // of ItemBreaks
        let use_ambitious = self.0.season == 17 && self.0.day >= 7 && self.0.day <= 10;
        self.push_child(dmg.sub_event, |mut child| {
            child.push_description(&description);
            child.push_player_tag(dmg.player_id);
            child.push_team_tag(dmg.team_id);
            child.push_metadata_i64("itemDurability", dmg.durability);
            child.push_metadata_i64("itemHealthAfter", dmg.health);
            child.push_metadata_i64("itemHealthBefore", dmg.health + 1);
            child.push_metadata_uuid("itemId", dmg.item_id);
            child.push_metadata_str("itemName", &dmg.item_name);
            child.push_metadata_str_vec("mods", dmg.item_mods.clone());
            child.push_metadata_f64_opt("playerItemRatingAfter", dmg.player_item_rating_after);
            child.push_metadata_f64_opt("playerItemRatingBefore", dmg.player_item_rating_before);
            child.push_metadata_f64("playerRating", dmg.player_rating);
            child.build(if dmg.health == 0 {
                if use_ambitious {
                    EventType::Ambitious
                } else {
                    EventType::ItemBreaks
                }
            } else {
                EventType::ItemDamaged
            })
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

    pub fn push_scores(&mut self, scores: Scores, home_team_id: Uuid, score_label: &str) {
        self.push_scorers(scores.scores, home_team_id, score_label);
        self.push_free_refills(scores.free_refills);
    }

    pub fn push_score_event(&mut self, score: &ScoreEvent) {
        self.push_child(score.sub_event, |mut child_eb| {
            child_eb.set_category(EventCategory::Game);
            child_eb.push_team_tag(score.team_id);
            child_eb.push_description(&format!("The {} scored!", score.team_nickname));
            child_eb.push_metadata_str("awayEmoji", &score.away_emoji);
            child_eb.push_metadata_i64_or_f64("awayScore", score.away_score);
            child_eb.push_metadata_str("homeEmoji", &score.home_emoji);
            child_eb.push_metadata_i64_or_f64("homeScore", score.home_score);
            child_eb.push_metadata_str("ledger", ""); // Is this ever nonempty?
            child_eb.push_metadata_str("update", if score.runs_scored == 1.0 {
                "1 Run scored!".to_string()
            } else if score.runs_scored < 0.0 {
                format!("{} Unruns scored!", -score.runs_scored)
            } else {
                format!("{} Runs scored!", score.runs_scored)
            });
            child_eb.build(EventType::RunsScored)
        });
    }

    pub fn push_attraction(&mut self, attraction: &Attraction, player_name: &str, player_id: Uuid) {
        self.push_player_tag(player_id);
        self.push_description(&format!("The {} Attract {player_name}!", attraction.team_nickname));
        self.push_child(attraction.sub_event, |mut child| {
            child.push_description(&format!("The {} Attracted {player_name}!", attraction.team_nickname));
            child.push_player_tag(player_id);
            child.push_team_tag(attraction.team_id);
            child.push_metadata_i64("location", 2); // Shadows, I don't have an enum for that yet
            child.push_metadata_uuid("playerId", player_id);
            child.push_metadata_str("playerName", player_name);
            child.push_metadata_uuid("teamId", attraction.team_id);
            child.push_metadata_str("teamName", &attraction.team_nickname);
            child.build(EventType::PlayerAddedToTeam)
        });
        if let Some(boost) = &attraction.boost {
            self.push_child(boost.sub_event, |mut child| {
                child.push_description(&format!("{player_name} entered the Shadows."));
                child.push_player_tag(player_id);
                child.push_team_tag(attraction.team_id);
                child.build_boost(boost)
            })
        }
    }

    pub fn push_hotel_motel_party(&mut self, hotel_motel_party: &PlayerBoostSubEventWithTeam, player_name: &str, player_id: Uuid) {
        self.push_player_tag(player_id);
        let description = format!("{player_name} is Partying!");
        self.push_description(&description);
        self.push_child(hotel_motel_party.sub_event, |mut child| {
            child.push_description(&description);
            child.push_player_tag(player_id);
            child.build_boost_with_team(hotel_motel_party)
        })
    }

    pub fn push_attraction_with_player(&mut self, attraction: Option<AttractionWithPlayer>) {
        let Some(at) = attraction else { return; };
        self.push_player_tag(at.player_id);
        self.push_description(&format!("The {} Attract {}!", at.team_nickname, at.player_name));
        self.push_child(at.sub_event, |mut child| {
            child.push_description(&format!("The {} Attracted {}!", at.team_nickname, at.player_name));
            child.push_player_tag(at.player_id);
            child.push_team_tag(at.team_id);
            child.push_metadata_i64("location", 2); // Shadows, I don't have an enum for that yet
            child.push_metadata_uuid("playerId", at.player_id);
            child.push_metadata_str("playerName", at.player_name);
            child.push_metadata_uuid("teamId", at.team_id);
            child.push_metadata_str("teamName", at.team_nickname);
            child.build(EventType::PlayerAddedToTeam)
        });
    }

    pub fn push_scorers(&mut self, scorers: Vec<ScoringPlayer>, home_team_id: Uuid, score_label: &str) {
        // Base scores
        for scorer in &scorers {
            self.push_player_tag(scorer.player_id);
            self.push_hype_opt(scorer.hype.as_ref(), home_team_id);
            if let Some(damage) = &scorer.item_damage {
                self.push_item_damage(damage, &scorer.player_name);
            }
            self.push_description(&format!("{} {score_label}", scorer.player_name));
            if let Some(score_event) = &scorer.score_event {
                self.push_score_event(score_event);
            }
        }
        // Attractions happen in a block after the scores block
        for scorer in &scorers {
            if let Some(attraction) = &scorer.attraction {
                self.push_attraction(attraction, &scorer.player_name, scorer.player_id);
            }
        }
        // Hotel motel parties happen in a block after the scores block (not sure of order w/r/t
        // attractions)
        for scorer in &scorers {
            if let Some(party) = &scorer.hotel_motel_party {
                self.push_hotel_motel_party(party, &scorer.player_name, scorer.player_id)
            }
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

    pub fn push_cooled_off(&mut self, cooled_off: Option<ModChangeSubEventWithPlayer>, player_name: &str) {
        if let Some(co) = cooled_off {
            let description = format!("{player_name} cooled off.");
            self.push_description(&description);
            self.push_player_tag(co.player_id);
            self.set_category(EventCategory::Special);
            self.push_child(co.sub_event, |mut child| {
                child.push_description(&description);
                child.push_player_tag(co.player_id);
                child.push_team_tag(co.team_id);
                child.push_metadata_str("mod", "ON_FIRE");
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(EventType::RemovedMod)
            })
        }
    }

    pub fn push_batter_debt(&mut self, batter_debt: Option<BatterDebt>, batter_name: &str, fielder_name: &str) {
        if let Some(bd) = batter_debt {
            self.push_description(&format!("{batter_name} hit a ball at {fielder_name}..."));
            let common_description = format!("{fielder_name} is now being Observed.");
            self.push_description(&common_description);
            self.push_player_tag(bd.batter_id);
            self.push_player_tag(bd.fielder_id);
            self.set_category(EventCategory::Special);
            if let Some(mod_change) = bd.sub_event {
                // I tried extracting this as a method but I was passing all but one value in as a
                // separate parameter so it didn't make sense
                self.push_child(mod_change.sub_event, |mut child| {
                    child.push_description(&common_description);
                    child.push_player_tag(bd.fielder_id);
                    child.push_team_tag(mod_change.team_id);
                    child.push_metadata_str("mod", "COFFEE_PERIL");
                    child.push_metadata_i64("type", ModDuration::Weekly as i64);
                    child.build(EventType::AddedMod)
                })
            }
        }
    }

    pub fn push_pitch(&mut self, pitch: GamePitch) {
        if let Some(pitcher_name) = pitch.double_strike {
            self.set_category(EventCategory::Special);
            self.push_description(&format!("{pitcher_name} fires a Double Strike!"));
        }
        if let Some(pitcher_name) = pitch.acidic_pitch {
            self.push_description(&format!("{pitcher_name} throws an Acidic pitch!"));
        }
    }

    pub fn push_charge_blood(&mut self, power_charge: Option<ModChangeSubEvent>, batter_name: &str, batter_id: Uuid, a: &str) {
        if let Some(charge) = power_charge {
            let description = format!("{batter_name} Power Ch{a}rged!");
            self.push_description(&description);
            self.push_child(charge.sub_event, |mut child| {
                child.push_description(&description);
                child.push_player_tag(batter_id);
                child.push_team_tag(charge.team_id);
                child.push_metadata_str("mod", "OVERPERFORMING");
                child.push_metadata_str("source", a.to_ascii_uppercase());
                child.push_metadata_i64("type", ModDuration::Game as i64);
                child.build(EventType::AddedModFromOtherMod)
            })
        }
    }

    pub fn push_birds(&mut self, num_birds: Option<i32>) {
        if let Some(n) = num_birds {
            self.push_description(&format!("A new Bird finds a Birdhouse. {n}"));
        }
    }

    pub fn push_parasite(&mut self, parasite: Option<Parasite>) {
        if let Some(parasite) = parasite {
            self.push_description(&format!("{} parasitically drained some of {} {}.",
                                           parasite.pitcher_name, Possessive(&parasite.batter_name), parasite.attribute_name));
            self.push_description(&format!("{} boosted their {}!",
                                           parasite.pitcher_name, parasite.attribute_name));
            self.push_child(parasite.batter_sub_event, |mut child| {
                child.push_description(&format!("{} had blood drained by Parasite {}.",
                                                parasite.batter_name, parasite.pitcher_name));
                child.push_player_tag(parasite.batter_id);
                child.push_team_tag(parasite.batter_team_id);
                child.build_player_attribute_changed(parasite.batter_rating_before, parasite.batter_rating_after, parasite.attribute_id)
            });
            self.push_maintenance_mode(parasite.maintenance_mode);
            self.push_child(parasite.pitcher_sub_event, |mut child| {
                child.push_description(&format!("Parasite {} drained blood from {}.",
                                                parasite.pitcher_name, parasite.batter_name));
                child.push_player_tag(parasite.pitcher_id);
                child.push_team_tag(parasite.pitcher_team_id);
                child.build_player_attribute_changed(parasite.pitcher_rating_before, parasite.pitcher_rating_after, parasite.attribute_id)
            });
        }
    }

    pub fn push_magmatic(&mut self, magmatic: Option<ModChangeSubEvent>, batter_name: &str, batter_id: Uuid) {
        if let Some(mod_change) = magmatic {
            self.push_description(&format!("{batter_name} is Magmatic!"));
            self.push_child(mod_change.sub_event, |mut child| {
                child.push_description(&format!("{batter_name} hit a Magmatic home run!"));
                child.push_player_tag(batter_id);
                child.push_team_tag(mod_change.team_id);
                child.push_metadata_str("mod", "MAGMATIC");
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(EventType::RemovedMod)
            });
        }
    }

    pub fn push_hotel_motel(&mut self, parties: &[HotelMotelScoringPlayer]) {
        for party in parties {
            let description = format!("{} is Partying!", party.player_name);
            self.push_description(&description);
            self.push_player_tag(party.player_id);
            self.push_child(party.boost.sub_event, |mut child| {
                child.push_description(&description);
                child.push_player_tag(party.player_id);
                child.push_team_tag(party.team_id);
                child.build_boost(&party.boost)
            });
        }
    }

    pub fn push_gravity(&mut self, gravity_players: Vec<PlayerNameId>) {
        for player in gravity_players {
            self.push_description(&format!("{}'s Gravity kept them in place!", player.player_name));
            self.push_player_tag(player.player_id);
        }
    }

    pub fn push_maintenance_mode(&mut self, maintenance_mode: Option<MaintenanceMode>) {
        if let Some(maintenance_mode) = maintenance_mode {
            self.push_child(maintenance_mode.sub_event, |mut child| {
                child.push_description("Impairment Detected. Entering Maintenance Mode.");
                child.push_team_tag(maintenance_mode.team_id);
                child.push_metadata_str("mod", "EXTRA_OUT");
                child.push_metadata_i64("type", ModDuration::Game as i64);
                child.build(EventType::AddedMod)
            });
        }
    }

    pub fn push_hype_opt(&mut self, hype: Option<&Hype>, home_team_id: Uuid) {
        if let Some(h) = hype {
            self.push_hype(h, home_team_id);
        }
    }

    pub fn push_hype(&mut self, hype: &Hype, home_team_id: Uuid) {
        self.push_description("Shame!");
        self.push_description(&format!("Hype Builds in {}!", hype.stadium_name));
        self.push_child(hype.sub_event, |mut child_eb| {
            child_eb.set_category(EventCategory::Changes);
            // Love how the descriptions are slightly different
            child_eb.push_description(&format!("Hype built in {}!", hype.stadium_name));
            child_eb.push_team_tag(home_team_id);
            child_eb.push_metadata_f64("before", hype.hype_before);
            child_eb.push_metadata_f64("after", hype.hype_after);

            child_eb.build(EventType::HypeBuilds)
        });
    }

    pub fn push_sent_elsewhere(&mut self, sent_elsewhere: PlayerSentElsewhere, outer_description: &str, inner_description: &str) {
        self.push_description(outer_description);
        self.push_child(sent_elsewhere.sub_event, |mut child_self| {
            child_self.push_description(inner_description);
            child_self.push_team_tag(sent_elsewhere.team_id);
            child_self.push_player_tag(sent_elsewhere.player_id);
            child_self.push_metadata_str("mod", "ELSEWHERE");
            child_self.push_metadata_i64("type", ModDuration::Permanent);
            child_self.build(EventType::AddedMod)
        });

        if let Some(flip) = sent_elsewhere.flipped_negative {
            // First, undertaker also goes Elsewhere
            let undertaker_description = format!("{} dove in after {}.", flip.undertaker_player_name, sent_elsewhere.player_name);
            self.push_description(&undertaker_description);
            self.push_player_tag(flip.undertaker_player_id);
            self.push_child(flip.undertaker_elsewhere_sub_event, |mut child_self| {
                child_self.push_description(&undertaker_description);
                child_self.push_team_tag(sent_elsewhere.team_id);
                child_self.push_player_tag(flip.undertaker_player_id);
                child_self.push_metadata_str("mod", "ELSEWHERE");
                child_self.push_metadata_i64("type", ModDuration::Permanent);
                child_self.build(EventType::AddedMod)
            });

            // Then the actual flipping
            self.push_description(&format!("{} was flipped Negative!", sent_elsewhere.player_name));
            self.push_player_tag(sent_elsewhere.player_id);
            self.push_child(flip.flip_negative_sub_event, |mut child_self| {
                child_self.push_description(&format!("{} flipped {} Negative.", flip.undertaker_player_name, sent_elsewhere.player_name));
                child_self.push_team_tag(sent_elsewhere.team_id);
                child_self.push_player_tag(sent_elsewhere.player_id);
                child_self.push_metadata_str("mod", "NEGATIVE");
                child_self.push_metadata_i64("type", ModDuration::Permanent);
                child_self.build(EventType::AddedMod)
            });
        }
    }

    pub fn build_item_repaired(mut self, item_repaired: ItemRepaired) -> EventuallyEvent {
        self.push_player_tag(item_repaired.player_id);
        self.push_team_tag(item_repaired.team_id);
        self.push_metadata_i64("itemDurability", item_repaired.durability);
        self.push_metadata_i64("itemHealthAfter", item_repaired.health_after);
        self.push_metadata_i64("itemHealthBefore", item_repaired.health_before);
        self.push_metadata_uuid("itemId", item_repaired.item_id);
        self.push_metadata_str("itemName", item_repaired.item_name);
        self.push_metadata_str_vec("mods", item_repaired.item_mods);
        self.push_metadata_f64("playerItemRatingAfter", item_repaired.player_item_rating_after);
        self.push_metadata_f64("playerItemRatingBefore", item_repaired.player_item_rating_before);
        self.push_metadata_f64("playerRating", item_repaired.player_rating);
        // In season 17 days 7-10 inclusive, the Coasting event type was accidentally used instead
        // of BrokenItemRepaired
        let use_coasting = self.0.season == 17 && self.0.day >= 7 && self.0.day <= 10;
        self.build(if item_repaired.health_before == 0 {
            if use_coasting {
                EventType::Coasting
            } else {
                EventType::BrokenItemRepaired
            }
        } else {
            EventType::DamagedItemRepaired
        })
    }

    pub fn build_item_damaged(mut self, item_damaged: ItemDamaged) -> EventuallyEvent {
        self.push_player_tag(item_damaged.player_id);
        self.push_team_tag(item_damaged.team_id);
        self.push_metadata_i64("itemDurability", item_damaged.durability);
        self.push_metadata_i64("itemHealthAfter", item_damaged.health);
        self.push_metadata_i64("itemHealthBefore", item_damaged.health + 1);
        self.push_metadata_uuid("itemId", item_damaged.item_id);
        self.push_metadata_str("itemName", item_damaged.item_name);
        self.push_metadata_str_vec("mods", item_damaged.item_mods);
        self.push_metadata_f64_opt("playerItemRatingAfter", item_damaged.player_item_rating_after);
        self.push_metadata_f64_opt("playerItemRatingBefore", item_damaged.player_item_rating_before);
        self.push_metadata_f64("playerRating", item_damaged.player_rating);
        self.build(if item_damaged.health == 0 {
            EventType::ItemBreaks
        } else {
            EventType::ItemDamaged
        })
    }

    pub fn build_player_stat_changed(mut self, rating_before: f64, rating_after: f64, attribute_type: i64) -> EventuallyEvent {
        self.push_metadata_f64("before", rating_before);
        self.push_metadata_f64("after", rating_after);
        self.push_metadata_i64("type", attribute_type);
        self.build(if rating_after > rating_before {
            EventType::PlayerStatIncrease
        } else {
            EventType::PlayerStatDecrease
        })
    }

    // TODO What the fheck is the difference from StatIncrease
    pub fn build_player_attribute_changed(mut self, rating_before: f64, rating_after: f64, attribute_type: i64) -> EventuallyEvent {
        self.push_metadata_f64("before", rating_before);
        self.push_metadata_f64("after", rating_after);
        self.push_metadata_i64("type", attribute_type);
        self.build(if rating_after > rating_before {
            EventType::PlayerAttributeIncrease
        } else {
            EventType::PlayerAttributeDecrease
        })
    }

    pub fn build_boost(self, boost: &PlayerBoostSubEvent) -> EventuallyEvent {
        self.build_player_stat_changed(boost.rating_before, boost.rating_after, 4)
    }

    pub fn build_boost_with_team(mut self, boost: &PlayerBoostSubEventWithTeam) -> EventuallyEvent {
        self.push_team_tag(boost.team_id);
        self.build_player_stat_changed(boost.rating_before, boost.rating_after, 4)
    }

    pub fn build_detective_activity(mut self, activity: DetectiveActivity) -> EventuallyEvent {
        self.set_category(EventCategory::Special);
        self.push_player_tag(activity.detective_id);
        self.build(EventType::InvestigationMessage)
    }

    pub fn build(mut self, event_type: EventType) -> EventuallyEvent {
        self.0.r#type = event_type;
        self.0
    }
}