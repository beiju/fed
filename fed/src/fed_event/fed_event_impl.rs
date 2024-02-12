use eventually_api::{EventCategory, EventType, EventuallyEvent, Weather};
use itertools::Itertools;
use serde_json::json;
use std::fmt::Write;
use std::iter;

use crate::parse::builder::{EventBuilderChild, EventBuilderChildFull, EventBuilderCommon, EventBuilderUpdate, make_free_refill_child, possessive};
use crate::parse::event_builder_new::{EventBuilder, Possessive};
use crate::{BatterSkippedReason, CoffeeBeanMod, ConsumerAttackEffect, Echo, EchoChamberModAdded, EchoIntoStatic, FedEvent, FedEventData, FloodingSweptEffect, HitType, ModChangeSubEventWithNamedPlayer, ModDuration, PitcherNameId, PlayerNameId, PlayerReverb, PositionType, TeamNicknameOrPlayerName, ReturnFromElsewhereFlavor, ReverbType, Scattered, StatChangeCategory, SubEvent, TimeElsewhere, TogglePerforming, PlayerStatChange, ReturnFromElsewhere, ModChangeSubject, SubseasonalModChange, SubseasonalMod, PostseasonBirthBoostEventOrder, NumbersGo, RenovationVotes, HomeRunHypeSource, RoamFromLocation, GameStartAnnouncement};

#[deprecated = "This is part of the old event builder"]
fn make_switch_performing_child(toggle: &TogglePerforming, description: &str, mod_source: &str) -> EventBuilderChildFull {
    let mod_name = if toggle.is_overperforming { "OVERPERFORMING" } else { "UNDERPERFORMING" };
    let opposite_mod_name = if toggle.is_overperforming { "UNDERPERFORMING" } else { "OVERPERFORMING" };
    if toggle.is_first_proc {
        EventBuilderChild::new(&toggle.sub_event)
            .update(EventBuilderUpdate {
                category: EventCategory::Changes,
                r#type: EventType::AddedModFromOtherMod,
                description: description.to_string(),
                team_tags: vec![toggle.team_id],
                player_tags: vec![toggle.player_id],
                ..Default::default()
            })
            .metadata(json!({
                "mod": mod_name,
                "source": mod_source,
                "type": 0, // ?
            }))
    } else {
        EventBuilderChild::new(&toggle.sub_event)
            .update(EventBuilderUpdate {
                r#type: EventType::ChangedModFromOtherMod,
                category: EventCategory::Changes,
                description: description.to_string(),
                team_tags: vec![toggle.team_id],
                player_tags: vec![toggle.player_id],
                ..Default::default()
            })
            .metadata(json!({
                "from": opposite_mod_name,
                "source": mod_source,
                "to": mod_name,
                "type": 0, // ?
            }))
    }
}

fn push_subseasonal_mod_changes(eb: &mut EventBuilder, effects: Vec<SubseasonalModChange>, season: i32) {
    for effect in effects {
        match effect.subject {
            ModChangeSubject::Player { team_id, player_id, player_name } => {
                let description = match (effect.active, effect.source_mod) {
                    // Specific language for specific mods
                    (false, SubseasonalMod::Ambitious) => format!("{} loses their Ambition.", player_name),
                    (false, SubseasonalMod::Coasting) => format!("{} stops Coasting.", player_name),
                    // General cases
                    (true, m) => format!("{} is {}.", player_name, m.label_for_players()),
                    (false, m) => format!("{} is no longer {}.", player_name, m.label_for_players()),
                };

                eb.push_description(&description);
                eb.push_player_tag(player_id);
                if let Some(sub_event) = effect.sub_event {
                    eb.push_child(sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_team_tag(team_id);
                        child.push_player_tag(player_id);
                        child.push_metadata_str("mod", effect.source_mod.performing_mod_id());
                        child.push_metadata_str("source", effect.source_mod.mod_id());
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(if effect.active {
                            EventType::AddedModFromOtherMod
                        } else {
                            EventType::RemovedModFromOtherMod
                        })
                    })
                }
            }
            ModChangeSubject::Team { team_id, team_nickname } => {
                let display_team_nickname = team_nickname.unwrap_or_else(|| "[object Object]".to_string());
                let description = if season < 15 {
                    if let Some(prefix) = effect.source_mod.prefix() {
                        eb.push_description(prefix);
                    }
                    if effect.active {
                        format!("The {} are {}!", display_team_nickname, effect.source_mod.label_for_teams())
                    } else {
                        format!("{} wears off for the {}.", effect.source_mod.label_for_teams(), display_team_nickname)
                    }
                } else {
                    if effect.active {
                        format!("The {} are {}.", display_team_nickname, effect.source_mod.label_for_teams())
                    } else {
                        format!("{} are no longer {}.", display_team_nickname, effect.source_mod.label_for_teams())
                    }
                };

                eb.push_description(&description);
                if let Some(sub_event) = effect.sub_event {
                    eb.push_child(sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_team_tag(team_id);
                        child.push_metadata_str("mod", effect.source_mod.performing_mod_id());
                        child.push_metadata_str("source", effect.source_mod.mod_id());
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(if effect.active {
                            EventType::AddedModFromOtherMod
                        } else {
                            EventType::RemovedModFromOtherMod
                        })
                    })
                }
            }
        }
    }
}

impl FedEvent {
    // I would like this to take by reference but it currently needs to call into into_feed_event,
    // which consumes its input.
    /// Returns the string that appears in the lastUpdate field in the corresponding game update for
    /// this game event. Defaults to the parent event text (in Beta) / concatenation of sibling
    /// event texts (in Gamma, not yet implemented) when the event has no corresponding game update.
    pub fn last_update(self) -> String {
        match self.data {
            // I know it makes no sense to have a match statement with only a wildcard match but
            // trust me, there will be special cases in the future.
            _ => {
                self.into_feed_events().into_iter()
                    .map(|event| event.description)
                    .join("\n")
            }
        }
    }

    pub fn into_feed_events(self) -> Vec<EventuallyEvent> {
        let event_builder = EventBuilderCommon {
            id: self.id,
            created: self.created,
            sim: self.sim.clone(),
            day: self.day,
            phase: self.phase.into(),
            season: self.season,
            tournament: self.tournament,
            nuts: self.nuts,
        };

        let mut eb = EventBuilder::new(
            self.id,
            self.created,
            self.sim.clone(),
            self.day,
            self.season,
            self.tournament,
            self.phase.into(),
            self.nuts,
        );

        let item = match self.data {
            FedEventData::BeingSpeech { being, message } => {
                let being_id: i32 = being.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BigDeal,
                        category: EventCategory::Narrative,
                        description: message,
                        ..Default::default()
                    })
                    .metadata(json!({ "being": being_id }))
                    .build()
            }
            FedEventData::GameStart { game, weather, stadium_id, announcement } => {
                match announcement {
                    GameStartAnnouncement::LetsGo => { eb.push_description("Let's Go!"); }
                    GameStartAnnouncement::TeamNames { away, home } => {
                        eb.push_description(&format!("{away} vs. {home}"));
                    }
                }
                eb.push_metadata_uuid("home", game.home_team);
                eb.push_metadata_uuid("away", game.away_team);
                eb.set_game(game);
                eb.push_metadata_i32("weather", weather);
                if let Some(id) = stadium_id {
                    eb.push_metadata_uuid("stadium", id);
                }

                eb.build(EventType::GameStart)
            }
            FedEventData::PlayBall { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayBall,
                        description: "Play ball!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name, subseasonal_mod_effects } => {
                eb.set_game(game);
                push_subseasonal_mod_changes(&mut eb, subseasonal_mod_effects, self.season);
                eb.push_description(&format!("{} of {inning}, {batting_team_name} batting.",
                                             if top_of_inning { "Top" } else { "Bottom" }));
                eb.build(EventType::HalfInning)
            }
            FedEventData::BatterUp { ref game, ref batter_name, team_nickname: ref team_name, ref wielding_item, ref inhabiting, is_repeating } => {
                let item_suffix = if let Some(item_name) = wielding_item {
                    format!(", wielding {}", item_name)
                } else {
                    String::default()
                };

                let prefix = if is_repeating {
                    format!("{batter_name} is Repeating!\n")
                } else {
                    String::default()
                };

                let inhabiting_child = inhabiting.as_ref()
                    .and_then(|inhabiting| {
                        inhabiting.sub_event.as_ref().map(|sub_event|
                            EventBuilderChild::new(sub_event)
                                .update(EventBuilderUpdate {
                                    r#type: EventType::AddedMod,
                                    category: EventCategory::Changes,
                                    description: format!("{} is Inhabiting {}!",
                                                         batter_name, inhabiting.inhabited_player_name),
                                    player_tags: vec![inhabiting.inhabiting_player_id],
                                    team_tags: inhabiting.inhabiting_player_team_id.iter().cloned().collect(),
                                    ..Default::default()
                                })
                                .metadata(json!({
                                    "mod": "INHABITING",
                                    "type": 0, // ?
                                }))
                        )
                    });

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BatterUp,
                        category: EventCategory::special_if(inhabiting.is_some() || is_repeating),
                        description: if let Some(inhabiting) = &inhabiting {
                            format!("{prefix}{batter_name} is Inhabiting {}!\n{batter_name} batting for the {team_name}{item_suffix}.",
                                    inhabiting.inhabited_player_name)
                        } else {
                            format!("{prefix}{batter_name} batting for the {team_name}{item_suffix}.")
                        },
                        player_tags: if let Some(inhabiting) = inhabiting {
                            vec![inhabiting.inhabiting_player_id, inhabiting.inhabited_player_id]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    })
                    .children(inhabiting_child)
                    .build()
            }
            FedEventData::SuperyummyGameStart { ref game, ref toggle } => {
                let description = format!("{} {} Peanuts.", toggle.player_name,
                                          if toggle.is_overperforming { "loves" } else { "misses" });
                let change_event = make_switch_performing_child(toggle, &description, "SUPERYUMMY");
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Superyummy,
                        description,
                        ..Default::default()
                    })
                    .child(change_event)
                    .build()
            }
            FedEventData::EchoedSuperyummyGameStart { ref game, ref player_name, peanuts_present: peanuts } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Superyummy,
                        description: format!("{} {} Peanuts.", player_name,
                                             if peanuts { "loves" } else { "misses" }),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Ball { game, pitch, balls, strikes, batter_item_damage } => {
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.push_description(&format!("Ball. {}-{}", balls, strikes));
                // I think this conversion should be implicit but the compiler disagrees
                eb.push_named_item_damage(batter_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Ball)
            }
            FedEventData::StrikeSwinging { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                let is_double_strike = pitch.double_strike.is_some();
                if is_double_strike { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike{}, swinging. {balls}-{strikes}",
                                             if is_double_strike { "s" } else { "" }));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeLooking { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike, looking. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeFlinching { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("Strike, flinching. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::FoulBall { game, pitch, balls, strikes, batter_item_damage, birds } => {
                eb.set_game(game);
                let foul_ball_text = if pitch.double_strike.is_some() {
                    eb.set_category(EventCategory::Special);
                    "Foul Balls"
                } else if self.season < 19 {
                    "Foul Ball"
                } else {
                    " Foul Ball" // Annoying extra space
                };
                eb.push_pitch(pitch);
                eb.push_description(&format!("{foul_ball_text}. {balls}-{strikes}"));
                eb.push_named_item_damage(batter_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_birds(birds);
                eb.build(EventType::FoulBall)
            }
            FedEventData::Flyout { game, pitch, batter_name, fielder_name, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, fielder_item_damage, other_player_item_damage, parasite } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} hit a flyout to {fielder_name}."));
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_opt_item_damage(fielder_item_damage.as_ref(), &fielder_name);
                eb.push_named_item_damage(other_player_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                eb.push_scores(scores, home_team_id, "tags up and scores!");
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_parasite(parasite);
                eb.build(EventType::FlyOut)
                // let (suffix, observed_child, player_tags) = apply_batter_debt(&batter_debt, &batter_name, &fielder_name);
                //
                // event_builder.for_game(&game)
                //     .fill(EventBuilderUpdate {
                //         r#type: ,
                //         category: EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special),
                //         description: format!("{batter_name} hit a flyout to {fielder_name}.{suffix}"),
                //         player_tags,
                //         ..Default::default()
                //     })
                //     .scores(&scores, " tags up and scores!")
                //     .stopped_inhabiting(&stopped_inhabiting)
                //     .cooled_off(&cooled_off, &batter_name)
                //     .children(observed_child) // slight abuse of IntoIter
                //     .item_damage_before_score(&batter_item_damage, &batter_name)
                //     .item_damage_before_score(&fielder_item_damage, &fielder_name)
                //     .named_item_damage_before_score(&other_player_item_damage)
                //     .build()
            }
            FedEventData::Hit { game, pitch, batter_name, batter_id, hit_type, scores, spicy_status, stopped_inhabiting, is_special, pitcher_item_damage, batter_item_damage, other_player_item_damage } => {
                let home_team_id = game.home_team; // Need this later
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_description(&format!("{batter_name} hits a {hit_type}!"));
                eb.push_player_tag(batter_id);
                match hit_type {
                    HitType::Triple(power_charge) => {
                        eb.push_charge_blood(power_charge, &batter_name, batter_id, "aaa");
                    }
                    HitType::Double(power_charge) => {
                        eb.push_charge_blood(power_charge, &batter_name, batter_id, "aa");
                    }
                    _ => {}
                }
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_scores(scores, home_team_id, "scores!");
                eb.push_spicy(spicy_status, &batter_name, batter_id);
                eb.push_named_item_damage(other_player_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));

                eb.build(EventType::Hit)
            }
            FedEventData::HomeRun { game, pitch, magmatic, batter_name, batter_id, home_run_type, free_refills, spicy_status, stopped_inhabiting, is_special, big_bucket, attraction, damaged_items, hotel_motel_parties, hype, alley_oop, score_event } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                if is_special { eb.set_category(EventCategory::Special) }
                eb.push_pitch(pitch);
                eb.push_named_item_damages(damaged_items.iter().map(|(x, y)| (x.as_str(), y)));
                eb.push_magmatic(magmatic, &batter_name, batter_id);
                if let Some(h) = &hype && h.source == HomeRunHypeSource::HomeRun {
                    eb.push_hype(&h.hype, home_team_id);
                }

                // HR itself
                eb.push_description(&format!("{batter_name} hits a {home_run_type}!"));
                eb.push_player_tag(batter_id);

                eb.push_hotel_motel(&hotel_motel_parties);

                if big_bucket {
                    eb.push_description("The ball lands in a Big Bucket. An extra Run scores!");
                    if let Some(h) = &hype && h.source == HomeRunHypeSource::Buckets {
                        eb.push_hype(&h.hype, home_team_id);
                    }
                }

                if let Some((ooper, success)) = alley_oop {
                    eb.push_description(&format!("{ooper} went up for the alley oop..."));
                    eb.push_description(if success {
                        "...they slammed it down for an extra Run!"
                    } else {
                        "...but they can't connect."
                    });
                    if let Some(h) = &hype && h.source == HomeRunHypeSource::Hoops {
                        eb.push_hype(&h.hype, home_team_id);
                    }
                }

                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_free_refills(free_refills);
                eb.push_spicy(spicy_status, &batter_name, batter_id);
                eb.push_attraction_with_player(attraction);
                if let Some(se) = &score_event {
                    eb.push_score_event(se);
                }

                eb.build(EventType::HomeRun)
            }
            FedEventData::GroundOut { game, pitch, batter_name, fielder_name, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, pitcher_item_damage, fielder_item_damage } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} hit a ground out to {fielder_name}."));
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                if self.season < 18 {
                    eb.push_scores(scores, home_team_id, "advances on the sacrifice.");
                    // Per resim, it's definitely pitcher-batter-fielder in that order. It's also
                    // definitely somewhere after scores. Rest of the order is not yet known
                    eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                    eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                    eb.push_opt_item_damage(fielder_item_damage.as_ref(), &fielder_name);
                } else {
                    // Seems like order was changed in s19
                    eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                    eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                    eb.push_opt_item_damage(fielder_item_damage.as_ref(), &fielder_name);
                    eb.push_scores(scores, home_team_id, "advances on the sacrifice.");
                }
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.build(EventType::GroundOut)
            }
            FedEventData::StolenBase { game, runner_name, runner_id, base_stolen, blaserunning, free_refill, runner_item_damage, is_special, hype, score_event } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.push_player_tag(runner_id);
                eb.set_category(EventCategory::special_if(blaserunning || free_refill.is_some() || is_special));
                eb.push_description(&format!("{runner_name} steals {base_stolen} base!"));

                if blaserunning {
                    eb.push_description(&format!("{} scores with Blaserunning!", runner_name));
                    // The player tag appears a second time when there's blaserunning
                    eb.push_player_tag(runner_id);
                }

                eb.push_free_refill(free_refill);
                eb.push_opt_item_damage(runner_item_damage.as_ref(), &runner_name);
                eb.push_hype_opt(hype.as_ref(), home_team_id);
                if let Some(se) = &score_event {
                    eb.push_score_event(se);
                }

                eb.build(EventType::StolenBase)
            }
            FedEventData::StrikeoutSwinging { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{} strikes out swinging.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.build(EventType::Strikeout)
            }
            FedEventData::StrikeoutLooking { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{} strikes out looking.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.build(EventType::Strikeout)
            }
            FedEventData::Walk { game, pitch, batter_name, batter_id, scores, base_instincts, batter_item_damage, stopped_inhabiting, is_special } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || base_instincts.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} draws a walk."));
                if let Some(base) = base_instincts {
                    eb.push_description(&format!("Base Instincts take them directly to {base} base!"));
                }
                eb.push_player_tag(batter_id);
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_scores(scores, home_team_id, "scores!");
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.build(EventType::Walk)
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen, fielder_item_damage } => {
                eb.set_game(game);
                eb.push_description(&format!("{runner_name} gets caught stealing {base_stolen} base."));
                eb.push_named_item_damage(fielder_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::StolenBase)
            }
            FedEventData::InningEnd { ref game, inning_num, ref lost_triple_threat } => {
                let (children, suffix) = self.make_mod_change_sub_events(lost_triple_threat, EventType::RemovedMod, "is no longer a Triple Threat.", "TRIPLE_THREAT");

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::InningEnd,
                        description: format!("Inning {inning_num} is now an Outing.{suffix}"),
                        player_tags: lost_triple_threat.iter().map(|e| e.player_id).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::CharmStrikeout { game, charmer_id, charmer_name, charmed_id, charmed_name, stopped_inhabiting, num_swings } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("{charmer_name} charmed {charmed_name}!"));
                eb.push_description(&format!("{charmed_name} swings {num_swings} times to strike out willingly!"));
                // I do not know why the charmer appears twice, but that seems to be accurate
                eb.push_player_tag(charmer_id);
                eb.push_player_tag(charmer_id);
                eb.push_player_tag(charmed_id);
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.build(EventType::Strikeout)
            }
            FedEventData::FieldersChoice { game, pitch, batter_name, runner_out_name, out_at_base, scores, stopped_inhabiting, cooled_off, is_special, damaged_items } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                if is_special { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(&format!("{runner_out_name} out at {out_at_base} base."));
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_scorers(scores.scores, home_team_id, "scores!");
                eb.push_named_item_damages(damaged_items.iter().map(|(x, y)| (x.as_str(), y)));
                eb.push_description(&format!("{batter_name} reaches on fielder's choice."));
                eb.push_free_refills(scores.free_refills);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.build(EventType::GroundOut)
            }
            FedEventData::StrikeZapped { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::StrikeZapped,
                        category: EventCategory::Special,
                        description: "The Electricity zaps a strike away!".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PeanutFlavorText { game, message } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PeanutFlavorText,
                        category: EventCategory::Special,
                        description: message,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::DoublePlay { game, pitch, batter_name, scores, stopped_inhabiting, cooled_off } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} hit into a double play!"));
                eb.push_scores(scores, home_team_id, "scores!");
                eb.push_stopped_inhabiting(stopped_inhabiting);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.build(EventType::GroundOut)
            }
            FedEventData::GameEnd { game, winner_id, winning_team_name, winning_team_score, losing_team_name, losing_team_score, temp_stolen_player_returned } => {
                let child = temp_stolen_player_returned
                    .map(|ret| {
                        EventBuilderChild::new(&ret.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::PlayerMoved,
                                category: EventCategory::Changes,
                                description: format!("{} is returned to the {}.",
                                                     ret.player_name, ret.new_team_nickname),
                                player_tags: vec![ret.player_id],
                                team_tags: vec![ret.previous_team_id, ret.new_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "location": ret.location as i64,
                                "playerId": ret.player_id,
                                "playerName": ret.player_name,
                                "receiveLocation": ret.location as i64,
                                "receiveTeamId": ret.new_team_id,
                                "receiveTeamName": ret.new_team_nickname,
                                "sendTeamId": ret.previous_team_id,
                                "sendTeamName": ret.previous_team_nickname,
                            }))
                    });
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GameEnd,
                        category: EventCategory::Outcomes,
                        description: format!("{winning_team_name} {winning_team_score}, {losing_team_name} {losing_team_score}"),
                        team_tags: vec![game.home_team, game.away_team],
                        // This is the default value but I'm stating it just to convey that yes, I
                        // do mean to repeat the team tags. That's how the events are.
                        override_team_tags: false,
                        ..Default::default()
                    })
                    .metadata(json!({ "winner": winner_id }))
                    .children(child)
                    .build()
            }
            FedEventData::MildPitch { ref game, pitcher_id, ref pitcher_name, balls, strikes, runners_advance, ref scores } => {
                let runners_advance_str = if runners_advance {
                    "\nRunners advance on the pathetic play!"
                } else {
                    ""
                };

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\nBall, {balls}-{strikes}.{runners_advance_str}"),
                        player_tags: vec![pitcher_id],
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .build()
            }
            FedEventData::CoffeeBean { ref game, player_id, ref player_name, ref roast, ref notes, ref which_mod, gained_mod, ref sub_event, team_id, ref previous } => {
                let change_str = match (gained_mod, which_mod) {
                    (true, CoffeeBeanMod::Wired) => { "is Wired!" }
                    (true, CoffeeBeanMod::Tired) => { "is Tired." }
                    (false, CoffeeBeanMod::Wired) => { "is no longer Wired." }
                    (false, CoffeeBeanMod::Tired) => { "is no longer Tired!" }
                };
                let mod_id = which_mod.to_str();
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: if previous.is_some() {
                            EventType::ModChange
                        } else if gained_mod {
                            EventType::AddedMod
                        } else {
                            EventType::RemovedMod
                        },
                        category: EventCategory::Changes,
                        description: format!("{player_name} {change_str}"),
                        team_tags: team_id.into_iter().collect(),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(
                        if let Some(prev_mod) = previous {
                            let prev_mod_id = prev_mod.to_str();
                            json!({
                                "from": prev_mod_id,
                                "to": mod_id,
                                "type": 3, // ?
                            })
                        } else {
                            json!({
                                "mod": mod_id,
                                "type": 3, // ?
                            })
                        }
                    );

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::CoffeeBean,
                        category: EventCategory::Special,
                        description: format!("{player_name} is Beaned by a {roast} roast with {notes}.\n{player_name} {change_str}"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BecameMagmatic { game, player_id, player_name, is_unstable, magmatic_mod_added } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if is_unstable {
                    eb.push_description(&format!("{player_name} is Unstable!"));
                }
                eb.push_description(&format!("Rogue Umpire tried to incinerate {player_name}, but {player_name} ate the flame! They became Magmatic!"));
                eb.push_player_tag(player_id);
                if let Some(mod_added) = magmatic_mod_added {
                    eb.push_child(mod_added.sub_event, |mut child| {
                        child.set_description(format!("{player_name} ate some flame."));
                        child.push_player_tag(player_id);
                        child.push_team_tag(mod_added.team_id);
                        child.push_metadata_str("mod", "MAGMATIC");
                        child.push_metadata_i64("type", ModDuration::Permanent as i64);
                        child.build(EventType::AddedMod)
                    })
                }
                eb.build(EventType::IncinerationBlocked)
                // let child = EventBuilderChild::new(mod_add_event)
                //     .update(EventBuilderUpdate {
                //         r#type: EventType::AddedMod,
                //         category: EventCategory::Changes,
                //         description: format!("{player_name} ate some flame.", ),
                //         team_tags: vec![team_id],
                //         player_tags: vec![player_id],
                //         ..Default::default()
                //     })
                //     .metadata(json!({
                //         "mod": "MAGMATIC",
                //         "type": 0, // ?
                //     }));
                // event_builder.for_game(game)
                //     .fill(EventBuilderUpdate {
                //         r#type: EventType::IncinerationBlocked,
                //         category: EventCategory::Special,
                //         description: format!("Rogue Umpire tried to incinerate {player_name}, but {player_name} ate the flame! They became Magmatic!"),
                //         player_tags: vec![player_id],
                //         ..Default::default()
                //     })
                //     .child(child)
                //     .build()
            }
            FedEventData::SpecialBlooddrain { game, sipper_id, sipper_name, sipped_id, sipped_team_id, sipped_name, sipped_category, action, sipped_event, rating_before, rating_after, maintenance_mode } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                eb.push_description(&format!("{sipper_name}'s Siphon activates!"));
                eb.push_description(&format!("{sipper_name} siphoned some of {sipped_name}'s {sipped_category} ability!"));
                eb.push_description(&format!("{sipper_name} {action}"));
                eb.push_player_tag(sipper_id);
                eb.push_player_tag(sipped_id);


                eb.push_child(sipped_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&format!("{sipped_name} had blood drained by {sipper_name}."));
                    child_eb.push_team_tag(sipped_team_id);
                    child_eb.push_player_tag(sipped_id);
                    child_eb.push_metadata_i64("type", sipped_category);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.build(EventType::PlayerStatDecrease)
                });

                if let Some(mm) = maintenance_mode {
                    eb.push_child(mm, |mut child_eb| {
                        child_eb.set_category(EventCategory::Changes);
                        child_eb.push_description("Impairment Detected. Entering Maintenance Mode.");
                        child_eb.push_team_tag(sipped_team_id);
                        child_eb.push_metadata_i64("type", ModDuration::Game);
                        child_eb.push_metadata_str("mod", "EXTRA_OUT");
                        child_eb.build(EventType::AddedMod)
                    });
                }

                eb.build(EventType::BlooddrainSiphon)
            }
            FedEventData::PlayerModExpires { team_id, player_id, player_name, mods, mod_duration } => {
                let mod_ids: Vec<_> = mods.iter()
                    .map(|removal| removal.mod_id.clone())
                    .collect();

                let mut events: Vec<_> = mods.into_iter()
                    .filter_map(|r| r.dependent_mod_removal
                        .map(|mod_removal| {
                            let mut child_eb = eb.connected_event(mod_removal.event);
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(&mod_removal.format_description_player(&player_name));
                            child_eb.push_player_tag(player_id);
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_json("removes", mod_removal.mods_removed.into());
                            child_eb.push_metadata_str("source", r.mod_id);
                            child_eb.build(EventType::RemovedModsFromAnotherMod)
                        })
                    )
                    .collect();
                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{} {mod_duration} mods wore off.", possessive(player_name)));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_str_vec("mods", mod_ids);
                eb.push_metadata_i64("type", mod_duration);
                events.push(eb.build(EventType::ModExpires));

                return events;
            }
            FedEventData::TeamModExpires { team_id, team_nickname, mods, mod_duration } => {
                let mod_ids: Vec<_> = mods.iter()
                    .map(|removal| removal.mod_id.clone())
                    .collect();

                let mut events: Vec<_> = mods.into_iter()
                    .filter_map(|r| r.dependent_mod_removal
                        .map(|mod_removal| {
                            let mut child_eb = eb.connected_event(mod_removal.event);
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(&mod_removal.format_description_team(&team_nickname));
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_json("removes", mod_removal.mods_removed.into());
                            child_eb.push_metadata_str("source", r.mod_id);
                            child_eb.build(EventType::RemovedModsFromAnotherMod)
                        })
                    )
                    .collect();
                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("The {} {mod_duration} mods wore off.", possessive(team_nickname)));
                eb.push_team_tag(team_id);
                eb.push_metadata_str_vec("mods", mod_ids);
                eb.push_metadata_i64("type", mod_duration);
                events.push(eb.build(EventType::ModExpires));

                return events;
            }
            FedEventData::BirdsCircle { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BirdsCircle,
                        category: EventCategory::Special,
                        description: "The Birds circle ... but they don't find what they're looking for.".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::AmbushedByCrows { ref game, batter_id, ref batter_name, friend_of_crows: ref pitcher } => {
                let prefix = if let Some(PitcherNameId { pitcher_name, .. }) = pitcher {
                    format!("{pitcher_name} calls upon their Friends!\n")
                } else {
                    String::new()
                };
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AmbushedByCrows,
                        category: EventCategory::Special,
                        description: format!("{prefix}A murder of Crows ambush {batter_name}!\nThey run to safety, resulting in an out."),
                        player_tags: if let Some(PitcherNameId { pitcher_id, .. }) = pitcher { vec![*pitcher_id, batter_id] } else { vec![batter_id] },
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2SetWin { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Sun2SetWin,
                        category: EventCategory::Outcomes,
                        description: format!("Sun 2 set a Win upon the {team_nickname}."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::BlackHoleSwallowedWin { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BlackHoleSwallowedWin,
                        category: EventCategory::Outcomes,
                        description: format!("The Black Hole swallowed a Win from the {team_nickname}!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Sun2 { game, team_nickname, caught_some_rays } => {
                let suffix = if let Some(rays) = &caught_some_rays {
                    format!("\n{} catches some rays.", rays.player_name)
                } else {
                    String::new()
                };

                let child = if let Some(rays) = &caught_some_rays {
                    Some(EventBuilderChild::new(&rays.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::PlayerStatIncrease,
                            category: EventCategory::Changes,
                            description: format!("{} caught some rays.", rays.player_name),
                            team_tags: vec![rays.team_id],
                            player_tags: vec![rays.player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                        "type": 4, // ?
                        "before": rays.rating_before,
                        "after": rays.rating_after,
                    })))
                } else {
                    None
                };

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Sun2,
                        category: EventCategory::Special,
                        description: format!("The {team_nickname} collect 10! Sun 2 smiles.\nSun 2 set a Win upon the {team_nickname}.{suffix}"),
                        player_tags: if let Some(rays) = &caught_some_rays {
                            vec![rays.player_id]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    })
                    .children(child)
                    .build()
            }
            FedEventData::BlackHole { game, scoring_team_nickname, victim_team_nickname, carcinization, compressed_by_gamma } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("The {scoring_team_nickname} collect 10!"));
                eb.push_description(&format!("The Black Hole swallows the Runs and a {victim_team_nickname} Win."));

                if let Some(carc_full) = carcinization {
                    let carc = carc_full.mv; // convenience
                    let carc_description = format!("The {} steal {} for the remainder of the game.",
                                                   carc_full.new_team_name, carc.player_name);
                    let mod_add_description = format!("{} was temporarily stolen.", carc.player_name);
                    eb.push_description(&carc_description);
                    eb.push_child(carc.sub_event, |mut child| {
                        child.push_description(&carc_description);
                        child.push_player_tag(carc.player_id);
                        child.push_team_tag(carc.previous_team_id);
                        child.push_team_tag(carc.new_team_id);
                        child.push_metadata_i64("location", carc.location);
                        child.push_metadata_uuid("playerId", carc.player_id);
                        child.push_metadata_str("playerName", carc.player_name);
                        child.push_metadata_i64("receiveLocation", carc.location);
                        child.push_metadata_uuid("receiveTeamId", carc.new_team_id);
                        child.push_metadata_str("receiveTeamName", carc.new_team_nickname);
                        child.push_metadata_uuid("sendTeamId", carc.previous_team_id);
                        child.push_metadata_str("sendTeamName", carc.previous_team_nickname);
                        child.build(EventType::PlayerMoved)
                    });
                    eb.push_child(carc_full.mod_added_sub_event, |mut child| {
                        child.push_description(&mod_add_description);
                        child.push_player_tag(carc.player_id);
                        child.push_team_tag(carc.new_team_id);
                        child.push_metadata_str("mod", "TEMP_STOLEN");
                        child.push_metadata_i64("type", ModDuration::Game as i64);
                        child.build(EventType::AddedMod)
                    });
                }

                if let Some(gamma) = compressed_by_gamma {
                    eb.push_description("The Black Hole burps!");
                    eb.push_description(&format!("{} is compressed by gamma!", gamma.player_name));
                    eb.push_player_tag(gamma.player_id);
                    eb.push_child(gamma.sub_event, |mut child| {
                        child.push_description(&format!("{} was compressed by gamma!", gamma.player_name));
                        child.push_player_tag(gamma.player_id);
                        child.push_team_tag(gamma.team_id);
                        child.push_metadata_f64("before", gamma.rating_before);
                        child.push_metadata_f64("after", gamma.rating_after);
                        child.push_metadata_i64("type", 4);
                        child.build(EventType::PlayerStatDecrease)
                    })
                }

                eb.build(EventType::BlackHole)
            }
            FedEventData::TeamDidShame { shaming_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamDidShame,
                        category: EventCategory::Outcomes,
                        description: format!("The {shaming_team_nickname} shamed the {shamed_team_nickname}."),
                        team_tags: vec![shaming_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "totalShames": total_shames,
                        "totalShamings": total_shamings,
                    }))
                    .build()
            }
            FedEventData::TeamWasShamed { shamed_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamWasShamed,
                        category: EventCategory::Outcomes,
                        description: format!("The {shamed_team_nickname} were shamed by the {shaming_team_nickname}."),
                        team_tags: vec![shamed_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "totalShames": total_shames,
                        "totalShamings": total_shamings,
                    }))
                    .build()
            }
            FedEventData::CharmWalk { game, pitch, batter_name, batter_id, pitcher_name, batter_item_damage, pitcher_item_damage, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_opt_item_damage(pitcher_item_damage.as_ref(), &pitcher_name);
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_description(&format!("{batter_name} charms {pitcher_name}!"));
                eb.push_description(&format!("{batter_name} walks to first base."));
                eb.push_player_tag(batter_id);
                eb.push_player_tag(batter_id); // two of them
                eb.push_scores(scores, home_team_id, "scores!");
                eb.build(EventType::Walk)
            }
            FedEventData::GainFreeRefill { ref game, player_id, ref player_name, ref roast, ref ingredient1, ref ingredient2, ref sub_event, team_id } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} got a Free Refill."),
                        team_tags: team_id.into_iter().collect(),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "COFFEE_RALLY",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GainFreeRefill,
                        category: EventCategory::Special,
                        description: format!("{player_name} is Poured Over with a {roast} roast blending {ingredient1} and {ingredient2}!\n{player_name} got a Free Refill."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .children(vec![child])
                    .build()
            }
            FedEventData::AllergicReaction { game, team_id, player_id, player_name, sub_event, rating_before, rating_after, weather_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("{player_name} swallowed a stray peanut and had an allergic reaction!"));
                eb.push_player_tag(player_id);

                if let Some(weather) = weather_event {
                    eb.push_child(weather.sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description(&format!("{player_name} swallowed a stray peanut."));
                        child_eb.push_team_tag(team_id);
                        child_eb.push_player_tag(player_id);
                        child_eb.push_metadata_str("effect", "Allergic Reaction");
                        child_eb.push_metadata_i32("weather", Weather::Peanuts);
                        child_eb.build(EventType::WeatherEvent)
                    });
                }

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&format!("{player_name} had an allergic reaction."));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_i64("type", 4);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.build(EventType::PlayerStatDecrease)
                });

                eb.build(EventType::AllergicReaction)
            }
            FedEventData::SuperallergicReaction { game, team_id, player_id, player_name, sub_event, rating_before, rating_after } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("{player_name} swallowed a stray peanut and had a Superallergic reaction!"));
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&format!("{player_name} had a Superallergic reaction."));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_i64("type", 4);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.build(EventType::PlayerStatDecreaseFromSuperallergic)
                });

                eb.build(EventType::SuperallergicReaction)
            }
            FedEventData::MildPitchWalk { ref game, pitcher_id, ref pitcher_name, batter_id, ref batter_name, ref scores } => {
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::MildPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} throws a Mild pitch!\n{batter_name} draws a walk."),
                        player_tags: vec![pitcher_id, batter_id],
                        ..Default::default()
                    })
                    .scores(scores, " scores!")
                    .build()
            }
            FedEventData::PerkUp { ref game, ref players } => {
                let children = players.iter()
                    .map(|player| {
                        EventBuilderChild::new(&player.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedModFromOtherMod,
                                category: EventCategory::Changes,
                                description: format!("{} Perks up.", player.player_name),
                                team_tags: vec![player.team_id],
                                player_tags: vec![player.player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "OVERPERFORMING",
                                "source": "PERK",
                                "type": 3, // ?
                            }))
                    });

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Perk,
                        category: EventCategory::Special,
                        description: players.iter()
                            .map(|player| format!("{} Perks up.", player.player_name))
                            .join("\n"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::Blooddrain { game, is_siphon, sipper, maintenance_mode, sipped, sipped_category } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                if is_siphon { eb.push_description(&format!("{}'s Siphon activates!", sipper.player_name)); }
                eb.push_description(&format!("{} siphoned some of {}'s {sipped_category} ability!", sipper.player_name, sipped.player_name));
                eb.push_description(&format!("{} increased their {sipped_category} ability!", sipper.player_name));

                // Can't put this in build_child because the player tags are in the opposite order
                // from the child events, for some reason
                eb.push_player_tag(sipper.player_id);
                eb.push_player_tag(sipped.player_id);

                let build_child = |eb: &mut EventBuilder, change: &PlayerStatChange, description: String| {
                    eb.push_child(change.sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_player_tag(change.player_id);
                        child.push_team_tag(change.team_id);
                        child.build_player_stat_changed(change.rating_before, change.rating_after, sipped_category as i64)
                    });
                };

                build_child(&mut eb, &sipped,
                            format!("{} had blood drained by {}.", sipped.player_name, sipper.player_name));
                eb.push_maintenance_mode(maintenance_mode);
                build_child(&mut eb, &sipper,
                            format!("{} drained blood from {}.", sipper.player_name, sipped.player_name));

                eb.build(if is_siphon { EventType::BlooddrainSiphon } else { EventType::Blooddrain })
            }
            FedEventData::Feedback { game, players: (player_a, player_b), lcd_soundsystem, position_type, sub_event } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Reality flickers. Things look different ...");
                eb.push_description(&format!("{} and {} switch teams in the feedback!", player_a.player_name, player_b.player_name));
                eb.push_player_tag(player_a.player_id);
                eb.push_player_tag(player_b.player_id);

                if let Some((lcd_a, lcd_b)) = lcd_soundsystem {
                    let team_nickname = if player_a.team_id == home_team_id {
                        &player_a.team_nickname
                    } else {
                        &player_b.team_nickname
                    };

                    eb.push_description(&format!("The LCD Soundsystem is playing at the {team_nickname}' house!"));
                    for (lcd, player) in [(lcd_a, &player_a), (lcd_b, &player_b)] {
                        eb.push_child(lcd.sub_event, |mut child| {
                            child.push_description(&format!("The LCD Soundsystem boosted {}!", player.player_name));
                            child.push_player_tag(player.player_id);
                            child.push_team_tag(player.team_id);
                            child.build_boost(&lcd)
                        });
                    }
                }

                eb.push_description(&format!("{} is now {}.", player_b.player_name, position_type.role()));
                eb.push_child(sub_event, |mut child| {
                    child.push_description("Reality flickered in the Feedback.");
                    child.push_player_tag(player_a.player_id);
                    child.push_player_tag(player_b.player_id);
                    child.push_team_tag(player_a.team_id);
                    child.push_team_tag(player_b.team_id);
                    child.push_metadata_i64("aLocation", player_a.location as i64);
                    child.push_metadata_uuid("aPlayerId", player_a.player_id);
                    child.push_metadata_str("aPlayerName", player_a.player_name);
                    child.push_metadata_uuid("aTeamId", player_a.team_id);
                    child.push_metadata_str("aTeamName", player_a.team_nickname);
                    child.push_metadata_i64("bLocation", player_b.location as i64);
                    child.push_metadata_uuid("bPlayerId", player_b.player_id);
                    child.push_metadata_str("bPlayerName", player_b.player_name);
                    child.push_metadata_uuid("bTeamId", player_b.team_id);
                    child.push_metadata_str("bTeamName", player_b.team_nickname);
                    child.build(EventType::PlayerTraded)
                });

                eb.build(EventType::FeedbackSwap)
            }
            FedEventData::BestowReverberating { ref game, team_id, player_id, ref player_name, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} is now Reverberating wildly!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "REVERBERATING",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ReverbBestowsReverberating,
                        category: EventCategory::Special,
                        description: format!("Reverberations are at dangerous levels!\n{player_name} is now Reverberating wildly!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Reverb { game, team_id, team_nickname, reverb_type, gravity_players } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                // let get_child = |sub_event, event_type, shuffle_location| {
                //     EventBuilderChild::new(sub_event)
                //         .update(EventBuilderUpdate {
                //             r#type: event_type,
                //             category: EventCategory::Changes,
                //             description: format!("The {team_nickname} {shuffle_location}"),
                //             team_tags: vec![team_id],
                //             ..Default::default()
                //         })
                //         .metadata(json!({ "parent": self.id }))
                // };
                //
                // let gravity_suffix = gravity_players.iter()
                //     .map(|player| format!("\n{}'s Gravity kept them in place!", player.player_name))
                //     .join("");
                //
                // let mut player_tags = gravity_players.iter()
                //     .map(|player| player.player_id)
                //     .collect();

                match reverb_type {
                    ReverbType::Lineup(sub_event) => {
                        eb.push_description("Reverberations are at unsafe levels!");
                        eb.push_description(&format!("The {team_nickname} had their lineup shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(&format!("The {team_nickname} had their lineup shuffled."));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbLineupShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::Rotation(sub_event) => {
                        eb.push_description("Reverberations are at unsafe levels!");
                        eb.push_description(&format!("The {team_nickname} had their rotation shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(&format!("The {team_nickname} had their rotation shuffled in the Reverb!"));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbRotationShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::Full(sub_event) => {
                        eb.push_description("Reverberations are at dangerous levels!");
                        eb.push_description(&format!("The {team_nickname} were shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(&format!("The {team_nickname} were shuffled in the Reverb!"));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbFullShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::SeveralPlayers(player_reverbs) => {
                        eb.push_description("Reverberations are at high levels!");
                        eb.push_description(&format!("The {team_nickname} had several players shuffled in the Reverb!"));
                        let common_description = format!("The {team_nickname} had several players shuffled in the Reverb!");
                        for player_reverb in player_reverbs {
                            match player_reverb {
                                PlayerReverb::RepeatId(repeated_id) => {
                                    eb.push_player_tag(repeated_id);
                                    eb.push_player_tag(repeated_id);
                                }
                                PlayerReverb::Swap { first_player_id, first_player_name, first_player_new_location, second_player_id, second_player_name, second_player_new_location, sub_event } => {
                                    eb.push_player_tag(first_player_id);
                                    eb.push_player_tag(second_player_id);
                                    eb.push_child(sub_event, |mut child| {
                                        child.push_description(&common_description);
                                        child.push_team_tag(team_id);
                                        child.push_player_tag(first_player_id);
                                        child.push_player_tag(second_player_id);
                                        child.push_metadata_i64("aLocation", first_player_new_location as i64);
                                        child.push_metadata_uuid("aPlayerId", first_player_id);
                                        child.push_metadata_str("aPlayerName", first_player_name);
                                        child.push_metadata_i64("bLocation", second_player_new_location as i64);
                                        child.push_metadata_uuid("bPlayerId", second_player_id);
                                        child.push_metadata_str("bPlayerName", second_player_name);
                                        child.push_metadata_uuid("teamId", team_id);
                                        child.push_metadata_str("teamName", &team_nickname);

                                        child.build(EventType::PlayerSwap)
                                    });
                                }
                            }
                        }
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                }
            }
            FedEventData::TarotReading { description, metadata, player_tags, team_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TarotReading,
                        category: EventCategory::Changes,
                        description,
                        player_tags,
                        team_tags,
                        ..Default::default()
                    })
                    .metadata(metadata)
                    .build()
            }
            FedEventData::TarotReadingAddedOrRemovedMod { team_id, player_id, description, r#mod, mod_duration, mod_removed, mods_removed_from_other_mod } => {
                let mut events = Vec::new();

                if let Some(from_other_mod) = mods_removed_from_other_mod {
                    let mut other_eb = eb.connected_event(from_other_mod.event);
                    other_eb.set_category(EventCategory::Changes);
                    other_eb.push_team_tag(team_id);
                    if let Some(pid) = player_id { other_eb.push_player_tag(pid); }
                    other_eb.push_description(&from_other_mod.format_description());
                    let removes: Vec<_> = from_other_mod.mods_removed.iter()
                        .map(|mod_desc| json!({ "type": mod_desc.mod_duration as i64, "mod": mod_desc.mod_id }))
                        .collect();
                    other_eb.push_metadata_json_vec("removes", removes);
                    other_eb.push_metadata_str("source", &r#mod);
                    events.push(other_eb.build(EventType::RemovedModsFromAnotherMod));
                }

                eb.set_category(EventCategory::Changes);
                eb.push_description(&description);
                eb.push_team_tag(team_id);
                if let Some(pid) = player_id { eb.push_player_tag(pid); }
                eb.push_metadata_str("mod", r#mod);
                eb.push_metadata_i64("type", mod_duration);
                events.push(eb.build(if mod_removed { EventType::RemovedMod } else { EventType::AddedMod }));

                // This is not the most elegant way to do this
                events.reverse();

                // Bypass the code that makes a single-event vec since we have multiple events
                return events;
            }
            FedEventData::BecomeTripleThreat { ref game, ref pitchers } => {
                let children = pitchers.iter()
                    .map(|pitcher| {
                        EventBuilderChild::new(&pitcher.sub_event)
                            .update(EventBuilderUpdate {
                                category: EventCategory::Changes,
                                r#type: EventType::AddedMod,
                                description: format!("{} is a Triple Threat.", pitcher.player_name),
                                team_tags: vec![pitcher.team_id],
                                player_tags: vec![pitcher.player_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "mod": "TRIPLE_THREAT",
                                "type": 0, // ?
                            }))
                    });
                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BecomeTripleThreat,
                        category: EventCategory::Special,
                        description: if let Some((pitcher_1, pitcher_2)) = pitchers.iter().collect_tuple() {
                            format!("{} and {} chug a Third Wave of Coffee!\nThey are now Triple Threats!", pitcher_1.player_name, pitcher_2.player_name)
                        } else if let Some((pitcher, )) = pitchers.iter().collect_tuple() {
                            format!("{} chugs a Third Wave of Coffee!\nThey are now a Triple Threat!", pitcher.player_name)
                        } else {
                            panic!("There should either be one or two pitchers here")
                        },
                        player_tags: pitchers.iter().map(|pitcher| pitcher.player_id).collect(),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::UnderOver { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Under Over, {}.", if on { "On" } else { "Off" });
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "UNDEROVER",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::UnderOver,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::OverUnder { ref game, team_id, player_id, ref player_name, on, ref sub_event } => {
                let description = format!("{player_name}, Over Under, {}.", if on { "On" } else { "Off" });
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "UNDERPERFORMING",
                        "source": "OVERUNDER",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::OverUnder,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::TasteTheInfinite { ref game, sheller_id, ref sheller_name, shellee_team_id, shellee_id, ref shellee_name, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: format!("{shellee_name} is Shelled!"),
                        team_tags: vec![shellee_team_id],
                        // Yes this makes no sense! but, it appears to be that way
                        player_tags: vec![sheller_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SHELLED",
                        "type": 0, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TasteTheInfinite,
                        category: EventCategory::Special,
                        description: format!("{sheller_name} tastes the infinite!\n{shellee_name} is Shelled!"),
                        player_tags: vec![sheller_id, shellee_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::BatterSkipped { game, batter_name, reason } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BatterSkipped,
                        description: match reason {
                            BatterSkippedReason::Shelled => { format!("{batter_name} is Shelled and cannot escape!") }
                            BatterSkippedReason::Elsewhere(_) => { format!("{batter_name} is Elsewhere..") }
                        },
                        // Bizarrely, the player tag is on elsewhere players but not shelled ones
                        player_tags: if let BatterSkippedReason::Elsewhere(id) = reason {
                            vec![id]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FeedbackBlocked { ref game, resisted_id, ref resisted_name, tangled_id, tangled_team_id, ref tangled_name, tangled_rating_before, tangled_rating_after, ref sub_event } => {
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatDecrease,
                        description: format!("{tangled_name} is tangled in the flicker!"),
                        team_tags: vec![tangled_team_id],
                        player_tags: vec![tangled_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": tangled_rating_before,
                        "after": tangled_rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FeedbackBlocked,
                        category: EventCategory::Special,
                        description: format!("Reality begins to flicker ...\nBut {resisted_name} resists!\n{tangled_name} is tangled in the flicker!"),
                        player_tags: vec![resisted_id, tangled_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::FlagPlanted { team_id, team_nickname, ballpark_name, prefab_name, renovation_id, votes, is_first } => {
                let flag_planted_str = if is_first {
                    "!\nTHE FLAG IS PLANTED"
                } else {
                    ".\nAnother flag is planted!"
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FlagPlanted,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} break ground on {ballpark_name}, selecting to build the {prefab_name} prefab{flag_planted_str}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "renoId": renovation_id,
                        "title": "Ground Broken",
                        "votes": votes,
                    }))
                    .build()
            }
            FedEventData::EmergencyAlert { message, team_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EmergencyAlert,
                        category: EventCategory::Outcomes,
                        description: message,
                        team_tags,
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::TeamJoinedILB { team_id, team_nickname, division_id, division_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have joined the ILB!\nThey will play in the {division_name} division."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "divisionId": division_id,
                        "divisionName": division_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::FloodingSwept { game, effects, free_refills, flood_pumps } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("A surge of Immateria rushes up from Under!");
                eb.push_description("Baserunners are swept from play!");

                for effect in effects {
                    match effect {
                        FloodingSweptEffect::Elsewhere(sent_elsewhere) => {
                            // This form uses the same text for inner and outer description
                            let description = format!("{} {} swept Elsewhere!",
                                                      sent_elsewhere.player_name,
                                                      if self.season < 18 { "is" } else { "was" });
                            eb.push_sent_elsewhere(sent_elsewhere, &description, &description);
                        }
                        FloodingSweptEffect::Flippers(PlayerNameId { player_name, player_id }) => {
                            // There's a danger that this could end a game and therefore have hype
                            // and whatever else comes with it, but I'll deal with that if and when
                            // it actually occurs
                            eb.push_description(&format!("{player_name} uses their Flippers to slingshot home!"));
                            eb.push_player_tag(player_id);
                        }
                        FloodingSweptEffect::Ego(PlayerNameId { player_name, player_id }) => {
                            eb.push_description(&format!("{player_name}'s Ego keeps them on base!"));
                            eb.push_player_tag(player_id);
                        }
                    }
                }

                if flood_pumps {
                    eb.push_description("The Flood Pumps activate!");
                }

                eb.push_free_refills(free_refills);
                eb.build(EventType::FloodingSwept)
            }
            FedEventData::ReturnFromElsewhere { game, returns } => {
                eb.set_game(game);

                for ReturnFromElsewhere { player_name, flavor } in returns {
                    match flavor {
                        ReturnFromElsewhereFlavor::Full { team_id, player_id, is_peanut, sub_event, time_elsewhere, scattered, recongealed_differently } => {
                            let returned_text = if is_peanut { "rolled back" } else { "returned" };
                            let has = if self.season < 18 { "has " } else { "" };
                            let description = format!("{player_name} {has}{returned_text} from Elsewhere after {time_elsewhere}!");
                            eb.push_description(&description);

                            if let Some(Scattered { scattered_name, sub_event }) = scattered {
                                eb.push_child(sub_event, |mut child| {
                                    child.push_description(&format!("{scattered_name} was Scattered..."));
                                    child.push_team_tag(team_id);
                                    child.push_player_tag(player_id);
                                    child.push_metadata_str("mod", "SCATTERED");
                                    child.push_metadata_i64("type", ModDuration::Permanent as i64);
                                    child.build(EventType::AddedMod)
                                });
                            }

                            eb.push_child(sub_event, |mut child| {
                                child.push_description(&description);
                                child.push_team_tag(team_id);
                                child.push_player_tag(player_id);
                                child.push_metadata_str("mod", "ELSEWHERE");
                                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                                child.build(EventType::RemovedMod)
                            });

                            if let Some(recongeal) = recongealed_differently {
                                eb.push_child(recongeal.sub_event, |mut child| {
                                    child.push_description(&format!("{} re-congealed differently.", recongeal.player_name));
                                    child.push_team_tag(recongeal.team_id);
                                    child.push_player_tag(recongeal.player_id);
                                    child.push_metadata_i64("type", ModDuration::Permanent as i64);
                                    child.build_player_stat_changed(recongeal.rating_before, recongeal.rating_after, 4)
                                });
                            }
                        }
                        ReturnFromElsewhereFlavor::Short { team_id, player_id, is_peanut, sub_event } => {
                            let description = if self.season < 18 {
                                format!("{player_name} has {} from Elsewhere!",
                                        if is_peanut { "rolled back" } else { "returned" })
                            } else {
                                format!("{player_name} {} from Elsewhere.",
                                        if is_peanut { "rolled back" } else { "returned" })
                            };
                            eb.push_description(&description);
                            eb.push_child(sub_event, |mut child| {
                                child.push_description(&description);
                                child.push_team_tag(team_id);
                                child.push_player_tag(player_id);
                                child.push_metadata_str("mod", "ELSEWHERE");
                                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                                child.build(EventType::RemovedMod)
                            });
                        }
                        ReturnFromElsewhereFlavor::False { is_peanut } => {
                            let description = format!("{player_name} has {} from Elsewhere!",
                                                      if is_peanut { "rolled back" } else { "returned" });
                            eb.push_description(&description);
                        }
                        ReturnFromElsewhereFlavor::PulledBack { team_id, sought_player_id, seeker_player_id, seeker_player_name, sub_event, time_elsewhere } => {
                            eb.push_description(&format!("{seeker_player_name} sought out Elsewhere teammate {player_name}..."));
                            eb.push_player_tag(seeker_player_id);
                            let description = format!("{player_name} was pulled back from Elsewhere after {time_elsewhere}!");
                            eb.push_description(&description);
                            eb.push_child(sub_event, |mut child| {
                                child.push_description(&description);
                                child.push_team_tag(team_id);
                                child.push_player_tag(sought_player_id);
                                child.push_metadata_str("mod", "ELSEWHERE");
                                child.push_metadata_i64("type", ModDuration::Permanent as i64);
                                child.build(EventType::RemovedMod)
                            });
                        }
                    }
                }

                eb.build(EventType::ReturnFromElsewhere)
            }
            FedEventData::Incineration { game, team_id, team_nickname, victim_id, victim_name, replacement_id, replacement_name, location, unstable_chain, sub_events, ambush } => {
                let (incin_child, enter_hall_child, hatch_child, replace_child) = sub_events;

                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_player_tag(victim_id);
                eb.push_player_tag(replacement_id);

                if unstable_chain.is_some() {
                    eb.push_description(&format!("{victim_name} is Unstable!"));
                    eb.push_description("A Debt was collected.");
                }

                eb.push_description(&format!("Rogue Umpire incinerated {victim_name}!"));
                eb.push_description(&format!("They're replaced by {replacement_name}."));

                eb.push_child(incin_child, |mut child_eb| {
                    child_eb.push_description(&format!("Rogue Umpire incinerated {victim_name}!"));
                    child_eb.push_player_tag(victim_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.build(EventType::Incineration)
                });

                eb.push_child(enter_hall_child, |mut child_eb| {
                    child_eb.push_description(&format!("{victim_name} entered the Hall of Flame."));
                    child_eb.push_player_tag(victim_id);
                    child_eb.build(EventType::EnterHallOfFlame)
                });

                eb.push_child(hatch_child, |mut child_eb| {
                    child_eb.push_description(&format!("{replacement_name} has been hatched from the field of eggs."));
                    child_eb.push_player_tag(replacement_id);
                    child_eb.push_metadata_uuid("id", replacement_id);
                    child_eb.build(EventType::PlayerHatched)
                });

                eb.push_child(replace_child, |mut child_eb| {
                    child_eb.push_description(&format!("{replacement_name} replaced the incinerated {victim_name}."));
                    child_eb.push_player_tag(victim_id);
                    child_eb.push_player_tag(replacement_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_uuid("inPlayerId", replacement_id);
                    child_eb.push_metadata_str("inPlayerName", replacement_name);
                    child_eb.push_metadata_i64("location", location);
                    child_eb.push_metadata_uuid("outPlayerId", victim_id);
                    child_eb.push_metadata_str("outPlayerName", victim_name);
                    child_eb.push_metadata_uuid("teamId", team_id);
                    child_eb.push_metadata_str("teamName", team_nickname);
                    child_eb.build(EventType::PlayerBornFromIncineration)
                });

                if let Some(unstable) = unstable_chain {
                    let unstable_desc = format!("The Instability chains to {}!", unstable.player_name);
                    eb.push_description(&unstable_desc);
                    eb.push_child(unstable.sub_event, |mut child_eb| {
                        child_eb.push_description(&unstable_desc);
                        child_eb.push_team_tag(unstable.team_id);
                        child_eb.push_player_tag(unstable.player_id);
                        child_eb.push_metadata_str("mod", "MARKED");
                        child_eb.push_metadata_i64("type", ModDuration::Weekly);
                        child_eb.build(EventType::AddedMod)
                    });
                }

                if let Some(ambush) = ambush {
                    eb.push_description("An Ambush.");
                    eb.push_description(&format!("{} enters the {} shadows.", ambush.player_name, possessive(ambush.team_nickname.clone())));
                    eb.push_child(ambush.exit_hall_event, |mut child_eb| {
                        child_eb.push_description(&format!("{} exited the Hall of Flame", ambush.player_name));
                        child_eb.push_player_tag(ambush.player_id);
                        child_eb.build(EventType::ExitHallOfFlame)
                    });
                    eb.push_child(ambush.added_to_team_event, |mut child_eb| {
                        child_eb.push_description(&format!("{} joins the Ambush.", ambush.player_name));
                        child_eb.push_player_tag(ambush.player_id);
                        child_eb.push_team_tag(ambush.team_id);
                        child_eb.push_metadata_uuid("playerId", ambush.player_id);
                        child_eb.push_metadata_str("playerName", &ambush.player_name);
                        child_eb.push_metadata_uuid("teamId", ambush.team_id);
                        child_eb.push_metadata_str("teamName", &ambush.team_nickname);
                        // Ambush didn't exist until after shadows were unified, so the location
                        // is always "shadows" (location 2)
                        child_eb.push_metadata_i64("location", 2);
                        child_eb.build(EventType::PlayerAddedToTeam)
                    });
                    eb.push_child(ambush.shadow_boost_event, |mut child_eb| {
                        child_eb.push_description(&format!("{} entered the Shadows.", ambush.player_name));
                        child_eb.push_player_tag(ambush.player_id);
                        child_eb.push_team_tag(ambush.team_id);
                        child_eb.push_metadata_f64("after", ambush.player_rating_after);
                        child_eb.push_metadata_f64("before", ambush.player_rating_before);
                        child_eb.push_metadata_i64("type", 4); // 4 = "all categories"
                        child_eb.build(EventType::PlayerStatIncrease)
                    });
                }

                eb.build(EventType::Incineration)
                // let location_int: i64 = location.into();
                // let mut prefix = String::new();
                // let mut suffix = String::new();
                // let mut children = vec![
                //     EventBuilderChild::new(incin_child)
                //         .update(EventBuilderUpdate {
                //             category: ,
                //             r#type: ,
                //             description: format!("Rogue Umpire incinerated {victim_name}!"),
                //             team_tags: vec![team_id],
                //             player_tags: vec![victim_id],
                //             ..Default::default()
                //         }),
                //     EventBuilderChild::new(enter_hall_child)
                //         .update(EventBuilderUpdate {
                //             category: EventCategory::Changes,
                //             r#type: EventType::EnterHallOfFlame,
                //             description: format!("{victim_name} entered the Hall of Flame."),
                //             player_tags: vec![victim_id],
                //             ..Default::default()
                //         }),
                //     EventBuilderChild::new(hatch_child)
                //         .update(EventBuilderUpdate {
                //             category: EventCategory::Changes,
                //             r#type: EventType::PlayerHatched,
                //             description: format!("{replacement_name} has been hatched from the field of eggs."),
                //             player_tags: vec![replacement_id],
                //             ..Default::default()
                //         })
                //         .metadata(json!({ "id": replacement_id })),
                //     EventBuilderChild::new(replace_child)
                //         .update(EventBuilderUpdate {
                //             category: EventCategory::Changes,
                //             r#type: EventType::PlayerBornFromIncineration,
                //             description: format!("{replacement_name} replaced the incinerated {victim_name}."),
                //             team_tags: vec![team_id],
                //             player_tags: vec![victim_id, replacement_id],
                //             ..Default::default()
                //         })
                //         .metadata(json!({
                //             "inPlayerId": replacement_id,
                //             "inPlayerName": replacement_name,
                //             "location": location_int,
                //             "outPlayerId": victim_id,
                //             "outPlayerName": victim_name,
                //             "teamId": team_id,
                //             "teamName": team_nickname,
                //         })),
                // ];
                //
                // if let Some(chain) = unstable_chain {
                //     prefix = format!("{victim_name} is Unstable!\nA Debt was collected.\n");
                //     suffix = format!("\nThe Instability chains to {}!", chain.player_name);
                //     children.push(
                //         EventBuilderChild::new(&chain.sub_event)
                //             .update(EventBuilderUpdate {
                //                 category: EventCategory::Changes,
                //                 r#type: EventType::AddedMod,
                //                 description: format!("The Instability chains to {}!", chain.player_name),
                //                 team_tags: vec![chain.team_id],
                //                 player_tags: vec![chain.player_id],
                //                 ..Default::default()
                //             })
                //             .metadata(json!({
                //                 "mod": "MARKED",
                //                 "type": ModDuration::Weekly as i64,
                //             }))
                //     )
                // }
                //
                // event_builder.for_game(game)
                //     .fill(EventBuilderUpdate {
                //         r#type: EventType::Incineration,
                //         category: EventCategory::Special,
                //         description: format!("{prefix}Rogue Umpire incinerated {victim_name}!\nThey're replaced by {replacement_name}.{suffix}"),
                //         player_tags: vec![victim_id, replacement_id],
                //         ..Default::default()
                //     })
                //     .children(children)
                //     .build()
            }
            FedEventData::PitcherChange { game, team_nickname: team_name, pitcher_id, pitcher_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PitcherChange,
                        description: format!("{pitcher_name} is now pitching for the {team_name}."),
                        player_tags: vec![pitcher_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Party { ref game, team_id, player_id, ref player_name, ref sub_event, rating_before, rating_after } => {
                let description = format!("{player_name} is Partying!");
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatIncrease,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Party,
                        description,
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerHatched { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerHatched,
                        category: EventCategory::Changes,
                        description: format!("{player_name} has been hatched from the field of eggs."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PostseasonBirth { team_id, team_nickname, player_id, player_name, location } => {
                let location_int: i64 = location.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerAddedToTeam,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} earn a Postseason Birth!"),
                        player_tags: vec![player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": location_int,
                        "playerId": player_id,
                        "playerName": player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::FinalStandings { team_id, team_nickname, place, division_name } => {
                let place_str = match place {
                    0 => "1st".to_string(),
                    1 => "2nd".to_string(),
                    2 => "3rd".to_string(),
                    _ => format!("{}th", place + 1),
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::FinalStandings,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} finished {place_str} in the {division_name}."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "place": place }))
                    .build()
            }
            FedEventData::TeamLeftPartyTimeForPostseason { team_id, team_nickname } => {
                // TODO This was combined into another event, should it be deleted?
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have been removed from Party Time to join the Postseason!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "PARTY_TIME",
                        "type": 1, // ?
                    }))
                    .build()
            }
            FedEventData::EarnedPostseasonSlot { team_id, team_nickname, postseason_birth_name, postseason_birth_id, postseason_birth_location, hatch_event_metadata, postseason_birth_event_metadata, shadow_boost, left_party_event_metadata } => {
                let mut hatch_eb = eb.connected_event(hatch_event_metadata);
                hatch_eb.set_category(EventCategory::Changes);
                hatch_eb.push_description(&format!("{postseason_birth_name} has been hatched from the field of eggs."));
                hatch_eb.push_player_tag(postseason_birth_id);
                hatch_eb.push_metadata_uuid("id", postseason_birth_id);
                let hatch_event = hatch_eb.build(EventType::PlayerHatched);

                let birth_event = postseason_birth_event_metadata.map(|postseason_birth_event_metadata| {
                    let mut birth_eb = eb.connected_event(postseason_birth_event_metadata);
                    birth_eb.set_category(EventCategory::Changes);
                    birth_eb.push_description(&format!("The {team_nickname} earn a Postseason Birth!"));
                    birth_eb.push_player_tag(postseason_birth_id);
                    birth_eb.push_team_tag(team_id);
                    birth_eb.push_metadata_i64("location", postseason_birth_location);
                    birth_eb.push_metadata_uuid("playerId", postseason_birth_id);
                    birth_eb.push_metadata_str("playerName", &postseason_birth_name);
                    birth_eb.push_metadata_uuid("teamId", team_id);
                    birth_eb.push_metadata_str("teamName", &team_nickname);
                    birth_eb.build(EventType::PlayerAddedToTeam)
                });

                let party_event = left_party_event_metadata.map(|left_party_time| {
                    let mut party_eb = eb.connected_event(left_party_time);
                    party_eb.set_category(EventCategory::Changes);
                    party_eb.push_description(&format!("The {team_nickname} have been removed from Party Time to join the Postseason!"));
                    party_eb.push_team_tag(team_id);
                    party_eb.push_metadata_str("mod", "PARTY_TIME");
                    party_eb.push_metadata_i64("type", ModDuration::Seasonal);
                    party_eb.build(EventType::RemovedMod)
                });

                let order = shadow_boost.as_ref().map(|(boost, order)| *order);
                let shadow_event = shadow_boost.map(|(boost, _)| {
                    let mut shadow_eb = eb.connected_event(boost.sub_event);
                    shadow_eb.set_category(EventCategory::Changes);
                    shadow_eb.push_description(&format!("{postseason_birth_name} entered the Shadows."));
                    shadow_eb.push_team_tag(team_id);
                    shadow_eb.push_player_tag(postseason_birth_id);
                    shadow_eb.push_known_boost(&boost);
                    shadow_eb.build(EventType::PlayerStatIncrease)
                });

                eb.set_category(EventCategory::Outcomes);
                eb.push_description(&format!("The {team_nickname} earned a spot in the Season {} Postseason.", self.season + 1));
                eb.push_team_tag(team_id);
                let main_event = eb.build(EventType::EarnedPostseasonSlot);

                return match order {
                    None | Some(PostseasonBirthBoostEventOrder::AfterBirth) => [Some(hatch_event), birth_event, shadow_event, party_event, Some(main_event)],
                    Some(PostseasonBirthBoostEventOrder::AfterHatch) => [Some(hatch_event), shadow_event, birth_event, party_event, Some(main_event)],
                    Some(PostseasonBirthBoostEventOrder::AfterEarnedSlot) => [Some(hatch_event), birth_event, party_event, Some(main_event), shadow_event],
                }.into_iter().filter_map(|x| x).collect();
            }
            FedEventData::PostseasonAdvance { team_id, team_nickname, round, displayed_season: season } => {
                let round_str = if let Some(round) = round {
                    format!("Round {round}")
                } else {
                    String::from("The Internet Series")
                };
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PostseasonAdvance,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} advanced to {round_str} of the Season {season} Postseason."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PostseasonEliminated { team_id, team_nickname, displayed_season: season } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PostseasonEliminated,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} have been eliminated from the Season {season} Postseason."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::PlayerBoosted { team_id, player_id, player_name, rating_before, rating_after } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
                        category: EventCategory::Changes,
                        description: format!("{player_name} was boosted."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }))
                    .build()
            }
            FedEventData::TeamEnteredPartyTime { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} have entered Party Time!"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "PARTY_TIME",
                        "type": 1
                    }))
                    .build()
            }
            FedEventData::TeamWonInternetSeries { team_id, team_nickname, championships } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::TeamWonInternetSeries,
                        category: EventCategory::Outcomes,
                        description: format!("The {team_nickname} won the Season {} Internet Series!", self.season + 1),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "championships": championships
                    }))
                    .build()
            }
            FedEventData::BottomDwellers { team_id, team_nickname, rating_before, rating_after } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerStatIncrease,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} are Bottom Dwellers."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 5, // ?
                    }))
                    .build()
            }
            FedEventData::WillReceived { team_id, will_title, metadata } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::WillRecieved,
                        category: EventCategory::Outcomes,
                        description: format!("Will Received: {will_title}"),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::BlessingWon { team_tags, blessing_title, metadata } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(&format!("Blessing Won: {blessing_title}"));
                eb.set_team_tags(team_tags);
                eb.set_full_metadata(metadata);
                eb.build(EventType::BlessingOrGiftWon)
            }
            FedEventData::SubseasonalModsChange { game, changes } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                // I'm guessing the last mod determines the event type?
                let event_type = changes.last()
                    .expect("SubseasonalModsChange should never have an empty changes vec")
                    .source_mod
                    .event_type();

                push_subseasonal_mod_changes(&mut eb, changes, self.season);

                eb.build(event_type)
            }
            FedEventData::DecreePassed { decree_title, metadata } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::DecreePassed,
                        category: EventCategory::Outcomes,
                        description: format!("Decree Passed: {decree_title}"),
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::PlayerJoinedILB { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("{player_name} has joined the ILB."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PlayerPermittedToStay { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerPermittedToStay,
                        category: EventCategory::Special,
                        description: format!("{player_name} has been permitted to stay."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::FireproofIncineration { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::IncinerationBlocked,
                        category: EventCategory::Special,
                        description: format!("Rogue Umpire tried to incinerate {player_name}, but they're Fireproof! The Umpire was incinerated instead!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::LineupSorted { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::LineupSorted,
                        category: EventCategory::Changes,
                        description: format!("The {} lineup has been optimized.", possessive(team_nickname)),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::Undersea { ref game, ref team_name, team_id, ref sub_event } => {
                let description = format!("The {team_name} go Undersea. They're now Overperforming!");
                let child = EventBuilderChild::new(sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedModFromOtherMod,
                        category: EventCategory::Changes,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "UNDERSEA",
                        "type": 3, // ?
                    }));

                event_builder.for_game(game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Undersea,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::RenovationBuilt { team_id, description, renovation_id, renovation_title, votes, mod_add_event } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(description);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("renoId", renovation_id);
                eb.push_metadata_str("title", renovation_title);
                match votes {
                    RenovationVotes::Normal(v) => { eb.push_metadata_i64("votes", v) }
                    RenovationVotes::Manual(v) => { eb.push_metadata_str("votes", v) }
                }

                if let Some(mod_add) = mod_add_event {
                    eb.push_child(mod_add.sub_event, |mut child_eb| {
                        child_eb.set_description(mod_add.description);
                        child_eb.push_team_tag(team_id);
                        child_eb.push_metadata_str("mod", mod_add.mod_id);
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        // Grumble grumble inconsistency
                        child_eb.clear_sub_play();
                        child_eb.build(EventType::AddedMod)
                    });
                }

                eb.build(EventType::RenovationBuilt)
            }
            FedEventData::PeanutMister { game, player_id, player_name, superallergy } => {
                let effect_str = if superallergy.is_some() {
                    "is no longer Superallergic"
                } else {
                    "has been cured of their peanut allergy"
                };

                let child = superallergy.map(|superallergy| {
                    EventBuilderChild::new(&superallergy.sub_event)
                        .update(EventBuilderUpdate {
                            r#type: EventType::RemovedMod,
                            category: EventCategory::Changes,
                            description: format!("{player_name} lost the Superallergic mod."),
                            player_tags: vec![player_id],
                            team_tags: vec![superallergy.team_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": "SUPERALLERGIC",
                            "type": 0, // ?
                        }))
                });

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PeanutMister,
                        category: EventCategory::Special,
                        description: format!("The Peanut Mister activates!\n{player_name} {effect_str}!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .children(child)
                    .build()
            }
            FedEventData::PlayerNamedMvp { team_id, player_id, player_name, level } => {
                let mod_name = format!("EGO{level}");
                if level == 1 {
                    event_builder
                        .fill(EventBuilderUpdate {
                            r#type: EventType::AddedMod,
                            category: EventCategory::Changes,
                            description: format!("{player_name} is named an MVP."),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "mod": mod_name,
                            "type": 0,
                        }))
                        .build()
                } else {
                    let prev_mod_name = format!("EGO{}", level - 1);
                    event_builder
                        .fill(EventBuilderUpdate {
                            r#type: EventType::ModChange,
                            category: EventCategory::Changes,
                            description: format!("{player_name} is named a {level}-Time MVP{}",
                                                 // i dont like this
                                                 if level == 2 { "." } else { "!" }),
                            team_tags: vec![team_id],
                            player_tags: vec![player_id],
                            ..Default::default()
                        })
                        .metadata(json!({
                            "from": prev_mod_name,
                            "to": mod_name,
                            "type": 0,
                        }))
                        .build()
                }
            }
            FedEventData::BirdsUnshell { game, team_id, player_id, player_name, pecked_free_event, superallergy_event } => {
                let pecked_free_child = EventBuilderChild::new(&pecked_free_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The Birds pecked {player_name} free!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SHELLED",
                        "type": 0, // ?
                    }));

                let superallergy_child = EventBuilderChild::new(&superallergy_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} emerges from the shell with a Superallergy!"),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "SUPERALLERGIC",
                        "type": 0, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::BirdsUnshell,
                        category: EventCategory::Special,
                        description: format!("The Birds circle...\nThe Birds pecked {player_name} free!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(pecked_free_child)
                    .child(superallergy_child)
                    .build()
            }
            FedEventData::ReplaceReturnedPlayerFromShadows { team_id, team_nickname, promoted_player_id, promoted_player_name, promoted_location, removed_player_id, removed_player_name, removed_location } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerReplacesReturned,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} cut a player and promoted another from the shadows."),
                        player_tags: vec![removed_player_id, promoted_player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "promoteLocation": promoted_location as i64,
                        "promotePlayerId": promoted_player_id,
                        "promotePlayerName": promoted_player_name,
                        "removeLocation": removed_location as i64,
                        "removePlayerId": removed_player_id,
                        "removePlayerName": removed_player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::PlayerCalledBackToHall { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterHallOfFlame,
                        category: EventCategory::Changes,
                        description: format!("{player_name} entered the Hall of Flame."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::TeamUsedFreeWill { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} used their Free Will."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "FREE_WILL",
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::PlayerLostMod { team_id, player_id, player_name, r#mod, mod_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("{player_name} lost the {mod_name} mod."),
                        team_tags: vec![team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": r#mod,
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::InvestigationMessage { player_id, message } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::InvestigationMessage,
                        category: EventCategory::Special,
                        description: message,
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::HighPressure { game, team_id, team_nickname, is_on, sub_event } => {
                let description = if is_on {
                    format!("The pressure is on! The {team_nickname} are Overperforming.")
                } else {
                    format!("The pressure is off. The {team_nickname} are no longer Overperforming.")
                };

                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        r#type: if is_on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod },
                        category: EventCategory::Changes,
                        description: description.clone(),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "OVERPERFORMING",
                        "source": "HIGH_PRESSURE",
                        "type": 3, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::HighPressure,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::PlayerPulledThroughRift { player_id, player_name } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerDivisionMove,
                        category: EventCategory::Changes,
                        description: format!("{player_name} was pulled through the Rift."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({ "id": player_id }))
                    .build()
            }
            FedEventData::PlayerLocalized { team_id, team_nickname, player_id, player_name, location } => {
                let location_int: i64 = location.into();
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerAddedToTeam,
                        category: EventCategory::Changes,
                        description: format!("{player_name} Localized into the {} {}.", possessive(team_nickname.clone()), location.location()),
                        player_tags: vec![player_id],
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": location_int,
                        "playerId": player_id,
                        "playerName": player_name,
                        "teamId": team_id,
                        "teamName": team_nickname,
                    }))
                    .build()
            }
            FedEventData::Echo { game, echoee_name, primary_echo: main_echo, receiver_echos: sub_echos, } => {
                let make_children_for_echo = |echo: Echo, mod_type: i64, source: &str, echo_description: &str| {
                    let child_removed = echo.mods_removed.map(|mods_removed| {
                        let removes: Vec<_> = mods_removed.mod_ids.into_iter()
                            .map(|mod_id| json!({ "type": mod_type, "mod": mod_id }))
                            .collect();

                        EventBuilderChild::new(&mods_removed.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::RemovedModsFromAnotherMod,
                                category: EventCategory::Changes,
                                description: format!("{}'s {}Echo faded.", echo.receiver_name,
                                                     if mod_type == 0 { "" } else { "Echoed " }),
                                player_tags: vec![echo.receiver_id],
                                team_tags: vec![echo.receiver_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "removes": removes,
                                "source": source,
                            }))
                    });
                    let child_added = {
                        let adds: Vec<_> = echo.mods_added.mod_ids.into_iter()
                            .map(|mod_id| json!({ "type": mod_type, "mod": mod_id }))
                            .collect();

                        EventBuilderChild::new(&echo.mods_added.sub_event)
                            .update(EventBuilderUpdate {
                                r#type: EventType::AddedModsFromAnotherMod,
                                category: EventCategory::Changes,
                                description: format!("{}{echo_description}!", echo.receiver_name),
                                player_tags: vec![echo.receiver_id],
                                team_tags: vec![echo.receiver_team_id],
                                ..Default::default()
                            })
                            .metadata(json!({
                                "adds": adds,
                                "source": source,
                            }))
                    };

                    (child_removed, child_added)
                };

                let receiver_echo_description = format!("'s Echoed an Echo from {}", main_echo.receiver_name);
                let main_echo_children = make_children_for_echo(main_echo, 0, "ECHO",
                                                                &format!(" Echoed {echoee_name}"));
                let sub_echo_children = sub_echos.into_iter()
                    .map(|sub_echo| make_children_for_echo(sub_echo, 1, "RECEIVER",
                                                           &receiver_echo_description));

                let description = main_echo_children.1.update.description.clone();
                let children = iter::once(main_echo_children)
                    .chain(sub_echo_children)
                    .map(|(removed, added)| [removed, Some(added)])
                    .flatten() // This one should flatten the array
                    .flatten() // This one should flatten the options
                    .collect_vec(); // for debugging

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Echo,
                        category: EventCategory::Special,
                        description,
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::SolarPanelsAwait { game } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::SolarPanelsAwait,
                        category: EventCategory::Special,
                        description: "The Solar Panels are angled toward Sun 2.".to_string(),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EchoIntoStatic { game, echoer, echoee } => {
                let description = format!("ECHO {} STATIC\nECHO {} STATIC", echoer.player_name, echoee.player_name);

                let make_sub_event = |echo: &EchoIntoStatic, sub_event: &SubEvent, event_type: EventType| {
                    let child = EventBuilderChild::new(sub_event)
                        .update(EventBuilderUpdate {
                            r#type: event_type,
                            category: EventCategory::Changes,
                            description: description.clone(),
                            player_tags: vec![echo.player_id],
                            team_tags: vec![echo.team_id],
                            ..Default::default()
                        });

                    if event_type == EventType::PlayerRemovedFromTeam {
                        child.metadata(json!({
                            "playerId": echo.player_id,
                            "playerName": echo.player_name,
                            "teamId": echo.team_id,
                            "teamName": echo.team_nickname,
                        }))
                    } else {
                        child.metadata(json!({
                            "from": "ECHO",
                            "to": "STATIC",
                            "type": 0,
                        }))
                    }
                };

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoIntoStatic,
                        category: EventCategory::Special,
                        description: description.clone(),
                        ..Default::default()
                    })
                    .child(make_sub_event(&echoer, &echoer.removed_from_team_sub_event,
                                          EventType::PlayerRemovedFromTeam))
                    .child(make_sub_event(&echoee, &echoee.removed_from_team_sub_event,
                                          EventType::PlayerRemovedFromTeam))
                    .child(make_sub_event(&echoer, &echoer.mod_changed_sub_event,
                                          EventType::ModChange))
                    .child(make_sub_event(&echoee, &echoee.mod_changed_sub_event,
                                          EventType::ModChange))
                    .build()
            }
            FedEventData::ConsumerAttack { game, team_id, player_id, player_name_all_caps: player_name, effect, sensed_something_fishy, scattered } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_player_tag(player_id);
                eb.push_description("CONSUMERS ATTACK");
                if scattered {
                    eb.push_description("SCATTERED");
                }

                match effect {
                    ConsumerAttackEffect::Chomp { rating_before, rating_after, sub_event } => {
                        eb.push_description(&player_name);
                        let description = eb.description().to_string();
                        eb.push_child(sub_event, |mut child| {
                            child.push_player_tag(player_id);
                            child.push_team_tag(team_id);
                            child.set_description(description);
                            child.build_player_stat_changed(rating_before, rating_after, StatChangeCategory::All as i64)
                        });
                    }
                    ConsumerAttackEffect::DefendedWithItem(damage) => {
                        // Sticking the extra \n here arbitrarily. There are two in a row.
                        eb.push_description(&format!("{player_name} DEFENDS\n"));
                        eb.push_description(&format!("{} {}",
                                                     damage.item_name.to_ascii_uppercase(),
                                                     if damage.health > 0 { "DAMAGED" } else if damage.item_name.ends_with('s') { "BREAK" } else { "BREAKS" }));
                        let description = eb.description().to_string();
                        eb.push_child(damage.sub_event, |mut child| {
                            child.set_description(description);
                            child.build_item_damaged(damage)
                        });
                    }
                }

                if let Some(fishy) = sensed_something_fishy {
                    eb.push_child(fishy.sub_event, |mut child| {
                        child.push_description(&format!("{} sensed something fishy.", fishy.detective_name));
                        child.build_detective_activity(fishy)
                    });
                }

                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::Psychoacoustics { game, stadium_name, team_id, team_nickname, mod_name, mod_id, sub_event, subseasonal_mod_effects } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                push_subseasonal_mod_changes(&mut eb, subseasonal_mod_effects, self.season);

                let description = format!("{stadium_name} is Resonating.\nPsychoAcoustics Echo {mod_name} {} the {team_nickname}.",
                                          if (self.season, self.day) < (15, 33) { "at" } else { "to" });
                if (self.season, self.day) > (15, 32) { // tgb did a whoopsie
                    eb.push_description(&description);
                }

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(&description);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", mod_id);
                    child_eb.push_metadata_str("source", "PSYCHOACOUSTICS");
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(EventType::AddedModFromOtherMod)
                });

                eb.build(EventType::Psychoacoustics)
            }
            FedEventData::EchoReceiver { game, echoer_name, echoee_name, echoee_id, echoee_team_id, sub_event } => {
                let description = format!("ECHO {echoer_name} ECHO {echoee_name} ECHO");
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        r#type: EventType::ModChange,
                        category: EventCategory::Changes,
                        description: description.clone(),
                        player_tags: vec![echoee_id],
                        team_tags: vec![echoee_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "from": "RECEIVER",
                        "to": "ECHO",
                        "type": 0,
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoReciever,
                        category: EventCategory::Special,
                        description,
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::TeamGainedFreeWill { team_id, team_nickname } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::AddedMod,
                        category: EventCategory::Changes,
                        description: format!("The {team_nickname} gain Free Will."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "FREE_WILL",
                        "type": 0,
                    }))
                    .build()
            }
            FedEventData::Tidings { message, metadata, player_tags } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::Tidings,
                        category: EventCategory::Outcomes,
                        description: message,
                        player_tags,
                        ..Default::default()
                    })
                    .full_metadata(metadata)
                    .build()
            }
            FedEventData::HomebodyGameStart { game, homebodies } => {
                let (descriptions, children): (Vec<_>, Vec<_>) = homebodies.into_iter()
                    .map(|toggle| {
                        let description = format!("{} is {}.", toggle.player_name,
                                                  if toggle.is_overperforming { "happy to be home" } else { "homesick" });
                        let change_event = make_switch_performing_child(&toggle, &description, "HOMEBODY");
                        (description, change_event)
                    })
                    .unzip();

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        category: EventCategory::Special,
                        r#type: EventType::Homebody,
                        description: descriptions.into_iter().join("\n"),
                        ..Default::default()
                    })
                    .children(children)
                    .build()
            }
            FedEventData::SalmonSwim { game, inning_num, run_losses, item_repaired: item_restored, player_expelled } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(player_expelled.is_some()));
                eb.push_description("The Salmon swim upstream!");
                eb.push_description(&format!("Inning {inning_num} begins again."));
                eb.push_description(&run_losses.to_string());

                if let Some(item_restored) = item_restored {
                    let item_base_name = item_restored.item_name.split(" of ").next()
                        .expect("API of split doesn't allow for empty iterator, I think");
                    let restored_description = format!(
                        "{} {} {} {}",
                        Possessive(&item_restored.player_name), item_restored.item_name,
                        // Not sure if the code looks for plurals or if the item has a plural flag.
                        // I'll try looking for plurals first and if that fails I'll add a tag.
                        if item_base_name.ends_with('s') { "were" } else { "was" },
                        if item_restored.health_before == 0 { "restored!" } else { "repaired." },
                    );
                    eb.push_description(&restored_description);
                    eb.push_child(item_restored.sub_event, |mut child| {
                        // Yes, the parent says swim and the child says swam
                        child.push_description("The Salmon swam upstream!");
                        child.push_description(&restored_description);
                        child.build_item_repaired(item_restored)
                    });
                }

                if let Some(sent_elsewhere) = player_expelled {
                    eb.push_player_tag(sent_elsewhere.player_id);
                    let outer_description = format!("{} is caught in the bind!", sent_elsewhere.player_name);
                    let inner_description = format!("Salmon Cannons expelled {} Elsewhere.", sent_elsewhere.player_name);
                    eb.push_sent_elsewhere(sent_elsewhere, &outer_description, &inner_description);
                }

                eb.build(EventType::SalmonSwim)
            }
            FedEventData::HitByPitch { game, pitcher_id, pitcher_name, batter_team_id, batter_id, batter_name, sub_event, scores } => {
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: format!("{batter_name} is now being Observed..."),
                        team_tags: vec![batter_team_id],
                        player_tags: vec![batter_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "COFFEE_PERIL",
                        "type": 2, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::HitByPitch,
                        category: EventCategory::Special,
                        description: format!("{pitcher_name} hits {batter_name} with a pitch!\n{batter_name} is now being Observed..."),
                        player_tags: vec![pitcher_id, batter_id],
                        ..Default::default()
                    })
                    .scores(&scores, " scores!")
                    .child(child)
                    .build()
            }
            FedEventData::SolarPanelsActivate { game, num_runs, team_nickname } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::SolarPanelsActivation,
                        category: EventCategory::Special,
                        description: format!("The Solar Panels absorb Sun 2's energy!\n{num_runs} Runs are collected and saved for the {team_nickname}'s next game."),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::RunsOverflowing { game, team_nickname, num_runs } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RunsOverflowing,
                        category: EventCategory::Special,
                        description: format!("Runs are Overflowing!\n{team_nickname} gain {}.",
                                             if num_runs == -1. {
                                                 format!("1 Unrun")
                                             } else if num_runs == 1. {
                                                 format!("1 Run")
                                             } else if num_runs < 0. {
                                                 format!("{} Unruns", -num_runs)
                                             } else {
                                                 format!("{num_runs} Runs")
                                             }),
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EnterCrimeScene { game, player_id, player_name, previous_team_id, previous_team_name, previous_location, new_team_id, new_team_name, stadium_name, rating_before, rating_after, enter_crime_scene_sub_event: crime_scene_sub_event, enter_shadows_sub_event } => {
                let crime_child = EventBuilderChild::new(&crime_scene_sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerMoved,
                        description: format!("{player_name} entered the Crime Scene at {stadium_name} to Investigate..."),
                        team_tags: vec![previous_team_id, new_team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": previous_location as i64,
                        "playerId": player_id,
                        "playerName": player_name,
                        "receiveLocation": 3,
                        "receiveTeamId": new_team_id,
                        "receiveTeamName": new_team_name,
                        "sendTeamId": previous_team_id,
                        "sendTeamName": previous_team_name,
                    }));
                let shadows_child = EventBuilderChild::new(&enter_shadows_sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::PlayerStatIncrease,
                        description: format!("{player_name} entered the Shadows."),
                        team_tags: vec![new_team_id],
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "before": rating_before,
                        "after": rating_after,
                        "type": 4, // ?
                    }));

                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterCrimeScene,
                        category: EventCategory::Special,
                        description: format!("{player_name} enters the Crime Scene at {stadium_name} to Investigate..."),
                        ..Default::default()
                    })
                    .child(crime_child)
                    .child(shadows_child)
                    .build()
            }
            FedEventData::ReturnFromInvestigation { player_id, player_name, previous_team_id, previous_team_name, new_location, new_team_id, new_team_name, emptyhanded } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::PlayerMoved,
                        category: EventCategory::Changes,
                        description: format!("{player_name} returns from the Investigation{}.",
                                             if emptyhanded { " emptyhanded" } else { "" }),
                        player_tags: vec![player_id],
                        team_tags: vec![previous_team_id, new_team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "location": 3,
                        "playerId": player_id,
                        "playerName": player_name,
                        "receiveLocation": new_location as i64,
                        "receiveTeamId": new_team_id,
                        "receiveTeamName": new_team_name,
                        "sendTeamId": previous_team_id,
                        "sendTeamName": previous_team_name,
                    }))
                    .build()
            }
            FedEventData::InvestigationConcluded { stadium_name, team_id } => {
                event_builder
                    .fill(EventBuilderUpdate {
                        r#type: EventType::RemovedMod,
                        category: EventCategory::Changes,
                        description: format!("The Crime Scene Investigation at {stadium_name} has concluded."),
                        team_tags: vec![team_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": "CRIME_SCENE",
                        "type": 0, // ?
                    }))
                    .build()
            }
            FedEventData::GrindRail { game, player_id, player_name, first_trick, success } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::GrindRail,
                        category: EventCategory::Special,
                        description: format!("{player_name} hops on the Grind Rail toward third base.\nThey do a {first_trick}!\n{success}"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EnterSecretBase { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EnterSecretBase,
                        category: EventCategory::Special,
                        description: format!("{player_name} enters the Secret Base..."),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::ExitSecretBase { game, player_id, player_name } => {
                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::ExitSecretBase,
                        category: EventCategory::Special,
                        description: format!("{player_name} exits the Secret Base to Second Base!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .build()
            }
            FedEventData::EchoChamber { game, team_id, player_id, player_name, which_mod, sub_event } => {
                let mod_id = match which_mod {
                    EchoChamberModAdded::Repeating => { "REPEATING" }
                    EchoChamberModAdded::Reverberating => { "REVERBERATING" }
                };
                let child = EventBuilderChild::new(&sub_event)
                    .update(EventBuilderUpdate {
                        category: EventCategory::Changes,
                        r#type: EventType::AddedMod,
                        description: "The Echo Chamber traps a wave.".to_string(),
                        team_tags: team_id.into_iter().collect(),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": mod_id,
                        "type": 3, // ?
                    }));


                event_builder.for_game(&game)
                    .fill(EventBuilderUpdate {
                        r#type: EventType::EchoChamber,
                        category: EventCategory::Special,
                        description: format!("The Echo Chamber traps a wave.\n{player_name} is temporarily {which_mod}!"),
                        player_tags: vec![player_id],
                        ..Default::default()
                    })
                    .child(child)
                    .build()
            }
            FedEventData::Roam { player_id, player_name, location, new_team_id, new_team_nickname, roam_from: RoamFromLocation::Team { previous_team_id, previous_team_nickname, good_riddance_parties } } => {
                let mut events = Vec::with_capacity(good_riddance_parties.len() + 1);

                for party in good_riddance_parties {
                    let mut party_eb = eb.connected_event(party.sub_event);
                    party_eb.set_category(EventCategory::Changes);
                    party_eb.push_description(&format!("{} is Partying!", party.player_name));
                    party_eb.push_player_tag(party.player_id);
                    party_eb.push_team_tag(previous_team_id);
                    party_eb.push_metadata_f64("before", party.rating_before);
                    party_eb.push_metadata_f64("after", party.rating_after);
                    party_eb.push_metadata_i64("type", 4); // "all categories"
                    events.push(party_eb.build(EventType::PlayerStatIncrease));
                }

                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{player_name} {} to a new team.",
                                             if self.season < 17 { "wandered" } else { "roamed" }));
                eb.push_player_tag(player_id);
                eb.push_team_tag(previous_team_id);
                eb.push_team_tag(new_team_id);
                eb.push_metadata_i64("location", location);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", player_name);
                eb.push_metadata_i64("receiveLocation", location);
                eb.push_metadata_uuid("receiveTeamId", new_team_id);
                eb.push_metadata_str("receiveTeamName", new_team_nickname);
                eb.push_metadata_uuid("sendTeamId", previous_team_id);
                eb.push_metadata_str("sendTeamName", previous_team_nickname);
                events.insert(0, eb.build(EventType::PlayerMoved));

                return events;
            }
            FedEventData::Roam { player_id, player_name, location, new_team_id, new_team_nickname, roam_from: RoamFromLocation::HallOfFlame { sub_event } } => {
                let mut team_eb = eb.connected_event(sub_event);
                team_eb.set_category(EventCategory::Changes);
                team_eb.push_description(&format!("{player_name} roamed to The {new_team_nickname}."));
                team_eb.push_player_tag(player_id);
                team_eb.push_team_tag(new_team_id);
                team_eb.push_metadata_i64("location", location);
                team_eb.push_metadata_uuid("playerId", player_id);
                team_eb.push_metadata_str("playerName", &player_name);
                team_eb.push_metadata_uuid("teamId", new_team_id);
                team_eb.push_metadata_str("teamName", new_team_nickname);
                let added_to_team_event = team_eb.build(EventType::PlayerAddedToTeam);

                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{player_name} roamed out of the Hall of Flame."));
                eb.push_player_tag(player_id);
                let left_hall_event = eb.build(EventType::ExitHallOfFlame);

                return vec![left_hall_event, added_to_team_event];
            }
            FedEventData::GlitterCrate { game, player_name, gained_item } => {
                eb.set_game(game);
                eb.push_description("A shimmering Crate descends.");
                eb.push_gained_item(player_name, gained_item);
                eb.build(EventType::GlitterCrateDrop)
            }
            FedEventData::ModsFromAnotherModRemoved { team_id, player_id, player_name, mods_removed, source_mod_name, source_mod_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{} mods caused by {source_mod_name} were removed.", Possessive(&player_name)));
                eb.push_player_tag(player_id);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("source", source_mod_id);
                eb.push_metadata_json_vec("removes", mods_removed.iter()
                    .map(|r| json!({ "mod": r.mod_id, "type": r.mod_duration as i64 }))
                    .collect());

                eb.build(EventType::RemovedModsFromAnotherMod)
            }
            FedEventData::ConsumerExpelled { game, player_id } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("SALMON CANNONS FIRE");
                eb.push_description("CONSUMER EXPELLED");
                eb.push_player_tag(player_id);
                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::MindTrickWalk { game, pitch, strikeout_type, batter_id, batter_name, base_instincts, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_description(&format!("{batter_name} strikes out {strikeout_type}."));
                eb.push_description(&format!("{batter_name} uses a Mind Trick!"));
                eb.push_description("The umpire sends them to first base.");
                if let Some(base) = base_instincts {
                    eb.push_description(&format!("Base Instincts take them directly to {base} base!"));
                }
                eb.push_scores(scores, home_team_id, "scores!");
                eb.push_player_tag(batter_id);
                eb.build(EventType::Walk)
            }
            FedEventData::MindTrickStrikeout { game, pitch, batter_id, batter_name, pitcher_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                // Before s18d94 the mind trick strikeout was classed as a walk and had the
                // batter_id in there twice.
                if (self.season, self.day) < (17, 93) {
                    eb.push_description(&format!("{batter_name} draws a walk."));
                    eb.push_player_tag(batter_id);
                }
                eb.push_description(&format!("{pitcher_name} uses a Mind Trick!"));
                eb.push_description(&format!("{batter_name} strikes out thinking."));
                eb.push_player_tag(batter_id); // batter twice, apparently
                eb.build(if (self.season, self.day) < (17, 93) { EventType::Walk } else { EventType::Strikeout })
            }
            FedEventData::BlooddrainBlocked { game, is_siphon, sipper_id, sipper_name, sippee_id, sippee_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                if is_siphon {
                    eb.push_description(&format!("{sipper_name}'s Siphon activates!"));
                }
                eb.push_description(&format!("{sipper_name} tried to siphon blood from {sippee_name}, but they were Sealed!"));
                eb.push_player_tag(sipper_id);
                eb.push_player_tag(sippee_id);
                eb.build(EventType::BlooddrainBlocked)
            }
            FedEventData::TarotReadingAddedOrRemovedItem { description, item_id, item_name, item_mods, player_item_rating_before, player_item_rating_after, player_rating, team_id, player_id, item_gained } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(description);
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("itemId", item_id);
                eb.push_metadata_str("itemName", item_name);
                eb.push_metadata_str_vec("mods", item_mods);
                eb.push_metadata_f64("playerItemRatingAfter", player_item_rating_after);
                eb.push_metadata_f64("playerItemRatingBefore", player_item_rating_before);
                eb.push_metadata_f64("playerRating", player_rating);
                eb.build(if item_gained { EventType::PlayerGainedItem } else { EventType::PlayerLostItem })
            }
            FedEventData::CommunityChestOpens { item_id, item_name, item_mods, player_item_rating_before, player_item_rating_after, player_rating, team_id, player_name, player_id } => {
                // Starting with the drop on season 18 day 59, the out-of-game community chest
                // messages (but not the in game ones!) change category from Special to Changes
                eb.set_category(if (self.season, self.day) < (17, 58) { EventCategory::Special } else { EventCategory::Changes });
                eb.push_description(&format!("The Community Chest Opens! {player_name} gained {item_name}."));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("itemId", item_id);
                eb.push_metadata_str("itemName", item_name);
                eb.push_metadata_str_vec("mods", item_mods);
                eb.push_metadata_f64_opt("playerItemRatingAfter", player_item_rating_after);
                eb.push_metadata_f64_opt("playerItemRatingBefore", player_item_rating_before);
                eb.push_metadata_f64("playerRating", player_rating);
                eb.build(EventType::PlayerGainedItem)
            }
            FedEventData::PlayerDropsItem { item_id, item_name, item_mods, player_item_rating_before, player_item_rating_after, player_rating, team_id, player_name, player_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{player_name} dropped {item_name}."));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("itemId", item_id);
                eb.push_metadata_str("itemName", item_name);
                eb.push_metadata_str_vec("mods", item_mods);
                eb.push_metadata_f64("playerItemRatingAfter", player_item_rating_after);
                eb.push_metadata_f64("playerItemRatingBefore", player_item_rating_before);
                eb.push_metadata_f64("playerRating", player_rating);
                eb.build(EventType::PlayerLostItem)
            }
            FedEventData::CommunityChestGameMessage { game, first_player_name, first_player_item_name, first_player_dropped_item, second_player_name, second_player_item_name, second_player_dropped_item } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Community Chest Opens!");
                if let Some(dropped_item) = first_player_dropped_item {
                    eb.push_description(&format!("{first_player_name} gained {first_player_item_name} and dropped {dropped_item}."));
                } else {
                    eb.push_description(&format!("{first_player_name} gained {first_player_item_name}."));
                }
                if let Some(dropped_item) = second_player_dropped_item {
                    eb.push_description(&format!("{second_player_name} gained {second_player_item_name} and dropped {dropped_item}."));
                } else {
                    eb.push_description(&format!("{second_player_name} gained {second_player_item_name}."));
                }
                eb.build(EventType::CommunityChestOpens)
            }
            FedEventData::Fax { game, team_id, team_nickname, exiting_pitcher_id, exiting_pitcher_name, entering_pitcher_id, entering_pitcher_name, shadows_location, rating_before, rating_after, player_swap_sub_event, enter_shadows_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("10 Runs collected.");
                eb.push_description("Incoming Shadow Fax...");
                eb.push_description(&format!("{exiting_pitcher_name} is replaced by {entering_pitcher_name}."));
                eb.push_player_tag(exiting_pitcher_id);
                eb.push_player_tag(entering_pitcher_id);
                eb.push_child(player_swap_sub_event, |mut child| {
                    // They changed the text in season 18
                    child.push_description(&if self.season < 17 {
                        format!("The {team_nickname} made a roster move.")
                    } else {
                        format!("{exiting_pitcher_name} was replaced by an incoming Fax.")
                    });
                    child.push_player_tag(exiting_pitcher_id);
                    child.push_player_tag(entering_pitcher_id);
                    child.push_team_tag(team_id);
                    child.push_metadata_i64("aLocation", PositionType::Rotation as i64);
                    child.push_metadata_uuid("aPlayerId", exiting_pitcher_id);
                    child.push_metadata_str("aPlayerName", &exiting_pitcher_name);
                    child.push_metadata_i64("bLocation", shadows_location as i64);
                    child.push_metadata_uuid("bPlayerId", entering_pitcher_id);
                    child.push_metadata_str("bPlayerName", &entering_pitcher_name);
                    child.push_metadata_uuid("teamId", team_id);
                    child.push_metadata_str("teamName", team_nickname);
                    child.build(EventType::PlayerSwap)
                });
                eb.push_child(enter_shadows_sub_event, |mut child| {
                    child.push_description(&format!("{exiting_pitcher_name} entered the Shadows."));
                    child.push_player_tag(exiting_pitcher_id);
                    child.push_team_tag(team_id);
                    child.push_metadata_f64("after", rating_after);
                    child.push_metadata_f64("before", rating_before);
                    child.push_metadata_i64("type", 4 /* "all" attribute category */);
                    child.build(EventType::PlayerStatIncrease)
                });
                eb.build(EventType::FaxMachine)
            }
            FedEventData::Redacted { description, scales } => {
                // EventBuilder intentionally doesn't support Redacted events because they violate
                // too many invariants (like "tags arrays exist")
                EventuallyEvent {
                    id: self.id,
                    created: self.created,
                    r#type: EventType::Undefined,
                    category: EventCategory::Redacted,
                    metadata: eventually_api::EventMetadata {
                        other: json!({
                            "redacted": true,
                            "scales": scales,
                        }),
                        ..Default::default()
                    },
                    blurb: "".to_string(),
                    description,
                    election_option_id: None,
                    player_tags: None,
                    game_tags: None,
                    team_tags: None,
                    sim: self.sim,
                    day: self.day,
                    season: self.season,
                    tournament: self.tournament,
                    phase: self.phase.into(),
                    nuts: self.nuts,
                }
            }
            FedEventData::Smithy { game, repair } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("Smithy beckons to {}.", repair.player_name));
                eb.push_player_tag(repair.player_id);
                // This one doesn't seem to do plurals
                eb.push_description(&format!("{} is repaired!", repair.item_name));
                eb.push_child(repair.sub_event, |mut child| {
                    child.push_description(&format!("{} {} was repaired by Smithy.", Possessive(&repair.player_name), repair.item_name));
                    child.build_item_repaired(repair)
                });
                eb.build(EventType::Smithy)
            }
            FedEventData::HolidayInning { game, inning_number } => {
                eb.set_game(game);
                eb.push_description("Hotel Motel");
                eb.push_description(&format!("Inning {inning_number} is a Holiday Inning!"));
                eb.build(EventType::HolidayInning)
            }
            FedEventData::HomeFieldAdvantage { game, team_nickname } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("The {team_nickname} apply Home Field advantage!"));
                eb.build(EventType::HomeFieldAdvantage)
            }
            FedEventData::PrizeMatch { game, item_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("Prize Match!\nThe Winner gets {item_name}"));
                eb.build(EventType::PrizeMatch)
            }
            FedEventData::WonPrizeMatch { team_nickname_or_player_name, team_id, player_id, item_id, item_name, item_mods, player_item_rating_before, player_item_rating_after, player_rating } => {
                eb.set_category(EventCategory::Changes);
                match team_nickname_or_player_name {
                    TeamNicknameOrPlayerName::TeamNickname(team_nickname) => {
                        eb.push_description(&format!("The {team_nickname} won the Prize Match!"));
                    }
                    TeamNicknameOrPlayerName::PlayerName(player_name) => {
                        eb.push_description(&format!("{player_name} gained the Prized {item_name}."));
                    }
                }
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("itemId", item_id);
                eb.push_metadata_str("itemName", item_name);
                eb.push_metadata_str_vec("mods", item_mods);
                eb.push_metadata_f64_opt("playerItemRatingAfter", player_item_rating_after);
                eb.push_metadata_f64("playerItemRatingBefore", player_item_rating_before);
                eb.push_metadata_f64("playerRating", player_rating);

                eb.build(EventType::PlayerGainedItem)
            }
            FedEventData::TeamReceivedGifts { recipient, top_3_benefactor_coins, top_3_benefactors, total_benefactor_coins, total_gifts } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_team_tag(recipient);
                eb.push_metadata_uuid("recipient", recipient);
                eb.push_metadata_json("top3BenefactorCoins", serde_json::to_value(top_3_benefactor_coins).unwrap());
                eb.push_metadata_json("top3Benefactors", serde_json::to_value(top_3_benefactors).unwrap());
                eb.push_metadata_i64("totalBenefactorCoins", total_benefactor_coins);
                eb.push_metadata_i64("totalGifts", total_gifts);
                eb.build(EventType::TeamReceivedGifts)
            }
            FedEventData::GiftReceived { team_id, title_and_recipient, metadata } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(&format!("Gift Received: {title_and_recipient}"));
                eb.push_team_tag(team_id);
                eb.set_full_metadata(metadata);
                eb.build(EventType::BlessingOrGiftWon)
            }
            FedEventData::ReplicaFadedToDust { team_id, team_nickname, player_id, player_name, mod_added_event } => {
                let mut dust_eb = eb.connected_event(mod_added_event);
                dust_eb.set_category(EventCategory::Changes);
                dust_eb.push_description(&format!("{player_name} faded to dust."));
                dust_eb.push_team_tag(team_id);
                dust_eb.push_player_tag(player_id);
                dust_eb.push_metadata_str("mod", "DUST");
                dust_eb.push_metadata_i64("type", ModDuration::Permanent);
                let dust_add_event = dust_eb.build(EventType::AddedMod);

                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("{player_name} faded away from the {team_nickname}."));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", player_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                let main_event = eb.build(EventType::PlayerRemovedFromTeam);

                return vec![main_event, dust_add_event];
            }
            FedEventData::ABloodType { game, team_id, team_nickname, blood_type_mod_id, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&format!("The {team_nickname} have A Blood Type."));
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(&format!("The {team_nickname} have A Blood Type."));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", blood_type_mod_id);
                    child_eb.push_metadata_str("source", "A");
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(EventType::AddedModFromOtherMod)
                });
                eb.build(EventType::ABloodType)
            }
            FedEventData::PolarityShift { game, numbers_go, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                let description = format!("The Polarity shifted!\nNumbers go {numbers_go}.");
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(&description);
                    match numbers_go {
                        NumbersGo::Up => {
                            child_eb.push_metadata_i32("before", Weather::PolarityMinus);
                            child_eb.push_metadata_i32("after", Weather::PolarityPlus);
                        }
                        NumbersGo::Down => {
                            child_eb.push_metadata_i32("before", Weather::PolarityPlus);
                            child_eb.push_metadata_i32("after", Weather::PolarityMinus);
                        }
                    }
                    child_eb.build(EventType::WeatherChange)
                });
                eb.build(EventType::PolarityShift)
            }
            FedEventData::DonatedShameApplied { game, team_nickname, unruns, score_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Shame Donations are granted!");
                eb.push_description(&format!("The {team_nickname} receive {unruns} Unruns."));

                if let Some(score) = &score_event {
                    eb.push_score_event(score);
                }

                eb.build(EventType::ShameDonor)
            }
        };

        vec![item]
    }

    #[deprecated = "This is part of the old event builder"]
    fn make_mod_change_sub_events<'a>(&self, mod_changes: &[ModChangeSubEventWithNamedPlayer], event_type: EventType, message: &str, mod_name: &str) -> (Vec<EventBuilderChildFull>, String) {
        let suffix = mod_changes.iter()
            .map(|e| format!("\n{} {message}", e.player_name))
            .join("");

        let children = mod_changes.iter()
            .map(|e| {
                EventBuilderChild::new(&e.sub_event)
                    .update(EventBuilderUpdate {
                        r#type: event_type,
                        category: EventCategory::Changes,
                        description: format!("{} {message}", e.player_name),
                        team_tags: vec![e.team_id],
                        player_tags: vec![e.player_id],
                        ..Default::default()
                    })
                    .metadata(json!({
                        "mod": mod_name,
                        "type": 0, // ?
                    }))
            })
            .collect();

        (children, suffix)
    }
}