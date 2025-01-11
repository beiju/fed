use std::fmt::Write;

use crate::format_utils::Possessive;
use crate::{Attraction, AttractionWithPlayer, BalloonsPopped, BatterDebt, BracketType, DebtType, DetectiveActivity, EarnedWin, FlipNegative, FreeRefill, GameEvent, GamePitch, HotelMotelParty, HotelMotelScoringPlayer, Hype, ItemDamaged, ItemDroppedForNewItem, ItemGained, ItemRepaired, KnownPlayerStatChange, LedgerV2, MaintenanceMode, ModChangeSubEvent, ModChangeSubEventWithPlayer, ModDuration, Parasite, PlayerBoostSubEvent, PlayerBoostSubEventWithTeam, PlayerModChangeSubject, PlayerMovedTeams, PlayerNameId, PlayerSentElsewhere, Scattered, ScoreSummary, Scores, ScoringPlayer, SpicyStatus, StoppedInhabiting, SubEvent, SubseasonalMod, SubseasonalModChange, TeamModChangeSubject};
use chrono::{DateTime, Utc};
use eventually_api::{EventCategory, EventMetadata, EventType, EventuallyEvent};
use serde_json::{Map, Value};
use uuid::Uuid;

pub struct EventBuilder {
    event: EventuallyEvent,
    phantom_children: i64,
}

fn reverse_performing(input: &str) -> &'static str {
    if input == "OVERPERFORMING" {
        "UNDERPERFORMING"
    } else {
        "OVERPERFORMING"
    }
}

impl EventBuilder {
    pub fn new(id: Uuid, created: DateTime<Utc>, sim: String, day: i64, season: i64, tournament: i64, phase: i64, nuts: i64) -> Self {
        let mut builder = Self {
            event: EventuallyEvent {
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
            },
            phantom_children: 0,
        };

        builder.event.metadata.other = serde_json::json!({});

        builder
    }

    pub fn connected_event(&self, sub_event: SubEvent) -> Self {
        Self {
            event: EventuallyEvent {
                id: sub_event.id,
                created: sub_event.created,
                nuts: sub_event.nuts,
                metadata: self.event.metadata.connected_event_metadata(),
                ..self.event.clone()
            },
            phantom_children: 0,
        }
    }

    pub fn description(&self) -> &str {
        &self.event.description
    }

    pub fn set_description(&mut self, description: String) {
        self.event.description = description;
    }

    pub fn set_category(&mut self, category: EventCategory) {
        self.event.category = category;
    }

    pub fn set_game(&mut self, game: GameEvent) {
        self.event.game_tags = Some(vec![game.game_id]);
        self.event.team_tags = Some(vec![game.away_team, game.home_team]);
        self.event.metadata.play = Some(game.play);
        // Root events of games are always -1, non-games are null
        self.event.metadata.sub_play = Some(-1);

        if let Some(unscatter) = game.unscatter {
            self.push_child(unscatter.sub_event, |mut child| {
                child.push_description(format!("{} was Unscattered.", unscatter.player_name));
                child.push_player_tag(unscatter.player_id);
                child.push_team_tag(unscatter.team_id);
                child.push_metadata_str("mod", "SCATTERED");
                child.push_metadata_i64("type", 0);
                child.build(EventType::RemovedMod)
            });
        }

        if let Some(attractor) = game.attractor_secret_base {
            self.set_category(EventCategory::Special);
            self.push_description(format!("{} enters the Secret Base...", attractor.player_name));
            self.push_player_tag(attractor.player_id)
        }

        // This is probably going to need to be treated specially because I assume it goes at the
        // end of the child list, but for now pretend it's at the beginning
        if let Some(trader_trade) = game.trader_trade {
            self.push_child(trader_trade.victim_lost_item_sub_event, |mut child_eb| {
                child_eb.push_description(format!("{} traded away {} to {} for {}.", trader_trade.victim_name, trader_trade.stolen_item_name, trader_trade.trader_name, trader_trade.exchanged_item_name.as_deref().unwrap_or("nothing")));
                child_eb.push_player_tag(trader_trade.victim_id);
                child_eb.push_team_tag(trader_trade.victim_team_id);
                child_eb.push_metadata_uuid("itemId", trader_trade.stolen_item_id);
                child_eb.push_metadata_str("itemName", &trader_trade.stolen_item_name);
                child_eb.push_metadata_str_vec("mods", trader_trade.stolen_item_mods.clone());
                child_eb.push_metadata_f64("playerItemRatingBefore", trader_trade.victim_item_rating_before);
                child_eb.push_metadata_f64("playerItemRatingAfter", trader_trade.victim_item_rating_after);
                child_eb.push_metadata_f64("playerRating", trader_trade.victim_rating);
                child_eb.build(EventType::PlayerLostItem)
            });

            self.push_child(trader_trade.trader_gained_item_sub_event, |mut child_eb| {
                child_eb.push_description(format!("{} traded their {} for {} {}.", trader_trade.trader_name, trader_trade.exchanged_item_name.as_deref().unwrap_or("nothing"), Possessive(&trader_trade.victim_name), trader_trade.stolen_item_name));
                child_eb.push_player_tag(trader_trade.trader_id);
                child_eb.push_team_tag(trader_trade.trader_team_id);
                child_eb.push_metadata_uuid("itemId", trader_trade.stolen_item_id);
                child_eb.push_metadata_str("itemName", trader_trade.stolen_item_name);
                child_eb.push_metadata_str_vec("mods", trader_trade.stolen_item_mods);
                child_eb.push_metadata_f64("playerItemRatingBefore", trader_trade.trader_item_rating_before);
                child_eb.push_metadata_f64("playerItemRatingAfter", trader_trade.trader_item_rating_after);
                child_eb.push_metadata_f64("playerRating", trader_trade.trader_rating);
                child_eb.build(EventType::PlayerGainedItem)
            });
        }
    }

    pub fn push_child<F>(&mut self, sub_event: SubEvent, build_func: F) where F: FnOnce(Self) -> EventuallyEvent {
        let mut child_builder = Self::new(sub_event.id, sub_event.created, self.event.sim.clone(), self.event.day, self.event.season, self.event.tournament, self.event.phase, sub_event.nuts);
        // Childrens' categories are usually Changes
        child_builder.event.category = EventCategory::Changes;
        child_builder.event.metadata.parent = Some(self.event.id);
        child_builder.event.game_tags = self.event.game_tags.clone();
        child_builder.event.metadata.play = self.event.metadata.play;
        child_builder.event.metadata.sub_play = Some(self.event.metadata.children.len() as i64 + self.phantom_children);
        self.event.metadata.children.push(build_func(child_builder))
    }

    pub fn push_phantom_child(&mut self) {
        self.phantom_children += 1;
    }

    pub fn clear_sub_play(&mut self) {
        self.event.metadata.sub_play = None;
    }

    pub fn push_description<'a>(&mut self, desc: impl std::fmt::Display) {
        if !self.event.description.is_empty() {
            self.event.description.push('\n');
        }
        write!(self.event.description, "{desc}")
            .expect("Write on &mut String can't fail");
    }

    pub fn push_player_tag(&mut self, player_id: Uuid) {
        self.event.player_tags.as_mut()
            .expect("Builder should not be used for events with no player tags")
            .push(player_id)
    }

    pub fn push_team_tag(&mut self, team_id: Uuid) {
        self.event.team_tags.as_mut()
            .expect("Builder should not be used for events with no team tags")
            .push(team_id)
    }

    pub fn set_team_tags(&mut self, team_tags: Vec<Uuid>) {
        self.event.team_tags = Some(team_tags);
    }

    fn metadata_mut(&mut self) -> &mut Map<String, Value> {
        self.event.metadata.other
            .as_object_mut()
            .expect("Internal error: This metadata should always be an object")
    }

    pub fn set_full_metadata(&mut self, metadata: EventMetadata) {
        self.event.metadata = metadata;
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

    pub fn push_gained_item(&mut self, player_name: &str, gained_item: ItemGained) {
        if let Some(lost_item) = gained_item.dropped_item {
            let dropped_or_ditched = if lost_item.item_was_broken { "ditched" } else { "dropped" };
            self.push_description(format!("{player_name} gained {} and {dropped_or_ditched} {}.",
                                           gained_item.item_name, lost_item.item_name));
            self.push_dropped_item(&player_name, gained_item.player_id, gained_item.team_id, gained_item.player_rating, lost_item);
        } else {
            self.push_description(format!("{player_name} gained {}.", gained_item.item_name));
        }

        self.push_child(gained_item.sub_event, |mut child| {
            child.set_category(EventCategory::Changes);
            child.push_description(format!("{player_name} gained {}.", gained_item.item_name));
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

    pub fn push_dropped_item(&mut self, player_name: &str, player_id: Uuid, team_id: Uuid, player_rating: f64, dropped_item: ItemDroppedForNewItem) {
        self.push_child(dropped_item.sub_event, |mut child| {
            child.set_category(EventCategory::Changes);
            child.push_description(format!("{player_name} dropped {}.", dropped_item.item_name));
            child.push_player_tag(player_id);
            child.push_team_tag(team_id);
            child.push_metadata_uuid("itemId", dropped_item.item_id);
            child.push_metadata_str("itemName", dropped_item.item_name);
            child.push_metadata_str_vec("mods", dropped_item.item_mods);
            child.push_metadata_f64("playerItemRatingAfter", dropped_item.player_item_rating_after);
            child.push_metadata_f64("playerItemRatingBefore", dropped_item.player_item_rating_before);
            child.push_metadata_f64("playerRating", player_rating);
            child.build(EventType::PlayerLostItem)
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
                                  if (self.event.season, self.event.day) < (15, 3) { " " } else { "" },
                                  Possessive(player_name));
        self.push_description(&description);
        // In season 17 days 7-10 inclusive, the Ambitious event type was accidentally used instead
        // of ItemBreaks
        let use_ambitious = self.event.season == 17 && self.event.day >= 7 && self.event.day <= 10;
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

    pub fn push_stopped_inhabiting(&mut self, stopped_inhabiting: Option<&StoppedInhabiting>) {
        let Some(si) = stopped_inhabiting else { return; };
        self.push_child(si.sub_event, |mut child| {
            child.push_description(format!("{} stopped Inhabiting.", si.inhabiting_player_name));
            child.push_player_tag(si.inhabiting_player_id);
            if let Some(team_id) = si.inhabiting_player_team_id {
                child.push_team_tag(team_id);
            }
            child.push_metadata_str("mod", "INHABITING");
            child.push_metadata_i64("type", ModDuration::Permanent as i64);
            child.build(EventType::RemovedMod)
        })
    }

    pub fn push_free_refills(&mut self, free_refills: &[FreeRefill]) {
        for fr in free_refills {
            let common_description = format!("{} used their Free Refill.", fr.player_name);
            self.push_description(&common_description);
            self.push_description(format!("{} Refills the In!", fr.player_name));
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

    pub fn push_balloons(&mut self, balloons: Option<&str>, runs_scored: f64) {
        if let Some(stadium_name) = balloons {
            self.push_description(format!("{stadium_name} {} {runs_scored} Balloons!", self.inflated_or_inflates()));
        }
    }

    // This function only exists to make a more sensible name for the user. Option implements
    // IntoIterator so you could just call the plural form with an option.
    pub fn push_free_refill(&mut self, free_refill: Option<FreeRefill>) {
        self.push_free_refills(free_refill.as_slice())
    }

    pub fn push_scores_without_event<LedgerT: LedgerV2>(&mut self, scores: &Scores<LedgerT>, home_team_id: Uuid, score_label: &str, is_fc: bool, hype_before_score: bool) {
        // TODO get score_label from LedgerT
        self.push_scorers(&scores.scores, home_team_id, score_label, is_fc, hype_before_score);
        self.push_free_refills(&scores.free_refills);
    }

    pub fn push_scores<T: LedgerV2>(&mut self, scores: &Scores<T>, home_team_id: Uuid, score_label: &str, is_fc: bool, hype_before_score: bool) {
        self.push_scores_without_event(scores, home_team_id, score_label, is_fc, hype_before_score);
        self.push_score_summary(scores);
    }

    pub fn push_score_summary<T: LedgerV2>(&mut self, scores: &Scores<T>) {
        self.push_opt_direct_score_summary(scores.score_summary.as_ref())
    }

    pub fn push_opt_direct_score_summary<T: LedgerV2>(&mut self, score_summary: Option<&ScoreSummary<T>>) {
        if let Some(ss) = score_summary {
            self.push_direct_score_summary(ss)
        }
    }

    pub fn push_direct_score_summary<T: LedgerV2>(&mut self, score: &ScoreSummary<T>) {
        let (season, day) = (self.event.season, self.event.day);
        self.push_child(score.sub_event, |mut child_eb| {
            child_eb.set_category(EventCategory::Game);
            child_eb.push_team_tag(score.team_id);
            child_eb.push_description(format!("The {} scored!", score.team_nickname));
            child_eb.push_metadata_str("awayEmoji", &score.away_emoji);
            child_eb.push_metadata_i64_or_f64("awayScore", score.away_score);
            child_eb.push_metadata_str("homeEmoji", &score.home_emoji);
            child_eb.push_metadata_i64_or_f64("homeScore", score.home_score);
            child_eb.push_metadata_str("ledger", &score.ledger.to_string(season, day));
            // Apparently in season 22 they un-fixed the pluralization
            child_eb.push_metadata_str("update", if score.runs_scored == 1.0 && season < 21 {
                "1 Run scored!".to_string()
            } else if score.runs_scored.signum() < 0.0 { // TODO is it signum or just <= ?
                format!("{} Unruns scored!", -score.runs_scored)
            } else {
                format!("{} Runs scored!", score.runs_scored)
            });
            child_eb.build(EventType::RunsScored)
        });

        if let Some(stadium_name) = &score.balloons {
            self.push_description(format!("{stadium_name} {} {} Balloons!", self.inflated_or_inflates(), score.runs_scored.round()));
        }
    }

    pub fn inflated_or_inflates(&self) -> &'static str {
        if (self.event.season, self.event.day) < (19, 80) { "inflated" } else { "inflates" }
    }

    pub fn push_attraction(&mut self, attraction: &Attraction, player_name: &str, player_id: Uuid) {
        self.push_player_tag(player_id);
        self.push_description(format!("The {} Attract {player_name}!", attraction.team_nickname));
        self.push_child(attraction.sub_event, |mut child| {
            child.push_description(format!("The {} Attracted {player_name}!", attraction.team_nickname));
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
                child.push_description(format!("{player_name} entered the Shadows."));
                child.push_player_tag(player_id);
                child.push_team_tag(attraction.team_id);
                child.build_boost(boost)
            })
        }
    }

    pub fn push_hotel_motel_party(&mut self, hotel_motel_party: &HotelMotelParty, player_name: &str, player_id: Uuid) {
        self.push_player_tag(player_id);
        let description = format!("{player_name} is Partying!");
        self.push_description(&description);
        if let Some(stadium_name) = &hotel_motel_party.birds {
            self.push_description(format!("A flock of Birds are attracted to {stadium_name}!"));
        }
        self.push_child(hotel_motel_party.boost.sub_event, |mut child| {
            child.push_description(&description);
            child.push_player_tag(player_id);
            child.build_boost_with_team(&hotel_motel_party.boost)
        })
    }

    pub fn push_attraction_with_player(&mut self, attraction: Option<AttractionWithPlayer>) {
        let Some(at) = attraction else { return; };
        self.push_player_tag(at.player_id);
        self.push_description(format!("The {} Attract {}!", at.team_nickname, at.player_name));
        self.push_child(at.sub_event, |mut child| {
            child.push_description(format!("The {} Attracted {}!", at.team_nickname, at.player_name));
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

    pub fn push_scorers(&mut self, scorers: &[ScoringPlayer], home_team_id: Uuid, score_label: &str, is_fc: bool, hype_before_score: bool) {
        // Base scores
        for scorer in scorers {
            self.push_player_tag(scorer.player_id);
            if hype_before_score {
                self.push_hype_opt(scorer.hype.as_ref(), home_team_id);
            }
            // Fielders Choice has scorer damage after the score message, just for fun. Everything
            // else has it before.
            if is_fc {
                self.push_description(format!("{} {score_label}", scorer.player_name));
                self.push_opt_item_damage(scorer.item_damage.as_ref(), &scorer.player_name);
            } else {
                self.push_opt_item_damage(scorer.item_damage.as_ref(), &scorer.player_name);
                self.push_description(format!("{} {score_label}", scorer.player_name));
            }
            if !hype_before_score {
                self.push_hype_opt(scorer.hype.as_ref(), home_team_id);
            }
        }
        // Attractions happen in a block after the scores block
        for scorer in scorers {
            if let Some(attraction) = &scorer.attraction {
                self.push_attraction(attraction, &scorer.player_name, scorer.player_id);
            }
        }
        // Hotel motel parties happen in a block after the scores block (not sure of order w/r/t
        // attractions) (unless it's an FC in which case they're later! i love parsing blaseball.)
        if !is_fc {
            self.push_scorer_hotel_motel_parties(scorers);
        }
    }

    pub fn push_scorer_hotel_motel_parties(&mut self, scorers: &[ScoringPlayer]) {
        for scorer in scorers {
            if let Some(party) = &scorer.hotel_motel_party {
                self.push_hotel_motel_party(party, &scorer.player_name, scorer.player_id)
            }
        }
    }

    pub fn push_spicy(&mut self, spicy: SpicyStatus, player_name: &str, player_id: Uuid) {
        match spicy {
            SpicyStatus::None => {}
            SpicyStatus::HeatingUp => {
                self.push_description(format!("{player_name} is Heating Up!"));
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
            self.push_description(format!("{batter_name} hit a ball at {fielder_name}..."));
            let common_description = match bd.debt_type {
                DebtType::Observed => format!("{fielder_name} is now being Observed."),
                DebtType::Unstable => format!("{fielder_name} became Unstable!"),
            };
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
                    child.push_metadata_str("mod", bd.debt_type.mod_id());
                    child.push_metadata_i64("type", ModDuration::Weekly as i64);
                    child.build(EventType::AddedMod)
                })
            }
        }
    }

    pub fn push_pitch(&mut self, pitch: GamePitch) {
        if let Some(pitcher_name) = pitch.double_strike {
            self.set_category(EventCategory::Special);
            self.push_description(format!("{pitcher_name} fires a Double Strike!"));
        }
        if let Some(pitcher_name) = pitch.acidic_pitch {
            self.push_description(format!("{pitcher_name} throws an Acidic pitch!"));
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

    pub fn push_birds(&mut self, num_birds: Option<i64>) {
        if let Some(n) = num_birds {
            self.push_description(format!("A new Bird finds a Birdhouse. {n}"));
        }
    }

    pub fn push_parasite(&mut self, parasite: Option<Parasite>) {
        if let Some(parasite) = parasite {
            self.push_description(format!("{} parasitically drained some of {} {}.",
                                           parasite.pitcher_name, Possessive(&parasite.batter_name), parasite.attribute_name));
            self.push_description(format!("{} boosted their {}!",
                                           parasite.pitcher_name, parasite.attribute_name));
            self.push_child(parasite.batter_sub_event, |mut child| {
                child.push_description(format!("{} had blood drained by Parasite {}.",
                                                parasite.batter_name, parasite.pitcher_name));
                child.push_player_tag(parasite.batter_id);
                child.push_team_tag(parasite.batter_team_id);
                child.build_player_attribute_changed(parasite.batter_rating_before, parasite.batter_rating_after, parasite.attribute_id)
            });
            self.push_maintenance_mode(parasite.maintenance_mode);
            self.push_child(parasite.pitcher_sub_event, |mut child| {
                child.push_description(format!("Parasite {} drained blood from {}.",
                                                parasite.pitcher_name, parasite.batter_name));
                child.push_player_tag(parasite.pitcher_id);
                child.push_team_tag(parasite.pitcher_team_id);
                child.build_player_attribute_changed(parasite.pitcher_rating_before, parasite.pitcher_rating_after, parasite.attribute_id)
            });
        }
    }

    pub fn push_magmatic(&mut self, magmatic: Option<ModChangeSubEvent>, batter_name: &str, batter_id: Uuid) {
        if let Some(mod_change) = magmatic {
            self.push_description(format!("{batter_name} is Magmatic!"));
            self.push_child(mod_change.sub_event, |mut child| {
                child.push_description(format!("{batter_name} hit a Magmatic home run!"));
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
            self.push_hotel_motel_party(&party.party, &party.player_name, party.player_id);
        }
    }

    pub fn push_gravity(&mut self, gravity_players: Vec<PlayerNameId>) {
        for player in gravity_players {
            self.push_description(format!("{}'s Gravity kept them in place!", player.player_name));
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
        self.push_description(format!("Hype Builds in {}!", hype.stadium_name));
        self.push_child(hype.sub_event, |mut child_eb| {
            child_eb.set_category(EventCategory::Changes);
            // Love how the descriptions are slightly different
            child_eb.push_description(format!("Hype built in {}!", hype.stadium_name));
            child_eb.push_team_tag(home_team_id);
            child_eb.push_metadata_f64("before", hype.hype_before);
            child_eb.push_metadata_f64("after", hype.hype_after);

            child_eb.build(EventType::HypeBuilds)
        });
    }

    pub fn push_sent_elsewhere(&mut self, sent_elsewhere: &PlayerSentElsewhere, outer_description: &str, inner_description: &str) {
        self.push_description(outer_description);
        self.push_child(sent_elsewhere.sub_event, |mut child_self| {
            child_self.push_description(inner_description);
            child_self.push_team_tag(sent_elsewhere.team_id);
            child_self.push_player_tag(sent_elsewhere.player_id);
            child_self.push_metadata_str("mod", "ELSEWHERE");
            child_self.push_metadata_i64("type", ModDuration::Permanent);
            child_self.build(EventType::AddedMod)
        });

        self.push_flipped_negative_opt(sent_elsewhere.flipped_negative.as_ref(), &sent_elsewhere.player_name, sent_elsewhere.player_id, sent_elsewhere.team_id);
    }

    pub fn push_flipped_negative_opt(&mut self, flip_opt: Option<&FlipNegative>, elsewhere_player_name: &str, elsewhere_player_id: Uuid, elsewhere_team_id: Uuid) {
        if let Some(flip) = flip_opt {
            // First, undertaker also goes Elsewhere
            let undertaker_description = format!("{} dove in after {}.", flip.undertaker_player_name, elsewhere_player_name);
            self.push_description(&undertaker_description);
            self.push_player_tag(flip.undertaker_player_id);
            self.push_child(flip.undertaker_elsewhere_sub_event, |mut child_self| {
                child_self.push_description(&undertaker_description);
                child_self.push_team_tag(elsewhere_team_id);
                child_self.push_player_tag(flip.undertaker_player_id);
                child_self.push_metadata_str("mod", "ELSEWHERE");
                child_self.push_metadata_i64("type", ModDuration::Permanent);
                child_self.build(EventType::AddedMod)
            });

            // Then the actual flipping
            self.push_description(format!("{} was flipped Negative!", elsewhere_player_name));
            self.push_player_tag(elsewhere_player_id);
            self.push_child(flip.flip_negative_sub_event, |mut child_self| {
                child_self.push_description(format!("{} flipped {} Negative.", flip.undertaker_player_name, elsewhere_player_name));
                child_self.push_team_tag(elsewhere_team_id);
                child_self.push_player_tag(elsewhere_player_id);
                child_self.push_metadata_str("mod", "NEGATIVE");
                child_self.push_metadata_i64("type", ModDuration::Permanent);
                child_self.build(EventType::AddedMod)
            });
        }

    }

    pub fn push_earned_win(&mut self, win: EarnedWin) {
        let day = self.event.day;
        self.push_child(win.sub_event, |mut child_eb| {
            child_eb.set_category(EventCategory::Outcomes);
            child_eb.push_description(format!("The {} collected a Win.", win.winning_team_nickname));
            child_eb.push_team_tag(win.winning_team_id);
            // There were decrees that would have increased amount but they never won a vote
            child_eb.push_metadata_i64("amount", 1);
            child_eb.push_metadata_i64("before", win.wins_after - 1);
            child_eb.push_metadata_i64("after", win.wins_after);
            // This won't be hard-coded forever, but I won't change it until I need to
            child_eb.push_metadata_str_vec("lines", if let Some(BracketType::Underbracket) = win.bracket_type {
                // Postseason underbracket. You win by losing. God knows why it's negative.
                vec![
                    "Loss: -1".to_string(),
                    "Sun(Sun): -1 ^ 2 = 1".to_string(),
                ]
            } else if let Some(BracketType::Overbracket) = win.bracket_type {
                // Postseason underbracket. You win by winning.
                vec![
                    "Non-Loss: 1".to_string(),
                    "Sun(Sun): 1 ^ 2 = 1".to_string(),
                ]
            } else {
                // Regular season. You win by winning, and there's turntables.
                vec![
                    "Non-Loss: 1".to_string(),
                    "Turntables: 1 * -1 = -1".to_string(),
                    "Sun(Sun): -1 ^ 2 = 1".to_string(),
                ]
            });
            child_eb.build(if day < 99 {
                EventType::WinCollectedRegular
            } else {
                EventType::WinCollectedPostseason
            })
        });
    }

    pub fn push_earned_win_opt(&mut self, win: Option<EarnedWin>) {
        if let Some(win) = win {
            self.push_earned_win(win);
        }
    }

    pub fn push_team_subseasonal_mod_changes(&mut self, changes: impl IntoIterator<Item=SubseasonalModChange<TeamModChangeSubject>>, season: i64, day: i64) {
        for change in changes {
            self.push_team_subseasonal_mod_change(change, season, day);
        }
    }

    pub fn push_team_subseasonal_mod_change(&mut self, change: SubseasonalModChange<TeamModChangeSubject>, season: i64, day: i64) {
        let display_team_nickname = change.subject.team_nickname.unwrap_or_else(|| "[object Object]".to_string());
        let description = if season < 15 {
            if let Some(prefix) = change.source_mod.prefix() {
                self.push_description(prefix);
            }
            if change.active {
                format!("The {} are {}!", display_team_nickname, change.source_mod.label_for_teams())
            } else {
                format!("{} wears off for the {}.", change.source_mod.label_for_teams(), display_team_nickname)
            }
        } else {
            if change.active {
                format!("The {} are {}.", display_team_nickname, change.source_mod.label_for_teams())
            } else {
                format!("{} are no longer {}.", display_team_nickname, change.source_mod.label_for_teams())
            }
        };

        self.push_description(&description);
        if let Some(sub_event) = change.sub_event {
            self.push_child(sub_event, |mut child| {
                child.push_description(&description);
                child.push_team_tag(change.subject.team_id);
                // On s19d72, EarlyToTheParty added the wrong Performing. This was fixed on day 73.
                let performing_mod_id = if change.source_mod == SubseasonalMod::EarlyToTheParty && season == 19 && day == 72 {
                    reverse_performing(change.source_mod.performing_mod_id())
                } else {
                    change.source_mod.performing_mod_id()
                };
                child.push_metadata_str("mod", performing_mod_id);
                child.push_metadata_str("source", change.source_mod.mod_id());
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(if change.active {
                    EventType::AddedModFromOtherMod
                } else {
                    EventType::RemovedModFromOtherMod
                })
            })
        }
    }

    pub fn push_player_subseasonal_mod_change(&mut self, change: SubseasonalModChange<PlayerModChangeSubject>) {
        let description = match (change.active, change.source_mod) {
            // Specific language for specific mods
            (false, SubseasonalMod::Ambitious) => format!("{} loses their Ambition.", change.subject.player_name),
            (false, SubseasonalMod::Coasting) => format!("{} stops Coasting.", change.subject.player_name),
            // General cases
            (true, m) => format!("{} is {}.", change.subject.player_name, m.label_for_players()),
            (false, m) => format!("{} is no longer {}.", change.subject.player_name, m.label_for_players()),
        };

        self.push_description(&description);
        self.push_player_tag(change.subject.player_id);
        if let Some(sub_event) = change.sub_event {
            self.push_child(sub_event, |mut child| {
                child.push_description(&description);
                child.push_team_tag(change.subject.team_id);
                child.push_player_tag(change.subject.player_id);
                child.push_metadata_str("mod", change.source_mod.performing_mod_id());
                child.push_metadata_str("source", change.source_mod.mod_id());
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(if change.active {
                    EventType::AddedModFromOtherMod
                } else {
                    EventType::RemovedModFromOtherMod
                })
            })
        }
    }

    pub fn push_scattered(&mut self, scattered: Option<Scattered>, player_id: Uuid, team_id: Uuid) {
        if let Some(Scattered { scattered_name, sub_event }) = scattered {
            self.push_child(sub_event, |mut child| {
                child.push_description(format!("{scattered_name} was Scattered..."));
                child.push_team_tag(team_id);
                child.push_player_tag(player_id);
                child.push_metadata_str("mod", "SCATTERED");
                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                child.build(EventType::AddedMod)
            });
        }
    }

    pub fn push_temp_stolen_player_returned(&mut self, ret: &PlayerMovedTeams) {
        self.push_child(ret.sub_event, |mut child_eb| {
            child_eb.push_description(format!("{} is returned to the {}.", ret.player_name, ret.new_team_nickname));
            child_eb.push_player_tag(ret.player_id);
            child_eb.push_team_tag(ret.previous_team_id);
            child_eb.push_team_tag(ret.new_team_id);

            child_eb.push_metadata_i64("location", ret.location);
            child_eb.push_metadata_uuid("playerId", ret.player_id);
            child_eb.push_metadata_str("playerName", &ret.player_name);
            child_eb.push_metadata_i64("receiveLocation", ret.location);
            child_eb.push_metadata_uuid("receiveTeamId", ret.new_team_id);
            child_eb.push_metadata_str("receiveTeamName", &ret.new_team_nickname);
            child_eb.push_metadata_uuid("sendTeamId", ret.previous_team_id);
            child_eb.push_metadata_str("sendTeamName", &ret.previous_team_nickname);

            child_eb.build(EventType::PlayerMoved)
        });
    }

    pub fn push_flood_balloon_popped(&mut self, pop: Option<BalloonsPopped>) {
        if let Some(pop) = pop {
            self.push_description(format!("One of {} Flooding Balloons was struck and popped!", Possessive(&pop.stadium_name)));
            self.push_description(format!("{} Birds were scared away!", pop.birds_scared_away));
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
        self.push_metadata_f64_opt("playerItemRatingAfter", item_repaired.player_item_rating_after);
        self.push_metadata_f64_opt("playerItemRatingBefore", item_repaired.player_item_rating_before);
        self.push_metadata_f64("playerRating", item_repaired.player_rating);
        // In season 17 days 7-10 inclusive, the Coasting event type was accidentally used instead
        // of BrokenItemRepaired
        let use_coasting = self.event.season == 17 && self.event.day >= 7 && self.event.day <= 10;
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
        self.event.r#type = event_type;
        self.event
    }
}