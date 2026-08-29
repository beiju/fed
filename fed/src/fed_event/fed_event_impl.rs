use crate::fed_event::FedEventData;
use crate::fed_event::GameStartAnnouncement;
use crate::fed_event::HitType;
use crate::fed_event::HomeRunShameSource;
use crate::fed_event::PitcherNameId;
use crate::fed_event::{
    BatterSkippedReason, BlackHoleBurp, CoffeeBeanMod, ConsumerAttackEffect, EchoIntoStatic,
    EndZone, FloodingSweptEffect, NightShiftOutcome, PlayerMaybeCarcinized, PlayerReverb,
    PlayerStatChange, PositionType, PostseasonBirthBoostEventOrder, RenovationBuiltEffect,
    RenovationVotes, ReturnFromElsewhere, ReturnFromElsewhereFlavor, ReverbType, RoamFromLocation,
    RunStolenThroughTunnelsDetails, StatChangeCategory, TeamIncinerationReplacementSource,
    TeamNicknameOrPlayerName, TradeForNothing, TradeForSomething, TraderTraitor,
};
use crate::{PlayerMovedFrom, PlayersAddedToTeam};
use eventually_api::{EventCategory, EventType, EventuallyEvent, Weather};
use itertools::{Either, Itertools, Position};
use serde_json::json;
use std::iter;

use crate::format_utils::Possessive;
use crate::parse::event_builder::{possessive, EventBuilder};
use crate::*;

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
            _ => self
                .into_feed_events()
                .into_iter()
                .map(|event| event.description)
                .join("\n"),
        }
    }

    pub fn into_feed_events(self) -> Vec<EventuallyEvent> {
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
                eb.set_category(EventCategory::Narrative);
                eb.set_description(message);
                eb.push_metadata_i64("being", being);
                eb.build(EventType::BigDeal)
            }
            FedEventData::GameStart { game, weather, stadium_id, announcement } => {
                match announcement {
                    GameStartAnnouncement::LetsGo => { eb.push_description("Let's Go!"); }
                    GameStartAnnouncement::TeamNames { away, home } => {
                        eb.push_description(format!("{away} vs. {home}"));
                    }
                }
                eb.push_metadata_uuid("home", game.home_team);
                eb.push_metadata_uuid("away", game.away_team);
                eb.set_game(game);
                eb.push_metadata_i64("weather", weather);
                if let Some(id) = stadium_id {
                    eb.push_metadata_uuid("stadium", id);
                }

                eb.build(EventType::GameStart)
            }
            FedEventData::PlayBall { game } => {
                eb.set_game(game);
                eb.push_description("Play ball!");
                eb.build(EventType::PlayBall)
            }
            FedEventData::HalfInningStart { game, top_of_inning, inning, batting_team_name, team_subseasonal_mod_changes } => {
                eb.set_game(game);
                eb.push_team_subseasonal_mod_changes(team_subseasonal_mod_changes, self.season, self.day);
                eb.push_description(format!("{} of {inning}, {batting_team_name} batting.",
                                            if top_of_inning { "Top" } else { "Bottom" }));
                eb.build(EventType::HalfInning)
            }
            FedEventData::BatterUp { game, batter_name, team_nickname, wielding_item, inhabiting, is_repeating, is_skipping } => {
                eb.set_game(game);
                if inhabiting.is_some() || is_repeating {
                    eb.set_category(EventCategory::Special);
                }

                if is_repeating {
                    eb.push_description(format!("{batter_name} is Repeating!"));
                }

                if let Some(inhabiting) = inhabiting {
                    let inhabiting_description = format!("{batter_name} is Inhabiting {}!", inhabiting.inhabited_player_name);

                    if let Some(sub_event) = inhabiting.sub_event {
                        eb.push_child(sub_event, |mut child_eb| {
                            child_eb.push_description(&inhabiting_description);
                            child_eb.push_player_tag(inhabiting.inhabiting_player_id);
                            if let Some(team_id) = inhabiting.inhabiting_player_team_id {
                                child_eb.push_team_tag(team_id);
                            }
                            child_eb.push_metadata_str("mod", "INHABITING");
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                            child_eb.build(EventType::AddedMod)
                        });
                    }

                    eb.push_description(inhabiting_description);
                    eb.push_player_tag(inhabiting.inhabiting_player_id);
                    eb.push_player_tag(inhabiting.inhabited_player_id);
                }

                let item_suffix = if let Some(item_name) = wielding_item {
                    format!(", wielding {}", item_name)
                } else {
                    String::default()
                };

                let action = if is_skipping {
                    "skipped up to bat"
                } else {
                    "batting"
                };

                eb.push_description(format!("{batter_name} {action} for the {team_nickname}{item_suffix}."));

                eb.build(EventType::BatterUp)
            }
            FedEventData::SuperyummyGameStart { game, toggle } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                let description = format!("{} {} Peanuts.", toggle.player_name,
                                          if toggle.is_overperforming { "loves" } else { "misses" });
                eb.push_toggle_performing_child(toggle, &description, "SUPERYUMMY");
                eb.push_description(description);
                eb.build(EventType::Superyummy)
            }
            FedEventData::EchoedSuperyummyGameStart { game, player_name, peanuts_present } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{} {} Peanuts.", player_name,
                                            if peanuts_present { "loves" } else { "misses" }));
                eb.build(EventType::Superyummy)
            }
            FedEventData::Ball { game, pitch, balls, strikes, batter_item_damage } => {
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.push_description(format!("Ball. {}-{}", balls, strikes));
                // I think this conversion should be implicit but the compiler disagrees
                eb.push_named_item_damage(batter_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Ball)
            }
            FedEventData::StrikeSwinging { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                let is_double_strike = pitch.double_strike.is_some();
                if is_double_strike { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(format!("Strike{}, swinging. {balls}-{strikes}",
                                            if is_double_strike { "s" } else { "" }));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeLooking { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(format!("Strike, looking. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::StrikeFlinching { game, pitch, balls, strikes, pitcher_item_damage } => {
                eb.set_game(game);
                if pitch.double_strike.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(format!("Strike, flinching. {balls}-{strikes}"));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::Strike)
            }
            FedEventData::FoulBall { game, pitch, balls, strikes, batter_item_damage, birds, very_foul, offworld } => {
                eb.set_game(game);
                let offworld = if offworld { "Offworld " } else { "" };
                let very = if very_foul { "Very" } else { "" };
                let foul_ball_text = if pitch.double_strike.is_some() {
                    eb.set_category(EventCategory::Special);
                    "Foul Balls"
                } else {
                    "Foul Ball"
                };
                // Presumably a bug and presumably related to how they did string interpolation for
                // very foul balls (something like `${very_foul ? "Very" : ""} Foul Ball`)
                let extra_space = if self.season < 19 { "" } else { " " };

                eb.push_pitch(pitch);
                eb.push_description(format!("{offworld}{very}{extra_space}{foul_ball_text}. {balls}-{strikes}"));
                eb.push_birds(birds);
                eb.push_named_item_damage(batter_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::FoulBall)
            }
            FedEventData::Flyout { game, pitch, batter_name, fielder_name, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, fielder_item_damage, other_player_item_damage, parasite } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(format!("{batter_name} hit a flyout to {fielder_name}."));
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_opt_item_damage(fielder_item_damage.as_ref(), &fielder_name);
                eb.push_named_item_damage(other_player_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                eb.push_scores_without_event(&scores, home_team_id, "tags up and scores!", false, self.season < 21);
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_parasite(parasite);
                eb.push_score_summary(&scores);
                eb.build(EventType::FlyOut)
            }
            FedEventData::Hit { game, pitch, batter_name, batter_id, hit_type, scores, spicy_status, cooled_off, stopped_inhabiting, is_special, pitcher_item_damage, batter_item_damage, other_player_item_damage } => {
                let home_team_id = game.home_team; // Need this later
                eb.set_game(game);
                eb.push_pitch(pitch);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_description(format!("{batter_name} hits a {hit_type}!"));
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
                if self.season < 19 { eb.push_stopped_inhabiting(stopped_inhabiting.as_ref()); }
                // `scorers` is before spicy, but `score_event` is after
                eb.push_scorers(&scores.scores, home_team_id, "scores!", false, self.season < 21);

                eb.push_spicy(spicy_status, &batter_name, batter_id);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_named_item_damage(other_player_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                if self.season >= 19 { eb.push_stopped_inhabiting(stopped_inhabiting.as_ref()); }
                eb.push_free_refills(&scores.free_refills);
                eb.push_score_summary(&scores);

                eb.build(EventType::Hit)
            }
            FedEventData::HomeRun { game, pitch, magmatic, batter_name, batter_id, home_run_type, free_refills, spicy_status, stopped_inhabiting, is_special, big_bucket, attraction, damaged_items, hotel_motel_parties, shame, alley_oop, score_summary, balloons_inflated, balloons_popped } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                if is_special { eb.set_category(EventCategory::Special) }
                eb.push_pitch(pitch);

                // First, magmatic text...
                if let Some(_) = &magmatic {
                    eb.push_description(format!("{batter_name} is Magmatic!"));
                }

                // ...then hype...
                eb.push_home_run_shame_from_source(&shame, HomeRunShameSource::HomeRun, home_team_id);

                // ...then the magmatic event
                if let Some(mod_change) = &magmatic {
                    eb.push_child(mod_change.sub_event, |mut child| {
                        child.push_description(format!("{batter_name} hit a Magmatic home run!"));
                        child.push_player_tag(batter_id);
                        child.push_team_tag(mod_change.team_id);
                        child.push_metadata_str("mod", "MAGMATIC");
                        child.push_metadata_i64("type", ModDuration::Permanent);
                        child.build(EventType::RemovedMod)
                    });
                }

                eb.push_named_item_damages(damaged_items.iter().map(|(x, y)| (x.as_str(), y)));

                // HR itself
                eb.push_description(format!("{batter_name} hits a {home_run_type}!"));
                eb.push_player_tag(batter_id);

                if big_bucket {
                    eb.push_description("The ball lands in a Big Bucket. An extra Run scores!");
                    eb.push_home_run_shame_from_source(&shame, HomeRunShameSource::Buckets, home_team_id);
                }

                if let Some((ooper, success)) = alley_oop {
                    eb.push_description(format!("{ooper} went up for the alley oop..."));
                    eb.push_description(if success {
                        "...they slammed it down for an extra Run!"
                    } else {
                        "...but they can't connect."
                    });
                    eb.push_home_run_shame_from_source(&shame, HomeRunShameSource::Hoops, home_team_id);
                }

                if let Some(pop) = balloons_popped {
                    eb.push_description(format!("One of {} Balloons was struck and popped!", Possessive(&pop.stadium_name)));
                    eb.push_description(format!("{} Birds were scared away!", pop.birds_scared_away));
                }

                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_free_refills(&free_refills);
                eb.push_spicy(spicy_status, &batter_name, batter_id);
                // hotel motel is definitely after spicy
                eb.push_hotel_motel(&hotel_motel_parties);
                eb.push_attraction_with_player(attraction);
                // TODO: Store the ledgers for big buckets and alley oops in their respective
                //   Options and pass them in to push_opt_direct_score_summary with a new
                //   "additional scores" argument. This would be a large change, but it would make
                //   that data obey the single-source-of-truth principle.
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                let runs_scored = score_summary.as_ref().map_or(1, |s| s.runs_scored.round() as i64);
                eb.push_balloons(balloons_inflated.as_deref(), runs_scored);

                eb.build(EventType::HomeRun)
            }
            FedEventData::GroundOut { game, pitch, batter_name, fielder_name, fielder_shelled, scores, stopped_inhabiting, cooled_off, is_special, batter_debt, batter_item_damage, pitcher_item_damage_from_out, pitcher_item_damage_from_advance, fielder_item_damage_from_out, fielder_item_damage_from_advance, flood_balloon_popped } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || cooled_off.is_some() || is_special));
                eb.push_pitch(pitch);
                if fielder_shelled {
                    eb.push_description(format!("{batter_name} hit a ground out to {fielder_name}'s Shell."));
                } else {
                    eb.push_description(format!("{batter_name} hit a ground out to {fielder_name}."));
                }
                eb.push_batter_debt(batter_debt, &batter_name, &fielder_name);
                eb.push_opt_item_damage(fielder_item_damage_from_out.as_ref(), &fielder_name);
                eb.push_named_item_damage(pitcher_item_damage_from_out.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_scores_without_event(&scores, home_team_id, "advances on the sacrifice.", false, self.season < 21);
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_opt_item_damage(fielder_item_damage_from_advance.as_ref(), &fielder_name);
                eb.push_named_item_damage(pitcher_item_damage_from_advance.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_score_summary(&scores);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_flood_balloon_popped(flood_balloon_popped);
                eb.build(EventType::GroundOut)
            }
            FedEventData::StolenBase { game, runner_name, runner_id, base_stolen, blaserunning, free_refill, runner_item_damage, is_special, shame, score_summary, balloons, hotel_motel_party, took_the_fifth_base } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.push_player_tag(runner_id);
                eb.set_category(EventCategory::special_if(blaserunning || free_refill.is_some() || is_special));
                eb.push_description(format!("{runner_name} steals {base_stolen} base!"));
                eb.push_shame(&shame, home_team_id);

                if blaserunning {
                    eb.push_description(format!("{runner_name} scores with Blaserunning!"));
                    // The player tag appears a second time when there's blaserunning
                    eb.push_player_tag(runner_id);
                }

                if let Some(ttfb) = took_the_fifth_base {
                    eb.push_description(format!("{runner_name} took The Fifth Base!"));
                    // The player tag appears a second time when they take The Fifth Base
                    eb.push_player_tag(runner_id);

                    eb.push_child(ttfb.remove_mod_from_stadium_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{runner_name} took The Fifth Base from {}.", ttfb.stadium_name));
                        // It's always the stadium you're playing in, which is by definition the
                        // home team's stadium
                        child_eb.push_team_tag(home_team_id);
                        child_eb.push_metadata_str("mod", "EXTRA_BASE");
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.build(EventType::RemovedMod)
                    });

                    if let Some(dropped_item) = ttfb.dropped_item {
                        eb.push_dropped_item(&runner_name, runner_id, ttfb.team_id, ttfb.player_rating, dropped_item);
                    }

                    eb.push_child(ttfb.player_gained_item_sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Changes);
                        child_eb.push_description(format!("{runner_name} pocketed The Fifth Base."));
                        child_eb.push_player_tag(runner_id);
                        child_eb.push_team_tag(ttfb.team_id);
                        // As with the PlacedFifthBase, decided to hard-code The Fifth Base's data
                        child_eb.push_metadata_uuid("itemId", uuid::uuid!("eecc9bf3-96b5-4ea9-9a4a-05f0a0d586f0"));
                        child_eb.push_metadata_str("itemName", "The Fifth Base".to_string());
                        child_eb.push_metadata_str_vec("mods", vec!["SUPERWANDERER".to_string()]);
                        child_eb.push_metadata_f64("playerItemRatingAfter", ttfb.player_item_rating_after);
                        child_eb.push_metadata_f64("playerItemRatingBefore", ttfb.player_item_rating_before);
                        child_eb.push_metadata_f64("playerRating", ttfb.player_rating);
                        child_eb.build(EventType::PlayerGainedItem)
                    });
                }

                eb.push_free_refill(free_refill);
                eb.push_opt_item_damage(runner_item_damage.as_ref(), &runner_name);
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.push_balloons_from_score_summary(score_summary.as_ref(), balloons.as_deref());

                if let Some(party) = hotel_motel_party {
                    eb.push_hotel_motel_party(&party, &runner_name, runner_id);
                }

                eb.build(EventType::StolenBase)
            }
            FedEventData::StrikeoutSwinging { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite, score_summary } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(format!("{} strikes out swinging.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.build(EventType::Strikeout)
            }
            FedEventData::StrikeoutLooking { game, pitch, batter_name, stopped_inhabiting, pitcher_item_damage, free_refill, is_special, parasite, score_summary, balloons } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(is_special));
                eb.push_pitch(pitch);
                eb.push_description(format!("{} strikes out looking.", batter_name));
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_free_refill(free_refill);
                eb.push_parasite(parasite);
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.push_balloons_from_score_summary(score_summary.as_ref(), balloons.as_deref());
                eb.build(EventType::Strikeout)
            }
            FedEventData::Walk { game, pitch, batter_name, batter_id, scores, base_instincts, batter_item_damage, pitcher_item_damage, stopped_inhabiting, is_special } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(scores.used_refill() || base_instincts.is_some() || is_special));
                eb.push_pitch(pitch);
                eb.push_description(format!("{batter_name} draws a walk."));
                if let Some(base) = base_instincts {
                    eb.push_description(format!("Base Instincts take them directly to {base} base!"));
                }
                eb.push_player_tag(batter_id);
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                // Seems like Walks continue having hype before score even after s21 when the other events stop
                eb.push_scores(&scores, home_team_id, "scores!", false, true);
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.build(EventType::Walk)
            }
            FedEventData::CaughtStealing { game, runner_name, base_stolen, runner_item_damage, fielder_item_damage } => {
                eb.set_game(game);
                eb.push_description(format!("{runner_name} gets caught stealing {base_stolen} base."));
                eb.push_opt_item_damage(runner_item_damage.as_ref(), &runner_name);
                eb.push_named_item_damage(fielder_item_damage.as_ref().map(|(x, y)| (x.as_str(), y)));
                eb.build(EventType::StolenBase)
            }
            FedEventData::InningEnd { game, inning_num, lost_triple_threat } => {
                eb.set_game(game);
                eb.push_description(format!("Inning {inning_num} is now an Outing."));
                eb.push_mod_change_child(&lost_triple_threat, EventType::RemovedMod, "is no longer a Triple Threat.", "TRIPLE_THREAT");
                eb.build(EventType::InningEnd)
            }
            FedEventData::CharmStrikeout { game, charmer_id, charmer_name, charmed_id, charmed_name, stopped_inhabiting, num_swings } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{charmer_name} charmed {charmed_name}!"));
                eb.push_description(format!("{charmed_name} swings {num_swings} times to strike out willingly!"));
                // I do not know why the charmer appears twice, but that seems to be accurate
                eb.push_player_tag(charmer_id);
                eb.push_player_tag(charmer_id);
                eb.push_player_tag(charmed_id);
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.build(EventType::Strikeout)
            }
            FedEventData::FieldersChoice { game, pitch, batter_name, runner_out_name, out_at_base, scores, stopped_inhabiting, cooled_off, is_special, damaged_items } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                if is_special { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                eb.push_description(format!("{runner_out_name} out at {out_at_base} base."));
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_scorers(&scores.scores, home_team_id, "scores!", true, self.season < 21);
                eb.push_named_item_damages(damaged_items.iter().map(|(x, y)| (x.as_str(), y)));
                eb.push_description(format!("{batter_name} reaches on fielder's choice."));
                // This includes attractions and hotel motel parties, which are
                // part of push_scorers for every other event
                eb.push_scorers_trailing_matter(&scores.scores);
                eb.push_free_refills(&scores.free_refills);
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_score_summary(&scores);
                eb.build(EventType::GroundOut)
            }
            FedEventData::StrikeZapped { game } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Electricity zaps a strike away!");
                eb.build(EventType::StrikeZapped)
            }
            FedEventData::PeanutFlavorText { game, message } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(message);
                eb.build(EventType::PeanutFlavorText)
            }
            FedEventData::DoublePlay { game, pitch, batter_name, scores, stopped_inhabiting, cooled_off, pitcher_item_damage, flood_balloon_popped } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                if flood_balloon_popped.is_some() { eb.set_category(EventCategory::Special); }
                eb.push_pitch(pitch);
                // I feel like there should be an easier way to do this conversion
                eb.push_named_item_damage(pitcher_item_damage.as_ref().map(|(n, d)| (n.as_str(), d)));
                eb.push_description(format!("{batter_name} hit into a double play!"));
                eb.push_scores_without_event(&scores, home_team_id, "scores!", false, self.season < 21);
                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());
                eb.push_cooled_off(cooled_off, &batter_name);
                eb.push_flood_balloon_popped(flood_balloon_popped);
                eb.push_score_summary(&scores);
                eb.build(EventType::GroundOut)
            }
            FedEventData::GameEnd { game, winner_id, winning_team_name, winning_team_score, losing_team_name, losing_team_score, temp_stolen_player_returned } => {
                let home_team_id = game.home_team;
                let away_team_id = game.away_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("{winning_team_name} {winning_team_score}, {losing_team_name} {losing_team_score}"));
                eb.push_metadata_uuid("winner", winner_id);
                // It pushes them a second time. This has to be after set_game, as that overrides them
                eb.push_team_tag(home_team_id);
                eb.push_team_tag(away_team_id);

                if let Some(ret) = temp_stolen_player_returned {
                    eb.push_temp_stolen_player_returned(&ret);
                }

                eb.build(EventType::GameEnd)
            }
            FedEventData::MildPitch { game, pitcher_id, pitcher_name, balls, strikes, runners_advance, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{pitcher_name} throws a Mild pitch!"));
                eb.push_description(format!("Ball, {balls}-{strikes}."));
                if runners_advance {
                    eb.push_description("Runners advance on the pathetic play!");
                }
                eb.push_player_tag(pitcher_id);
                eb.push_scores(&scores, home_team_id, "scores!", false, self.season < 21);
                eb.build(EventType::MildPitch)
            }
            FedEventData::CoffeeBean { game, player_id, player_name, roast, notes, which_mod, gained_mod, sub_event, team_id, previous } => {
                let change_str = match (gained_mod, which_mod) {
                    (true, CoffeeBeanMod::Wired) => { "is Wired!" }
                    (true, CoffeeBeanMod::Tired) => { "is Tired." }
                    (false, CoffeeBeanMod::Wired) => { "is no longer Wired." }
                    (false, CoffeeBeanMod::Tired) => { "is no longer Tired!" }
                };

                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} is Beaned by a {roast} roast with {notes}."));
                let change_description = format!("{player_name} {change_str}");
                eb.push_description(&change_description);
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(change_description);
                    child_eb.push_player_tag(player_id);
                    if let Some(team_id) = team_id {
                        child_eb.push_team_tag(team_id);
                    }

                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    if let Some(prev_mod) = previous {
                        child_eb.push_metadata_str("from", prev_mod.to_str());
                        child_eb.push_metadata_str("to", which_mod.to_str());
                    } else {
                        child_eb.push_metadata_str("mod", which_mod.to_str());
                    }

                    child_eb.build(if previous.is_some() {
                        EventType::ModChange
                    } else if gained_mod {
                        EventType::AddedMod
                    } else {
                        EventType::RemovedMod
                    })
                });

                eb.build(EventType::CoffeeBean)
            }
            FedEventData::BecameMagmatic { game, player_id, player_name, is_unstable, magmatic_mod_added } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if is_unstable {
                    eb.push_description(format!("{player_name} is Unstable!"));
                }
                eb.push_description(format!("Rogue Umpire tried to incinerate {player_name}, but {player_name} ate the flame! They became Magmatic!"));
                eb.push_player_tag(player_id);
                if let Some(mod_added) = magmatic_mod_added {
                    eb.push_child(mod_added.sub_event, |mut child| {
                        child.set_description(format!("{player_name} ate some flame."));
                        child.push_player_tag(player_id);
                        child.push_team_tag(mod_added.team_id);
                        child.push_metadata_str("mod", "MAGMATIC");
                        child.push_metadata_i64("type", ModDuration::Permanent);
                        child.build(EventType::AddedMod)
                    })
                }
                eb.build(EventType::IncinerationBlocked)
            }
            FedEventData::SpecialBlooddrain { game, sipper_id, sipper_name, sipped_id, sipped_team_id, sipped_name, sipped_category, action, sipped_event, rating_before, rating_after, maintenance_mode } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                eb.push_description(format!("{sipper_name}'s Siphon activates!"));
                eb.push_description(format!("{sipper_name} siphoned some of {sipped_name}'s {sipped_category} ability!"));
                eb.push_description(format!("{sipper_name} {action}"));
                eb.push_player_tag(sipper_id);
                eb.push_player_tag(sipped_id);


                eb.push_child(sipped_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{sipped_name} had blood drained by {sipper_name}."));
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
                            child_eb.push_description(mod_removal.format_description_player(&player_name));
                            child_eb.push_player_tag(player_id);
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_json("removes", mod_removal.mods_removed.into());
                            child_eb.push_metadata_str("source", r.mod_id);
                            child_eb.build(EventType::RemovedModsFromAnotherMod)
                        })
                    )
                    .collect();
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{} {mod_duration} mods wore off.", possessive(player_name)));
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
                            child_eb.push_description(mod_removal.format_description_team(&team_nickname));
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_json("removes", mod_removal.mods_removed.into());
                            child_eb.push_metadata_str("source", r.mod_id);
                            child_eb.build(EventType::RemovedModsFromAnotherMod)
                        })
                    )
                    .collect();
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {} {mod_duration} mods wore off.", possessive(team_nickname)));
                eb.push_team_tag(team_id);
                eb.push_metadata_str_vec("mods", mod_ids);
                eb.push_metadata_i64("type", mod_duration);
                events.push(eb.build(EventType::ModExpires));

                return events;
            }
            FedEventData::BirdsCircle { game } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Birds circle ... but they don't find what they're looking for.");
                eb.build(EventType::BirdsCircle)
            }
            FedEventData::AmbushedByCrows { game, batter_id, batter_name, friend_of_crows } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if let Some(PitcherNameId { pitcher_name, pitcher_id }) = friend_of_crows {
                    eb.push_description(format!("{pitcher_name} calls upon their Friends!"));
                    eb.push_player_tag(pitcher_id);
                }
                eb.push_description(format!("A murder of Crows ambush {batter_name}!"));
                eb.push_description("They run to safety, resulting in an out.");
                eb.push_player_tag(batter_id);

                eb.build(EventType::AmbushedByCrows)
            }
            FedEventData::Sun2SetWin { team_id, team_nickname } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("Sun 2 set a Win upon the {team_nickname}."));
                eb.push_team_tag(team_id);
                eb.build(EventType::Sun2SetWin)
            }
            FedEventData::BlackHoleSwallowedWin { team_id, team_nickname } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("The Black Hole swallowed a Win from the {team_nickname}!"));
                eb.push_team_tag(team_id);
                eb.build(EventType::BlackHoleSwallowedWin)
            }
            FedEventData::Sun2 { game, scoring_team_nickname, caught_some_rays, win_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                // The presence of win_event is not causally connected to the burp message, but I'm
                // using it as a signal for now. iirc this will have to be changed later
                if let Some(win_event) = win_event {
                    eb.push_description(format!("The {scoring_team_nickname} collected 10!"));
                    eb.push_description(format!("Sun 2 smiled at the {scoring_team_nickname}."));
                    // Two of them
                    eb.push_description(format!("Sun 2 smiled at the {scoring_team_nickname}."));
                    eb.push_balloons(win_event.balloons.as_deref(), 10);
                    eb.push_child(win_event.sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Outcomes);
                        child_eb.push_description(format!("Sun 2 set a Win upon the {scoring_team_nickname}."));
                        child_eb.push_team_tag(win_event.team_id);
                        child_eb.push_metadata_i64("amount", 1);
                        child_eb.push_metadata_i64("before", win_event.wins_after - 1);
                        child_eb.push_metadata_i64("after", win_event.wins_after);
                        child_eb.push_metadata_str_vec("lines", if self.season < 23 {
                            vec![
                                "Sun 2: 1".to_string(),
                                "Sun(Sun): 1 ^ 2 = 1".to_string(),
                            ]
                        } else {
                            Vec::new()
                        });
                        child_eb.build(if self.day < 99 { EventType::WinCollectedRegular } else { EventType::WinCollectedPostseason })
                    });
                } else {
                    eb.push_description(format!("The {scoring_team_nickname} collect 10! Sun 2 smiles."));
                    eb.push_description(format!("Sun 2 set a Win upon the {scoring_team_nickname}."));
                }

                if let Some(rays) = caught_some_rays {
                    eb.push_description(format!("{} catches some rays.", rays.player_name));
                    eb.push_player_tag(rays.player_id);
                    eb.push_child(rays.sub_event, |mut child| {
                        child.push_description(format!("{} caught some rays.", rays.player_name));
                        child.push_player_tag(rays.player_id);
                        child.push_team_tag(rays.team_id);
                        child.push_metadata_f64("before", rays.rating_before);
                        child.push_metadata_f64("after", rays.rating_after);
                        child.push_metadata_i64("type", 4);
                        child.build(EventType::PlayerStatIncrease)
                    })
                }

                eb.build(EventType::Sun2)
            }
            FedEventData::BlackHole { game, scoring_team_nickname, victim_team_nickname, carcinization, compressed_by_gamma, burp } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {scoring_team_nickname} collect 10!"));

                match burp {
                    BlackHoleBurp::None => {
                        eb.push_description(format!("The Black Hole swallows the Runs and a {victim_team_nickname} Win."));
                    },
                    BlackHoleBurp::Win(win_event) => {
                        eb.push_description(format!("The Black Hole swallowed the Runs and burped at the {victim_team_nickname}."));
                        eb.push_balloons(win_event.balloons.as_deref(), 10);
                        eb.push_child(win_event.sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Outcomes);
                            child_eb.push_description(format!("The Black Hole burped a Win at the {victim_team_nickname}."));
                            child_eb.push_team_tag(win_event.team_id);
                            child_eb.push_metadata_i64("amount", 1);
                            child_eb.push_metadata_i64("before", win_event.wins_after - 1);
                            child_eb.push_metadata_i64("after", win_event.wins_after);
                            child_eb.push_metadata_str_vec("lines", vec![
                                "Black Hole: -1".to_string(),
                                "Sun(Sun): -1 ^ 2 = 1".to_string(),
                            ]);
                            child_eb.build(if self.day < 99 { EventType::WinCollectedRegular } else { EventType::WinCollectedPostseason })
                        })
                    },
                    BlackHoleBurp::Unwin(win_event) => {
                        eb.push_description(format!("The Black Hole swallowed the Runs and burped at the {victim_team_nickname}."));
                        eb.push_balloons(win_event.balloons.as_deref(), 10);
                        eb.push_child(win_event.sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Outcomes);
                            child_eb.push_description(format!("The Black Hole burped an Unwin at the {victim_team_nickname}."));
                            child_eb.push_team_tag(win_event.team_id);
                            child_eb.push_metadata_i64("amount", -1);
                            child_eb.push_metadata_i64("before", win_event.wins_after + 1);
                            child_eb.push_metadata_i64("after", win_event.wins_after);
                            child_eb.push_metadata_str_vec("lines", Vec::new());
                            child_eb.build(if self.day < 99 { EventType::WinCollectedRegular } else { EventType::WinCollectedPostseason })
                        })
                    },
                }

                if let Some(carc_full) = carcinization {
                    match carc_full.player_moved {
                        PlayerMaybeCarcinized::Successful { move_event: carc, mod_added_sub_event } => {
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
                            eb.push_child(mod_added_sub_event, |mut child| {
                                child.push_description(&mod_add_description);
                                child.push_player_tag(carc.player_id);
                                child.push_team_tag(carc.new_team_id);
                                child.push_metadata_str("mod", "TEMP_STOLEN");
                                child.push_metadata_i64("type", ModDuration::Game);
                                child.build(EventType::AddedMod)
                            });
                        }
                        PlayerMaybeCarcinized::FailedByForce(force) => {
                            let description = format!("The {} steal {} for the remainder of the game.",
                                                      carc_full.new_team_name, force.player_name);
                            eb.push_description(&description);
                            eb.push_description("Steal failed.");
                            eb.push_description(format!("{} was gripped by Force.", force.player_name));
                            eb.push_child(force.sub_event, |mut child_eb| {
                                child_eb.push_description(&description);
                                child_eb.push_player_tag(force.player_id);
                                child_eb.build(EventType::PlayerMoveFailedForce)
                            });
                        }
                    }
                }

                if let Some(gamma) = compressed_by_gamma {
                    eb.push_description("The Black Hole burps!");
                    eb.push_description(format!("{} is compressed by gamma!", gamma.player_name));
                    eb.push_player_tag(gamma.player_id);
                    eb.push_child(gamma.sub_event, |mut child| {
                        child.push_description(format!("{} was compressed by gamma!", gamma.player_name));
                        child.push_player_tag(gamma.player_id);
                        child.push_team_tag(gamma.team_id);
                        child.push_metadata_f64("before", gamma.rating_before);
                        child.push_metadata_f64("after", gamma.rating_after);
                        child.push_metadata_i64("type", StatChangeCategory::All);
                        child.build(EventType::PlayerStatDecrease)
                    })
                }

                eb.build(EventType::BlackHole)
            }
            FedEventData::TeamDidShame { shaming_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("The {shaming_team_nickname} shamed the {shamed_team_nickname}."));
                eb.push_team_tag(shaming_team_id);
                eb.push_metadata_i64("totalShames", total_shames);
                eb.push_metadata_i64("totalShamings", total_shamings);
                eb.build(EventType::TeamDidShame)
            }
            FedEventData::TeamWasShamed { shamed_team_id, shaming_team_nickname, shamed_team_nickname, total_shames, total_shamings } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("The {shamed_team_nickname} were shamed by the {shaming_team_nickname}."));
                eb.push_team_tag(shamed_team_id);
                eb.push_metadata_i64("totalShames", total_shames);
                eb.push_metadata_i64("totalShamings", total_shamings);
                eb.build(EventType::TeamWasShamed)
            }
            FedEventData::CharmWalk { game, pitch, batter_name, batter_id, pitcher_name, batter_item_damage, pitcher_item_damage, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_opt_item_damage(pitcher_item_damage.as_ref(), &pitcher_name);
                eb.push_opt_item_damage(batter_item_damage.as_ref(), &batter_name);
                eb.push_description(format!("{batter_name} charms {pitcher_name}!"));
                eb.push_description(format!("{batter_name} walks to first base."));
                eb.push_player_tag(batter_id);
                eb.push_player_tag(batter_id); // two of them
                eb.push_scores(&scores, home_team_id, "scores!", false, self.season < 21);
                eb.build(EventType::Walk)
            }
            FedEventData::GainFreeRefill { game, player_id, player_name, roast, ingredients, sub_event, team_id } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                let child_description = format!("{player_name} got a Free Refill.");
                let [ingredient1, ingredient2] = ingredients;
                eb.push_description(format!("{player_name} is Poured Over with a {roast} roast blending {ingredient1} and {ingredient2}!"));
                eb.push_description(&child_description);
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(child_description);
                    child_eb.push_player_tag(player_id);
                    if let Some(team_id) = team_id {
                        child_eb.push_team_tag(team_id);
                    }
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.push_metadata_str("mod", "COFFEE_RALLY");
                    child_eb.build(EventType::AddedMod)
                });

                eb.build(EventType::GainFreeRefill)
            }
            FedEventData::AllergicReaction { game, team_id, player_id, player_name, sub_event, rating_before, rating_after, weather_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} swallowed a stray peanut and had an allergic reaction!"));
                eb.push_player_tag(player_id);

                if let Some(weather) = weather_event {
                    eb.push_child(weather, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description(format!("{player_name} swallowed a stray peanut."));
                        child_eb.push_team_tag(team_id);
                        child_eb.push_player_tag(player_id);
                        child_eb.push_metadata_str("effect", "Allergic Reaction");
                        child_eb.push_metadata_i64("weather", Weather::Peanuts);
                        child_eb.build(EventType::WeatherEvent)
                    });
                }

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{player_name} had an allergic reaction."));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.build(EventType::PlayerStatDecrease)
                });

                eb.build(EventType::AllergicReaction)
            }
            FedEventData::SuperallergicReaction { game, team_id, player_id, player_name, sub_event, rating_before, rating_after } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} swallowed a stray peanut and had a Superallergic reaction!"));
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{player_name} had a Superallergic reaction."));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.build(EventType::PlayerStatDecreaseFromSuperallergic)
                });

                eb.build(EventType::SuperallergicReaction)
            }
            FedEventData::MildPitchWalk { game, pitcher_id, pitcher_name, batter_id, batter_name, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{pitcher_name} throws a Mild pitch!\n{batter_name} draws a walk."));
                eb.push_player_tag(pitcher_id);
                eb.push_player_tag(batter_id);
                eb.push_scores(&scores, home_team_id, "scores!", false, true);
                eb.build(EventType::MildPitch)
            }
            FedEventData::PerkUp { game, players } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(
                    players.iter()
                        .map(|player| format!("{} Perks up.", player.player_name))
                        .join("\n")
                );
                for player in players {
                    eb.push_child(player.sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} Perks up.", player.player_name));
                        child_eb.push_player_tag(player.player_id);
                        child_eb.push_team_tag(player.team_id);
                        child_eb.push_metadata_str("mod", "OVERPERFORMING");
                        child_eb.push_metadata_str("source", "PERK");
                        child_eb.push_metadata_i64("type", ModDuration::Game);
                        child_eb.build(EventType::AddedModFromOtherMod)
                    });
                }
                eb.build(EventType::Perk)
            }
            FedEventData::Blooddrain { game, is_siphon, sipper, maintenance_mode, sipped, sipped_category } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                if is_siphon { eb.push_description(format!("{}'s Siphon activates!", sipper.player_name)); }
                eb.push_description(format!("{} siphoned some of {}'s {sipped_category} ability!", sipper.player_name, sipped.player_name));
                eb.push_description(format!("{} increased their {sipped_category} ability!", sipper.player_name));

                // Can't put this in build_child because the player tags are in the opposite order
                // from the child events, for some reason
                eb.push_player_tag(sipper.player_id);
                eb.push_player_tag(sipped.player_id);

                let build_child = |eb: &mut EventBuilder, change: &PlayerStatChange, description: String| {
                    eb.push_child(change.sub_event, |mut child| {
                        child.push_description(&description);
                        child.push_player_tag(change.player_id);
                        child.push_team_tag(change.team_id);
                        child.build_player_stat_changed(change.rating_before, change.rating_after, sipped_category)
                    });
                };

                build_child(&mut eb, &sipped,
                            format!("{} had blood drained by {}.", sipped.player_name, sipper.player_name));
                eb.push_maintenance_mode(maintenance_mode);
                build_child(&mut eb, &sipper,
                            format!("{} drained blood from {}.", sipper.player_name, sipped.player_name));

                eb.build(if is_siphon { EventType::BlooddrainSiphon } else { EventType::Blooddrain })
            }
            FedEventData::Feedback { game, players: [player_a, player_b], lcd_soundsystem, position_type, sub_event, weather_event } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Reality flickers. Things look different ...");
                eb.push_description(format!("{} and {} switch teams in the feedback!", player_a.player_name, player_b.player_name));
                eb.push_player_tag(player_a.player_id);
                eb.push_player_tag(player_b.player_id);

                if let Some((lcd_a, lcd_b)) = lcd_soundsystem {
                    let team_nickname = if player_a.team_id == home_team_id {
                        &player_a.team_nickname
                    } else {
                        &player_b.team_nickname
                    };

                    eb.push_description(format!("The LCD Soundsystem is playing at the {team_nickname}' house!"));
                    for (lcd, player) in [(lcd_a, &player_a), (lcd_b, &player_b)] {
                        eb.push_child(lcd.sub_event, |mut child| {
                            child.push_description(format!("The LCD Soundsystem boosted {}!", player.player_name));
                            child.push_player_tag(player.player_id);
                            child.push_team_tag(player.team_id);
                            child.build_boost(&lcd)
                        });
                    }
                }

                if let Some(weather) = weather_event {
                    eb.push_child(weather, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description("Reality flickered in the Feedback...");
                        child_eb.push_player_tag(player_a.player_id);
                        child_eb.push_player_tag(player_b.player_id);
                        child_eb.push_team_tag(player_a.team_id);
                        child_eb.push_team_tag(player_b.team_id);
                        child_eb.push_metadata_str("effect", "Feedback Swap");
                        child_eb.push_metadata_i64("weather", Weather::Feedback);
                        child_eb.build(EventType::WeatherEvent)
                    });
                }

                eb.push_description(format!("{} is now {}.", player_b.player_name, position_type.role()));
                eb.push_child(sub_event, |mut child| {
                    if self.season < 19 {
                        child.push_description("Reality flickered in the Feedback.");
                    } else {
                        child.push_description(format!("{} and {} were swapped in Feedback.", player_a.player_name, player_b.player_name));
                    }
                    child.push_player_tag(player_a.player_id);
                    child.push_player_tag(player_b.player_id);
                    child.push_team_tag(player_a.team_id);
                    child.push_team_tag(player_b.team_id);
                    child.push_metadata_i64("aLocation", player_a.location);
                    child.push_metadata_uuid("aPlayerId", player_a.player_id);
                    child.push_metadata_str("aPlayerName", player_a.player_name);
                    child.push_metadata_uuid("aTeamId", player_a.team_id);
                    child.push_metadata_str("aTeamName", player_a.team_nickname);
                    child.push_metadata_i64("bLocation", player_b.location);
                    child.push_metadata_uuid("bPlayerId", player_b.player_id);
                    child.push_metadata_str("bPlayerName", player_b.player_name);
                    child.push_metadata_uuid("bTeamId", player_b.team_id);
                    child.push_metadata_str("bTeamName", player_b.team_nickname);
                    child.build(EventType::PlayerTraded)
                });

                eb.build(EventType::FeedbackSwap)
            }
            FedEventData::BestowReverberating { game, team_id, player_id, player_name, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Reverberations are at dangerous levels!");
                eb.push_description(format!("{player_name} is now Reverberating wildly!"));
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{player_name} is now Reverberating wildly!"));
                    child_eb.push_player_tag(player_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", "REVERBERATING");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::AddedMod)
                });

                eb.build(EventType::ReverbBestowsReverberating)
            }
            FedEventData::Reverb { game, team_id, team_nickname, reverb_type, gravity_players, weather_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                if let Some(weather) = weather_event {
                    eb.push_child(weather, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description(match &reverb_type {
                            ReverbType::Rotation(_) => { "Reverberations hit unsafe levels!" }
                            ReverbType::Lineup(_) => { "Reverberations hit unsafe levels!" }
                            ReverbType::Full(_) => { "Reverberations hit dangerous levels!" }
                            ReverbType::SeveralPlayers(_) => { "Reverberations hit high levels!" }
                        });
                        child_eb.push_team_tag(team_id);
                        child_eb.push_metadata_str("effect", match &reverb_type {
                            ReverbType::Rotation(_) => { "Rotation Shuffle" }
                            ReverbType::Lineup(_) => { "Lineup Shuffle" }
                            ReverbType::Full(_) => { "Roster Shuffle" }
                            ReverbType::SeveralPlayers(_) => { "Player Shuffle" }
                        });
                        child_eb.push_metadata_i64("weather", Weather::Reverb);
                        child_eb.build(EventType::WeatherEvent)
                    });
                }

                match reverb_type {
                    ReverbType::Lineup(sub_event) => {
                        eb.push_description(if self.season < 19 {
                            "Reverberations are at unsafe levels!"
                        } else {
                            "Reverberations hit unsafe levels!"
                        });
                        eb.push_description(format!("The {team_nickname} had their lineup shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(format!("The {team_nickname} had their lineup shuffled."));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbLineupShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::Rotation(sub_event) => {
                        eb.push_description(if self.season < 19 {
                            "Reverberations are at unsafe levels!"
                        } else {
                            "Reverberations hit unsafe levels!"
                        });
                        eb.push_description(format!("The {team_nickname} had their rotation shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(format!("The {team_nickname} had their rotation shuffled in the Reverb!"));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbRotationShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::Full(sub_event) => {
                        eb.push_description(if self.season < 19 {
                            "Reverberations are at dangerous levels!"
                        } else {
                            "Reverberations hit dangerous levels!"
                        });
                        eb.push_description(format!("The {team_nickname} were shuffled in the Reverb!"));
                        eb.push_child(sub_event, |mut child| {
                            child.push_description(format!("The {team_nickname} were shuffled in the Reverb!"));
                            child.push_team_tag(team_id);
                            child.build(EventType::ReverbFullShuffle)
                        });
                        eb.push_gravity(gravity_players);
                        eb.build(EventType::ReverbRosterShuffle)
                    }
                    ReverbType::SeveralPlayers(player_reverbs) => {
                        eb.push_description(if self.season < 19 {
                            "Reverberations are at high levels!"
                        } else {
                            "Reverberations hit high levels!"
                        });
                        eb.push_description(format!("The {team_nickname} had several players shuffled in the Reverb!"));
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
                                        child.push_metadata_i64("aLocation", first_player_new_location);
                                        child.push_metadata_uuid("aPlayerId", first_player_id);
                                        child.push_metadata_str("aPlayerName", first_player_name);
                                        child.push_metadata_i64("bLocation", second_player_new_location);
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
                eb.set_category(EventCategory::Changes);
                eb.push_description(description);
                eb.set_player_tags(player_tags);
                eb.set_team_tags(team_tags);
                eb.set_full_metadata(metadata);
                eb.build(EventType::TarotReading)
            }
            FedEventData::TarotReadingAddedOrRemovedMod { team_id, player_id, description, r#mod, mod_duration, mod_removed, mods_removed_from_other_mod } => {
                let mut events = Vec::new();

                if let Some(from_other_mod) = mods_removed_from_other_mod {
                    let mut other_eb = eb.connected_event(from_other_mod.event);
                    other_eb.set_category(EventCategory::Changes);
                    other_eb.push_team_tag(team_id);
                    if let Some(pid) = player_id { other_eb.push_player_tag(pid); }
                    other_eb.push_description(from_other_mod.format_description());
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
            FedEventData::BecomeTripleThreat { game, pitchers } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.set_description(if let Some((pitcher_1, pitcher_2)) = pitchers.iter().collect_tuple() {
                    format!("{} and {} chug a Third Wave of Coffee!\nThey are now Triple Threats!", pitcher_1.player_name, pitcher_2.player_name)
                } else if let Some((pitcher, )) = pitchers.iter().collect_tuple() {
                    format!("{} chugs a Third Wave of Coffee!\nThey are now a Triple Threat!", pitcher.player_name)
                } else {
                    panic!("There should either be one or two pitchers here")
                });
                eb.set_player_tags(pitchers.iter().map(|pitcher| pitcher.player_id).collect());
                for pitcher in pitchers {
                    eb.push_child(pitcher.sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Changes);
                        child_eb.push_description(format!("{} is a Triple Threat.", pitcher.player_name));
                        child_eb.push_team_tag(pitcher.team_id);
                        child_eb.push_player_tag(pitcher.player_id);
                        child_eb.push_metadata_str("mod", "TRIPLE_THREAT");
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.build(EventType::AddedMod)
                    });
                }
                eb.build(EventType::BecomeTripleThreat)
            }
            FedEventData::UnderOver { game, team_id, player_id, player_name, on, sub_event } => {
                let description = format!("{player_name}, Under Over, {}.", if on { "On" } else { "Off" });
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&description);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_str("mod", "OVERPERFORMING");
                    child_eb.push_metadata_str("source", "UNDEROVER");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod })
                });
                eb.build(EventType::UnderOver)
            }
            FedEventData::OverUnder { game, team_id, player_id, player_name, on, sub_event } => {
                let description = format!("{player_name}, Over Under, {}.", if on { "On" } else { "Off" });
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&description);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_str("mod", "UNDERPERFORMING");
                    child_eb.push_metadata_str("source", "OVERUNDER");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(if on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod })
                });
                eb.build(EventType::OverUnder)
            }
            FedEventData::TasteTheInfinite { game, sheller_id, sheller_name, shellee_team_id, shellee_id, shellee_name, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{sheller_name} tastes the infinite!\n{shellee_name} is Shelled!"));
                eb.push_player_tag(sheller_id);
                eb.push_player_tag(shellee_id);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{shellee_name} is Shelled!"));
                    child_eb.push_team_tag(shellee_team_id);
                    child_eb.push_player_tag(sheller_id);
                    child_eb.push_metadata_str("mod", "SHELLED");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::AddedMod)
                });
                eb.build(EventType::TasteTheInfinite)
            }
            FedEventData::BatterSkipped { game, batter_name, reason } => {
                eb.set_game(game);
                match reason {
                    BatterSkippedReason::Shelled => {
                        eb.set_description(format!("{batter_name} is Shelled and cannot escape!"));
                    }
                    BatterSkippedReason::Elsewhere(id) => {
                        eb.push_player_tag(id);
                        eb.set_description(format!("{batter_name} is Elsewhere.."));
                    }
                }
                eb.build(EventType::BatterSkipped)
            }
            FedEventData::FeedbackBlocked { game, resisted_id, resisted_name, tangled_id, tangled_team_id, tangled_name, tangled_rating_before, tangled_rating_after, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("Reality begins to flicker ...\nBut {resisted_name} resists!\n{tangled_name} is tangled in the flicker!"));
                eb.push_player_tag(resisted_id);
                eb.push_player_tag(tangled_id);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{tangled_name} is tangled in the flicker!"));
                    child_eb.push_team_tag(tangled_team_id);
                    child_eb.push_player_tag(tangled_id);
                    child_eb.push_metadata_f64("before", tangled_rating_before);
                    child_eb.push_metadata_f64("after", tangled_rating_after);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.build(EventType::PlayerStatDecrease)
                });
                eb.build(EventType::FeedbackBlocked)
            }
            FedEventData::FlagPlanted { team_id, team_nickname, ballpark_name, prefab_name, renovation_id, votes, is_first } => {
                let flag_planted_str = if is_first {
                    "!\nTHE FLAG IS PLANTED"
                } else {
                    ".\nAnother flag is planted!"
                };
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!(
                    "The {team_nickname} break ground on {ballpark_name}, selecting to build the \
                    {prefab_name} prefab{flag_planted_str}"
                ));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("renoId", renovation_id);
                eb.push_metadata_str("title", "Ground Broken");
                eb.push_metadata_i64("votes", votes);
                eb.build(EventType::FlagPlanted)
            }
            FedEventData::EmergencyAlert { message, team_tags } => {
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(message);
                eb.set_team_tags(team_tags);
                eb.build(EventType::EmergencyAlert)
            }
            FedEventData::TeamJoinedILB { team_id, team_nickname, division_id, division_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} have joined the ILB!\nThey will play in the {division_name} division."));
                eb.push_team_tag(team_id);
                eb.push_metadata_uuid("divisionId", division_id);
                eb.push_metadata_str("divisionName", division_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                eb.build(EventType::TeamDivisionMove)
            }
            FedEventData::FloodingSwept { game, effects, free_refills, balloons, flood_pumps, score_summary, flood_balloon, anti_flood_pumps } => {
                let home_team = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("A surge of Immateria rushes up from Under!");
                eb.push_description("Baserunners are swept from play!");

                for effect in &effects {
                    match effect {
                        FloodingSweptEffect::Elsewhere(sent_elsewhere) => {
                            // This form uses the same text for inner and outer description
                            let description = format!("{} {} swept Elsewhere!",
                                                      sent_elsewhere.player_name,
                                                      if self.season < 18 { "is" } else { "was" });
                            eb.push_sent_elsewhere(sent_elsewhere, &description, &description);
                        }
                        FloodingSweptEffect::Flippers { player_name, player_id, shame, .. } => {
                            eb.push_description(format!("{player_name} uses their Flippers to slingshot home!"));
                            eb.push_player_tag(*player_id);
                            eb.push_shame(&shame, home_team);
                        }
                        FloodingSweptEffect::Ego(PlayerNameId { player_name, player_id }) => {
                            eb.push_description(format!("{player_name}'s Ego keeps them on base!"));
                            eb.push_player_tag(*player_id);
                        }
                        FloodingSweptEffect::Slippery { player_name, player_id, team_id, sub_event } => {
                            let description = format!("{player_name} is now Slippery!");
                            eb.push_child(*sub_event, |mut child_eb| {
                                child_eb.push_description(&description);
                                child_eb.push_player_tag(*player_id);
                                child_eb.push_team_tag(*team_id);
                                child_eb.push_metadata_str("mod", "SLIPPERY");
                                child_eb.push_metadata_i64("type", ModDuration::Game);
                                child_eb.build(EventType::AddedMod)
                            });
                            eb.push_description(description);
                        }
                    }
                }

                if flood_pumps {
                    eb.push_description("The Flood Pumps activate!");
                }

                // Hotel motel parties from Flippers appear after flumps, so another loop is needed
                for effect in &effects {
                    if let FloodingSweptEffect::Flippers { player_name, player_id, hotel_motel_party: Some(party), .. } = effect {
                        eb.push_hotel_motel_party(party, player_name, *player_id);
                    }
                }

                // Flood balloons are definitely before normal balloons
                if flood_balloon {
                    eb.push_description("A Flood Balloon was filled!");
                }

                if anti_flood_pumps {
                    eb.push_description("The Anti Flood Pumps activate!");
                }

                eb.push_free_refills(&free_refills);
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.push_balloons_from_score_summary(score_summary.as_ref(), balloons.as_deref());

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

                            eb.push_scattered(scattered, player_id, team_id);

                            eb.push_child(sub_event, |mut child| {
                                child.push_description(&description);
                                child.push_team_tag(team_id);
                                child.push_player_tag(player_id);
                                child.push_metadata_str("mod", "ELSEWHERE");
                                child.push_metadata_i64("type", ModDuration::Permanent);
                                child.build(EventType::RemovedMod)
                            });

                            if let Some(recongeal) = recongealed_differently {
                                eb.push_child(recongeal.sub_event, |mut child| {
                                    child.push_description(format!("{} re-congealed differently.", recongeal.player_name));
                                    child.push_team_tag(recongeal.team_id);
                                    child.push_player_tag(recongeal.player_id);
                                    child.push_metadata_i64("type", ModDuration::Permanent);
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
                                child.push_metadata_i64("type", ModDuration::Permanent);
                                child.build(EventType::RemovedMod)
                            });
                        }
                        ReturnFromElsewhereFlavor::False { is_peanut } => {
                            let description = format!("{player_name} has {} from Elsewhere!",
                                                      if is_peanut { "rolled back" } else { "returned" });
                            eb.push_description(&description);
                        }
                        ReturnFromElsewhereFlavor::PulledBack { team_id, sought_player_id, seeker_player_id, seeker_player_name, scattered, sub_event, time_elsewhere } => {
                            eb.push_description(format!("{seeker_player_name} sought out Elsewhere teammate {player_name}..."));
                            eb.push_player_tag(seeker_player_id);
                            let description = if let Some(time) = time_elsewhere {
                                format!("{player_name} was pulled back from Elsewhere after {time}!")
                            } else {
                                format!("{player_name} was pulled back from Elsewhere.")
                            };
                            eb.push_description(&description);

                            eb.push_scattered(scattered, sought_player_id, team_id);

                            eb.push_child(sub_event, |mut child| {
                                child.push_description(&description);
                                child.push_team_tag(team_id);
                                child.push_player_tag(sought_player_id);
                                child.push_metadata_str("mod", "ELSEWHERE");
                                child.push_metadata_i64("type", ModDuration::Permanent);
                                child.build(EventType::RemovedMod)
                            });
                        }
                    }
                }

                eb.build(EventType::ReturnFromElsewhere)
            }
            FedEventData::Incineration {
                game,
                team_id,
                team_nickname,
                victim_id,
                victim_name,
                replacement_id,
                replacement_name,
                location,
                unstable_chain,
                incineration_sub_event,
                enters_hall_sub_event,
                hatch_sub_event,
                replacement_sub_event,
                ambush,
                pressure_built,
                heat_magnet,
            } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_player_tag(victim_id);
                eb.push_player_tag(replacement_id);

                if unstable_chain.is_some() {
                    eb.push_description(format!("{victim_name} is Unstable!"));
                    eb.push_description("A Debt was collected.");
                }

                eb.push_description(format!("Rogue Umpire incinerated {victim_name}!"));

                if heat_magnet.is_some() {
                    eb.push_description("The Heat Magnet catches. The Thermal Converter hums.");
                    eb.push_description(format!("5 Runs generated for the {team_nickname}!"));
                }

                eb.push_description(format!("They're replaced by {replacement_name}."));

                eb.push_child(incineration_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("Rogue Umpire incinerated {victim_name}!"));
                    child_eb.push_player_tag(victim_id);
                    child_eb.push_team_tag(team_id);
                    if self.season < 19 {
                        child_eb.build(EventType::Incineration)
                    } else {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_metadata_str("effect", "Incineration");
                        child_eb.push_metadata_i64("weather", Weather::SolarEclipse);
                        child_eb.build(EventType::WeatherEvent)
                    }
                });

                eb.push_child(enters_hall_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{victim_name} entered the Hall of Flame."));
                    child_eb.push_player_tag(victim_id);
                    child_eb.build(EventType::EnterHallOfFlame)
                });

                if let Some(pressure_built) = pressure_built {
                    eb.push_child(pressure_built.sub_event, |mut child_eb| {
                        child_eb.push_description("Sun(Sun)'s Pressure built...");
                        child_eb.push_metadata_f64("current", pressure_built.pressure_after);
                        child_eb.push_metadata_i64("maximum", 99999);
                        child_eb.push_metadata_i64("recharge", 26244);
                        child_eb.build(EventType::SunSunPressure)
                    });
                }

                eb.push_child(hatch_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{replacement_name} has been hatched from the field of eggs."));
                    child_eb.push_player_tag(replacement_id);
                    child_eb.push_metadata_uuid("id", replacement_id);
                    child_eb.build(EventType::PlayerHatched)
                });

                eb.push_child(replacement_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{replacement_name} replaced the incinerated {victim_name}."));
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
                    eb.push_description(format!("{} enters the {} shadows.", ambush.player_name, possessive(ambush.team_nickname.clone())));

                    if let Some(team) = ambush.former_team {
                        eb.push_child(team.sub_event, |mut child_eb| {
                            child_eb.push_description(format!("{} was pulled from the incinerated {}.", ambush.player_name, team.team_nickname));
                            child_eb.push_player_tag(ambush.player_id);
                            child_eb.push_team_tag(team.team_id);
                            child_eb.push_metadata_uuid("playerId", ambush.player_id);
                            child_eb.push_metadata_str("playerName", &ambush.player_name);
                            child_eb.push_metadata_uuid("teamId", team.team_id);
                            child_eb.push_metadata_str("teamName", &team.team_nickname);
                            child_eb.build(EventType::PlayerRemovedFromTeam)
                        });
                    }

                    eb.push_child(ambush.exit_hall_event, |mut child_eb| {
                        child_eb.push_description(format!("{} exited the Hall of Flame", ambush.player_name));
                        child_eb.push_player_tag(ambush.player_id);
                        child_eb.build(EventType::ExitHallOfFlame)
                    });
                    eb.push_child(ambush.added_to_team_event, |mut child_eb| {
                        child_eb.push_description(format!("{} joins the Ambush.", ambush.player_name));
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
                        child_eb.push_description(format!("{} entered the Shadows.", ambush.player_name));
                        child_eb.push_player_tag(ambush.player_id);
                        child_eb.push_team_tag(ambush.team_id);
                        child_eb.push_metadata_f64("after", ambush.player_rating_after);
                        child_eb.push_metadata_f64("before", ambush.player_rating_before);
                        child_eb.push_metadata_i64("type", StatChangeCategory::All);
                        child_eb.build(EventType::PlayerStatIncrease)
                    });
                }

                if let Some((score_summary, balloons)) = &heat_magnet {
                    eb.push_direct_score_summary(score_summary);
                    eb.push_balloons(balloons.as_deref(), 5);
                }

                eb.build(EventType::Incineration)
            }
            FedEventData::PitcherChange { game, team_nickname: team_name, pitcher_id, pitcher_name } => {
                eb.set_game(game);
                eb.set_description(format!("{pitcher_name} is now pitching for the {team_name}."));
                eb.set_player_tags(vec![pitcher_id]);
                eb.build(EventType::PitcherChange)
            }
            FedEventData::Party { game, team_id, player_id, player_name, sub_event, rating_before, rating_after, attracted_birds } => {
                eb.set_game(game);
                let description = format!("{player_name} is Partying!");
                eb.push_description(&description);
                eb.push_player_tag(player_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(&description);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.build(EventType::PlayerStatIncrease)
                });

                if let Some(stadium_name) = attracted_birds {
                    eb.push_description(format!("A flock of Birds are attracted to {stadium_name}!"));
                }

                eb.build(EventType::Party)
            }
            FedEventData::PlayerHatched { player_id, player_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} has been hatched from the field of eggs."));
                eb.set_player_tags(vec![player_id]);
                eb.push_metadata_uuid("id", player_id);
                eb.build(EventType::PlayerHatched)
            }
            FedEventData::PostseasonBirth { team_id, team_nickname, player_id, player_name, location } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} {} a Postseason Birth!", if self.season < 19 { "earn" } else { "earned" }));
                eb.set_player_tags(vec![player_id]);
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("location", location);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", player_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                eb.build(EventType::PlayerAddedToTeam)
            }
            FedEventData::FinalStandings { team_id, team_nickname, place, division_name } => {
                let place_str = match place {
                    0 => "1st".to_string(),
                    1 => "2nd".to_string(),
                    2 => "3rd".to_string(),
                    _ => format!("{}th", place + 1),
                };
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(format!("The {team_nickname} finished {place_str} in the {division_name}."));
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("place", place);
                eb.build(EventType::FinalStandings)
            }
            FedEventData::TeamLeftPartyTimeForPostseason { team_id, team_nickname } => {
                // TODO This was combined into another event, should it be deleted?
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} have been removed from Party Time to join the Postseason!"));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "PARTY_TIME");
                eb.push_metadata_i64("type", ModDuration::Seasonal);
                eb.build(EventType::RemovedMod)
            }
            FedEventData::EarnedPostseasonSlot { team_id, team_nickname, postseason_birth_name, postseason_birth_id, postseason_birth_location, hatch_event_metadata, postseason_birth_event_metadata, shadow_boost, left_party_event_metadata } => {
                let mut hatch_eb = eb.connected_event(hatch_event_metadata);
                hatch_eb.set_category(EventCategory::Changes);
                hatch_eb.push_description(format!("{postseason_birth_name} has been hatched from the field of eggs."));
                hatch_eb.push_player_tag(postseason_birth_id);
                hatch_eb.push_metadata_uuid("id", postseason_birth_id);
                let hatch_event = hatch_eb.build(EventType::PlayerHatched);

                let birth_event = postseason_birth_event_metadata.map(|postseason_birth_event_metadata| {
                    let mut birth_eb = eb.connected_event(postseason_birth_event_metadata);
                    birth_eb.set_category(EventCategory::Changes);
                    birth_eb.push_description(format!("The {team_nickname} {} a Postseason Birth!", if self.season < 19 { "earn" } else { "earned" }));
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
                    party_eb.push_description(format!("The {team_nickname} {} removed from Party Time to join the Postseason!", if self.season < 19 {
                        "have been"
                    } else {
                        "were"
                    }));
                    party_eb.push_team_tag(team_id);
                    party_eb.push_metadata_str("mod", "PARTY_TIME");
                    party_eb.push_metadata_i64("type", ModDuration::Seasonal);
                    party_eb.build(EventType::RemovedMod)
                });

                let order = shadow_boost.as_ref().map(|(_, order)| *order);
                let shadow_event = shadow_boost.map(|(boost, _)| {
                    let mut shadow_eb = eb.connected_event(boost.sub_event);
                    shadow_eb.set_category(EventCategory::Changes);
                    shadow_eb.push_description(format!("{postseason_birth_name} entered the Shadows."));
                    shadow_eb.push_team_tag(team_id);
                    shadow_eb.push_player_tag(postseason_birth_id);
                    shadow_eb.push_known_boost(&boost);
                    shadow_eb.build(EventType::PlayerStatIncrease)
                });

                eb.set_category(EventCategory::Outcomes);
                let overbracket_fmt = format!("Postseason Overbracket {}", self.season + 1); // wasted work but eh
                eb.push_description(format!("The {team_nickname} earned a spot in the Season {} {}.", self.season + 1, if self.season < 19 {
                    "Postseason"
                } else {
                    &overbracket_fmt
                }));
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
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(format!("The {team_nickname} advanced to {round_str} of the Season {season} Postseason."));
                eb.push_team_tag(team_id);
                eb.build(EventType::PostseasonAdvance)
            }
            FedEventData::PostseasonEliminated { team_id, team_nickname, displayed_season, bracket } => {
                eb.set_category(EventCategory::Outcomes);
                let description = format!("The {team_nickname} have been eliminated from the Season {displayed_season} Postseason.");
                let description = if let Some(BracketType::Overbracket) = bracket {
                    format!("{description} Overbracket {displayed_season}")
                } else if let Some(BracketType::Underbracket) = bracket {
                    format!("{description} Underbracket {displayed_season}")
                } else {
                    description
                };

                eb.set_description(description);
                eb.push_team_tag(team_id);
                eb.build(EventType::PostseasonEliminated)
            }
            FedEventData::PlayerBoosted { team_id, player_id, player_name, rating_before, rating_after } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} was boosted."));
                eb.set_player_tags(vec![player_id]);
                eb.push_team_tag(team_id);
                eb.push_metadata_f64("before", rating_before);
                eb.push_metadata_f64("after", rating_after);
                eb.push_metadata_i64("type", StatChangeCategory::All);
                eb.build(EventType::PlayerStatIncrease)
            }
            FedEventData::TeamEnteredPartyTime { team_id, team_nickname } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} have entered Party Time!"));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "PARTY_TIME");
                eb.push_metadata_i64("type", ModDuration::Seasonal);
                eb.build(EventType::AddedMod)
            }
            FedEventData::TeamWonInternetSeries { team_id, team_nickname, bracket_type, championships } => {
                let description = match bracket_type {
                    Some(BracketType::Underbracket) => {
                        format!("The {team_nickname} won the Season {season} Internet Series Underbracket {season}!", season=self.season + 1)
                    }
                    Some(BracketType::Overbracket) => {
                        format!("The {team_nickname} won the Season {season} Overbracket {season} Internet Series!", season=self.season + 1)
                    }
                    None => {
                        format!("The {team_nickname} won the Season {} Internet Series!", self.season + 1)
                    }
                };
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(&description);
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("championships", championships);
                if let Some(bracket) = bracket_type {
                    eb.push_metadata_i64("bracket", bracket);
                }

                eb.build(EventType::TeamWonInternetSeries)
            }
            FedEventData::BottomDwellers { team_id, team_nickname, rating_before, rating_after } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} are Bottom Dwellers."));
                eb.push_team_tag(team_id);
                eb.push_metadata_f64("before", rating_before);
                eb.push_metadata_f64("after", rating_after);
                eb.push_metadata_i64("type", StatChangeCategory::Team);
                eb.build(EventType::PlayerStatIncrease)
            }
            FedEventData::WillReceived { team_id, will_title, metadata } => {
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(format!("Will Received: {will_title}"));
                eb.push_team_tag(team_id);
                eb.set_full_metadata(metadata);
                eb.build(EventType::WillRecieved)
            }
            FedEventData::BlessingWon { team_tags, blessing_title, metadata } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("Blessing Won: {blessing_title}"));
                eb.set_team_tags(team_tags);
                eb.set_full_metadata(metadata);
                eb.build(EventType::BlessingOrGiftWon)
            }
            FedEventData::TeamSubseasonalModsChange { game, change } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                let event_type = change.source_mod.event_type();
                eb.push_team_subseasonal_mod_change(change, self.season, self.day);
                eb.build(event_type)
            }
            FedEventData::PlayerSubseasonalModsChange { game, team_changes, player_change } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_team_subseasonal_mod_changes(team_changes, self.season, self.day);
                let event_type = player_change.source_mod.event_type();
                eb.push_player_subseasonal_mod_change(player_change);
                eb.build(event_type)
            }
            FedEventData::DecreePassed { decree_title, metadata } => {
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(format!("Decree Passed: {decree_title}"));
                eb.set_full_metadata(metadata);
                eb.build(EventType::DecreePassed)
            }
            FedEventData::PlayerJoinedILB { player_id, player_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} has joined the ILB."));
                eb.set_player_tags(vec![player_id]);
                eb.push_metadata_uuid("id", player_id);
                eb.build(EventType::PlayerDivisionMove)
            }
            FedEventData::PlayerPermittedToStay { player_id, player_name } => {
                eb.set_category(EventCategory::Special);
                eb.set_description(format!("{player_name} has been permitted to stay."));
                eb.set_player_tags(vec![player_id]);
                eb.build(EventType::PlayerPermittedToStay)
            }
            FedEventData::FireproofIncineration { game, player_id, player_name, is_unstable } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if is_unstable {
                    eb.push_description(format!("{player_name} is Unstable!"));
                }
                eb.push_description(format!("Rogue Umpire tried to incinerate {player_name}, but they're Fireproof! The Umpire was incinerated instead!"));
                eb.push_player_tag(player_id);

                eb.build(EventType::IncinerationBlocked)
            }
            FedEventData::ShelledIncineration { game, player_id, player_name, is_unstable } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if is_unstable {
                    eb.push_description(format!("{player_name} is Unstable!"));
                }
                eb.push_description(format!("Rogue Umpire tried to incinerate {player_name}, but they're protected by their Shell!"));
                eb.push_player_tag(player_id);

                eb.build(EventType::IncinerationBlocked)
            }
            FedEventData::LineupSorted { team_id, team_nickname } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {} lineup has been optimized.", possessive(team_nickname)));
                eb.push_team_tag(team_id);
                eb.build(EventType::LineupSorted)
            }
            FedEventData::Undersea { game, team_name, team_id, sub_event } => {
                let description = format!("The {team_name} go Undersea. They're now Overperforming!");
                eb.set_game(game);
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&description);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", "OVERPERFORMING");
                    child_eb.push_metadata_str("source", "UNDERSEA");
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(EventType::AddedModFromOtherMod)
                });
                eb.build(EventType::Undersea)
            }
            FedEventData::RenovationBuilt { team_id, description, renovation_id, renovation_title, votes, effect } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(description);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("renoId", renovation_id);
                eb.push_metadata_str("title", renovation_title);
                match votes {
                    RenovationVotes::Normal(v) => { eb.push_metadata_i64("votes", v) }
                    RenovationVotes::Manual(v) => { eb.push_metadata_str("votes", v) }
                }

                match effect {
                    RenovationBuiltEffect::None => {}
                    RenovationBuiltEffect::ModAdded { description, mod_id, sub_event } => {
                        eb.push_child(sub_event, |mut child_eb| {
                            child_eb.set_description(description);
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_str("mod", mod_id);
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                            // Grumble grumble inconsistency
                            child_eb.clear_sub_play();
                            child_eb.build(EventType::AddedMod)
                        });
                    }
                    RenovationBuiltEffect::LightSwitchFlipped { stadium_name, is_on, sub_event } => {
                        eb.push_child(sub_event, |mut child_eb| {
                            child_eb.push_description(format!("{stadium_name}'s Light Switch is now {}.",
                                                              if is_on { "ON" } else { "OFF" }));
                            child_eb.push_team_tag(team_id);
                            // Grumble grumble inconsistency
                            child_eb.clear_sub_play();
                            child_eb.build(EventType::LightSwitchFlipped)
                        });
                    }
                }

                eb.build(EventType::RenovationBuilt)
            }
            FedEventData::PeanutMister { game, player_id, player_name, superallergy } => {
                let effect_str = if superallergy.is_some() { "is no longer Superallergic" } else { "has been cured of their peanut allergy" };
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The Peanut Mister activates!\n{player_name} {effect_str}!"));
                eb.push_player_tag(player_id);
                if let Some(superallergy) = superallergy {
                    eb.push_child(superallergy.sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Changes);
                        child_eb.push_description(format!("{player_name} lost the Superallergic mod."));
                        child_eb.push_player_tag(player_id);
                        child_eb.push_team_tag(superallergy.team_id);
                        child_eb.push_metadata_str("mod", "SUPERALLERGIC");
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.build(EventType::RemovedMod)
                    });
                }
                eb.build(EventType::PeanutMister)
            }
            FedEventData::PlayerNamedMvp { team_id, player_id, player_name, level } => {
                eb.set_category(EventCategory::Changes);
                eb.push_player_tag(player_id);
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("type", ModDuration::Permanent);
                let mod_name = format!("EGO{level}");
                if level == 1 {
                    eb.push_description(format!("{player_name} is named an MVP."));
                    eb.push_metadata_str("mod", mod_name);
                    eb.build(EventType::AddedMod)
                } else {
                    let prev_mod_name = format!("EGO{}", level - 1);
                    eb.push_description(format!("{player_name} is named a {level}-Time MVP{}",
                                                if self.season >= 21 || level == 2 { "." } else { "!" }));
                    eb.push_metadata_str("from", prev_mod_name);
                    eb.push_metadata_str("to", mod_name);

                    eb.build(EventType::ModChange)
                }
            }
            FedEventData::BirdsUnshell { game, team_id, player_id, player_name, pecked_free_event, superallergy_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The Birds circle...\nThe Birds pecked {player_name} free!"));
                eb.push_player_tag(player_id);
                eb.push_child(pecked_free_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("The Birds pecked {player_name} free!"));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_str("mod", "SHELLED");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::RemovedMod)
                });
                eb.push_child(superallergy_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{player_name} emerges from the shell with a Superallergy!"));
                    child_eb.push_team_tag(team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_str("mod", "SUPERALLERGIC");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::AddedMod)
                });
                eb.build(EventType::BirdsUnshell)
            }
            FedEventData::ReplaceReturnedPlayerFromShadows { team_id, team_nickname, promoted_player_id, promoted_player_name, promoted_location, removed_player_id, removed_player_name, removed_location } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} cut a player and promoted another from the shadows."));
                eb.set_player_tags(vec![removed_player_id, promoted_player_id]);
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("promoteLocation", promoted_location);
                eb.push_metadata_uuid("promotePlayerId", promoted_player_id);
                eb.push_metadata_str("promotePlayerName", promoted_player_name);
                eb.push_metadata_i64("removeLocation", removed_location);
                eb.push_metadata_uuid("removePlayerId", removed_player_id);
                eb.push_metadata_str("removePlayerName", removed_player_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                eb.build(EventType::PlayerReplacesReturned)
            }
            FedEventData::PlayerCalledBackToHall { player_id, player_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} entered the Hall of Flame."));
                eb.set_player_tags(vec![player_id]);
                eb.build(EventType::EnterHallOfFlame)
            }
            FedEventData::TeamUsedFreeWill { team_id, team_nickname } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {team_nickname} used their Free Will."));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "FREE_WILL");
                eb.push_metadata_i64("type", ModDuration::Permanent);
                eb.build(EventType::RemovedMod)
            }
            FedEventData::TeamUsedFreeGift { team_id, team_nickname } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {team_nickname} used their Free Gift."));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "FREE_GIFT");
                eb.push_metadata_i64("type", ModDuration::Permanent);
                eb.build(EventType::RemovedMod)
            }
            FedEventData::PlayerLostMod { team_id, player_id, player_name, r#mod, mod_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} lost the {mod_name} mod."));
                eb.set_player_tags(vec![player_id]);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", r#mod);
                eb.push_metadata_i64("type", ModDuration::Permanent);
                eb.build(EventType::RemovedMod)
            }
            FedEventData::InvestigationMessage { player_id, message } => {
                eb.set_category(EventCategory::Special);
                eb.set_description(message);
                eb.set_player_tags(vec![player_id]);
                eb.build(EventType::InvestigationMessage)
            }
            FedEventData::HighPressure { game, team_id, team_nickname, is_on, sub_event } => {
                let description = if is_on {
                    format!("The pressure is on! The {team_nickname} are Overperforming.")
                } else {
                    format!("The pressure is off. The {team_nickname} are no longer Overperforming.")
                };
                eb.set_game(game);
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&description);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", "OVERPERFORMING");
                    child_eb.push_metadata_str("source", "HIGH_PRESSURE");
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(if is_on { EventType::AddedModFromOtherMod } else { EventType::RemovedModFromOtherMod })
                });
                eb.build(EventType::HighPressure)
            }
            FedEventData::PlayerPulledThroughRift { player_id, player_name } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} was pulled through the Rift."));
                eb.set_player_tags(vec![player_id]);
                eb.push_metadata_uuid("id", player_id);
                eb.build(EventType::PlayerDivisionMove)
            }
            FedEventData::PlayerLocalized { team_id, team_nickname, player_id, player_name, location } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} Localized into the {} {}.", possessive(team_nickname.clone()), location.location()));
                eb.set_player_tags(vec![player_id]);
                eb.push_team_tag(team_id);
                eb.push_metadata_i64("location", location);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", player_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                eb.build(EventType::PlayerAddedToTeam)
            }
            FedEventData::Echo { game, echoee_name, primary_echo, receiver_echos, } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{} Echoed {echoee_name}!", primary_echo.receiver_name));

                if let Some(mods_removed) = primary_echo.mods_removed {
                    eb.push_child(mods_removed.sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{}'s Echo faded.", primary_echo.receiver_name));
                        child_eb.push_player_tag(primary_echo.receiver_id);
                        child_eb.push_team_tag(primary_echo.receiver_team_id);

                        let removes_vec = mods_removed.mods.into_iter()
                            .map(|m| json!({ "mod": m.mod_id, "type": m.mod_duration as i64 }))
                            .collect();
                        child_eb.push_metadata_json_vec("removes", removes_vec);
                        child_eb.push_metadata_str("source", "ECHO");

                        child_eb.build(EventType::RemovedModsFromAnotherMod)
                    });
                }

                eb.push_child(primary_echo.mods_added.sub_event, |mut child_eb| {
                    child_eb.push_description(format!( "{} Echoed {echoee_name}!", primary_echo.receiver_name));
                    child_eb.push_player_tag(primary_echo.receiver_id);
                    child_eb.push_team_tag(primary_echo.receiver_team_id);

                    let adds_vec = primary_echo.mods_added.mods.into_iter()
                        .map(|m| json!({ "mod": m.mod_id, "type": m.mod_duration as i64 }))
                        .collect();
                    child_eb.push_metadata_json_vec("adds", adds_vec);
                    child_eb.push_metadata_str("source", "ECHO");

                    child_eb.build(EventType::AddedModsFromAnotherMod)
                });

                for receiver_echo in receiver_echos {
                    if let Some(mods_removed) = receiver_echo.mods_removed {
                        eb.push_child(mods_removed.sub_event, |mut child_eb| {
                            child_eb.push_description(format!( "{}'s Echoed Echo faded.", receiver_echo.receiver_name));
                            child_eb.push_player_tag(receiver_echo.receiver_id);
                            child_eb.push_team_tag(receiver_echo.receiver_team_id);

                            let removes_vec = mods_removed.mods.into_iter()
                                .map(|m| json!({ "mod": m.mod_id, "type": m.mod_duration as i64 }))
                                .collect();
                            child_eb.push_metadata_json_vec("removes", removes_vec);
                            child_eb.push_metadata_str("source", "RECEIVER");

                            child_eb.build(EventType::RemovedModsFromAnotherMod)
                        });
                    }

                    eb.push_child(receiver_echo.mods_added.sub_event, |mut child_eb| {
                        child_eb.push_description(format!( "{}'s Echoed an Echo from {}!", receiver_echo.receiver_name, primary_echo.receiver_name));
                        child_eb.push_player_tag(receiver_echo.receiver_id);
                        child_eb.push_team_tag(receiver_echo.receiver_team_id);

                        let adds_vec = receiver_echo.mods_added.mods.into_iter()
                            .map(|m| json!({ "mod": m.mod_id, "type": m.mod_duration as i64 }))
                            .collect();
                        child_eb.push_metadata_json_vec("adds", adds_vec);
                        child_eb.push_metadata_str("source", "RECEIVER");

                        child_eb.build(EventType::AddedModsFromAnotherMod)
                    });
                }

                eb.build(EventType::Echo)
            }
            FedEventData::SolarPanelsAwait { game } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Solar Panels are angled toward Sun 2.");
                eb.build(EventType::SolarPanelsAwait)
            }
            FedEventData::EventHorizonAwaits { game } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Event Horizon awaits.");
                eb.build(EventType::EventHorizonAwaits)
            }
            FedEventData::EchoIntoStatic { game, echoer, echoee } => {
                let description = format!("ECHO {} STATIC\nECHO {} STATIC", echoer.player_name, echoee.player_name);
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(description.clone());
                let mut push_sub_event = |echo: &EchoIntoStatic, sub_event: &SubEvent, event_type: EventType| {
                    eb.push_child(*sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Changes);
                        child_eb.push_description(description.clone());
                        child_eb.push_player_tag(echo.player_id);
                        child_eb.push_team_tag(echo.team_id);
                        if event_type == EventType::PlayerRemovedFromTeam {
                            child_eb.push_metadata_uuid("playerId", echo.player_id);
                            child_eb.push_metadata_str("playerName", &echo.player_name);
                            child_eb.push_metadata_uuid("teamId", echo.team_id);
                            child_eb.push_metadata_str("teamName", &echo.team_nickname);
                        } else {
                            child_eb.push_metadata_str("from", "ECHO");
                            child_eb.push_metadata_str("to", "STATIC");
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        }
                        child_eb.build(event_type)
                    });
                };
                push_sub_event(&echoer, &echoer.removed_from_team_sub_event, EventType::PlayerRemovedFromTeam);
                push_sub_event(&echoee, &echoee.removed_from_team_sub_event, EventType::PlayerRemovedFromTeam);
                push_sub_event(&echoer, &echoer.mod_changed_sub_event, EventType::ModChange);
                push_sub_event(&echoee, &echoee.mod_changed_sub_event, EventType::ModChange);
                eb.build(EventType::EchoIntoStatic)
            }
            FedEventData::ConsumerAttack { game, team_id, player_id, player_name_all_caps, effect, sensed_something_fishy, scattered } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_player_tag(player_id);
                eb.push_description("CONSUMERS ATTACK");
                if scattered {
                    eb.push_description("SCATTERED");
                }

                match effect {
                    ConsumerAttackEffect::Chomp { rating_before, rating_after, sub_event } => {
                        eb.push_description(&player_name_all_caps);
                        let description = eb.description().to_string();
                        eb.push_child(sub_event, |mut child| {
                            child.push_player_tag(player_id);
                            child.push_team_tag(team_id);
                            child.set_description(description);
                            child.build_player_stat_changed(rating_before, rating_after, StatChangeCategory::All)
                        });
                    }
                    ConsumerAttackEffect::DefendedWithItem(damage) => {
                        // Sticking the extra \n here arbitrarily. There are two in a row.
                        eb.push_description(format!("{player_name_all_caps} DEFENDS\n"));
                        if damage.health > 0 {
                            eb.push_description(format!("{} DAMAGED", damage.item_name.to_ascii_uppercase()));
                        } else if damage.item_name_plural.expect("When item health > 0, whether its name is plural should be known") {
                            eb.push_description(format!("{} BREAK", damage.item_name.to_ascii_uppercase()));
                        } else {
                            eb.push_description(format!("{} BREAKS", damage.item_name.to_ascii_uppercase()));
                        }
                        let description = eb.description().to_string();
                        eb.push_child(damage.sub_event, |mut child| {
                            child.set_description(description);
                            child.build_item_damaged(damage)
                        });
                    }
                }

                if let Some(fishy) = sensed_something_fishy {
                    eb.push_child(fishy.sub_event, |mut child| {
                        child.push_description(format!("{} sensed something fishy.", fishy.player_name));
                        child.build_detective_activity(fishy)
                    });
                }

                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::Psychoacoustics { game, stadium_name, team_id, team_nickname, mod_name, mod_id, sub_event, team_subseasonal_mod_changes } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_team_subseasonal_mod_changes(team_subseasonal_mod_changes, self.season, self.day);

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
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(&description);
                    child_eb.push_player_tag(echoee_id);
                    child_eb.push_team_tag(echoee_team_id);
                    child_eb.push_metadata_str("from", "RECEIVER");
                    child_eb.push_metadata_str("to", "ECHO");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::ModChange)
                });
                eb.build(EventType::EchoReciever)
            }
            FedEventData::TeamGainedFreeWill { team_id, team_nickname } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The {team_nickname} gain Free Will."));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "FREE_WILL");
                eb.push_metadata_i64("type", ModDuration::Permanent);
                eb.build(EventType::AddedMod)
            }
            FedEventData::Tidings { message, metadata, player_tags } => {
                eb.set_category(EventCategory::Outcomes);
                eb.set_description(message);
                eb.set_player_tags(player_tags);
                eb.set_full_metadata(metadata);
                eb.build(EventType::Tidings)
            }
            FedEventData::HomebodyGameStart { game, homebodies } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                for toggle in homebodies {
                    let description = format!("{} is {}.", toggle.player_name,
                                              if toggle.is_overperforming { "happy to be home" } else { "homesick" });
                    eb.push_toggle_performing_child(toggle, &description, "HOMEBODY");
                    eb.push_description(description);
                }

                eb.build(EventType::Homebody)
            }
            FedEventData::SalmonSwim { game, inning_num, run_losses, item_repaired: item_restored, player_expelled } => {
                eb.set_game(game);
                eb.set_category(EventCategory::special_if(player_expelled.is_some()));
                eb.push_description("The Salmon swim upstream!");
                eb.push_description(format!("Inning {inning_num} begins again."));
                eb.push_description(run_losses.to_string());

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
                    eb.push_sent_elsewhere(&sent_elsewhere, &outer_description, &inner_description);
                }

                eb.build(EventType::SalmonSwim)
            }
            FedEventData::HitByPitch { game, pitcher_id, pitcher_name, batter_team_id, batter_id, batter_name, debt_type, sub_event, scores, stopped_inhabiting } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                eb.push_description(format!("{pitcher_name} hits {batter_name} with a pitch!"));
                let debt_description = match debt_type {
                    DebtType::Unstable => {
                        format!("{batter_name} became Unstable!")
                    }
                    DebtType::Observed => {
                        format!("{batter_name} is now being Observed...")
                    }
                };
                eb.push_description(&debt_description);
                eb.push_player_tag(pitcher_id);
                eb.push_player_tag(batter_id);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(&debt_description);
                    child_eb.push_team_tag(batter_team_id);
                    child_eb.push_player_tag(batter_id);
                    child_eb.push_metadata_str("mod", debt_type.mod_id());
                    child_eb.push_metadata_i64("type", ModDuration::Weekly);
                    child_eb.build(EventType::AddedMod)
                });

                eb.push_scores(&scores, home_team_id, "scores!", false, false);

                eb.push_stopped_inhabiting(stopped_inhabiting.as_ref());

                eb.build(EventType::HitByPitch)
            }
            FedEventData::SolarPanelsActivate { game, num_runs, team_nickname } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Solar Panels absorb Sun 2's energy!");
                eb.push_description(format!("{num_runs} Runs are collected and saved for the {team_nickname}'s next game."));
                eb.build(EventType::SolarPanelsActivation)
            }
            FedEventData::RunsOverflowing { game, team_nickname, num_runs, unruns, gained, score_summary, balloons } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Runs are Overflowing!");
                eb.push_description(format!("{team_nickname} {} {num_runs} {}{}.",
                                            if self.season >= 22 { "collect" } else if gained { "gain" } else { "lose" },
                                            if unruns { "Unrun" } else { "Run" },
                                            if num_runs.abs() == 1.0 { "" } else { "s" }));
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.push_balloons_from_score_summary(score_summary.as_ref(), balloons.as_deref());
                eb.build(EventType::RunsOverflowing)
            }
            FedEventData::EnterCrimeScene { game, player_id, player_name, previous_team_id, previous_team_name, previous_location, new_team_id, new_team_name, stadium_name, rating_before, rating_after, enter_crime_scene_sub_event: crime_scene_sub_event, enter_shadows_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} enters the Crime Scene at {stadium_name} to Investigate..."));
                eb.push_child(crime_scene_sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{player_name} entered the Crime Scene at {stadium_name} to Investigate..."));
                    child_eb.push_team_tag(previous_team_id);
                    child_eb.push_team_tag(new_team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_i64("location", previous_location);
                    child_eb.push_metadata_uuid("playerId", player_id);
                    child_eb.push_metadata_str("playerName", &player_name);
                    child_eb.push_metadata_i64("receiveLocation", 3);
                    child_eb.push_metadata_uuid("receiveTeamId", new_team_id);
                    child_eb.push_metadata_str("receiveTeamName", &new_team_name);
                    child_eb.push_metadata_uuid("sendTeamId", previous_team_id);
                    child_eb.push_metadata_str("sendTeamName", &previous_team_name);
                    child_eb.build(EventType::PlayerMoved)
                });
                eb.push_child(enter_shadows_sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description(format!("{player_name} entered the Shadows."));
                    child_eb.push_team_tag(new_team_id);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_f64("before", rating_before);
                    child_eb.push_metadata_f64("after", rating_after);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.build(EventType::PlayerStatIncrease)
                });
                eb.build(EventType::EnterCrimeScene)
            }
            FedEventData::ReturnFromInvestigation { player_id, player_name, previous_team_id, previous_team_name, new_location, new_team_id, new_team_name, emptyhanded } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("{player_name} returns from the Investigation{}.",
                                           if emptyhanded { " emptyhanded" } else { "" }));
                eb.set_player_tags(vec![player_id]);
                eb.push_team_tag(previous_team_id);
                eb.push_team_tag(new_team_id);
                eb.push_metadata_i64("location", PositionType::Bullpen);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", player_name);
                eb.push_metadata_i64("receiveLocation", new_location);
                eb.push_metadata_uuid("receiveTeamId", new_team_id);
                eb.push_metadata_str("receiveTeamName", new_team_name);
                eb.push_metadata_uuid("sendTeamId", previous_team_id);
                eb.push_metadata_str("sendTeamName", previous_team_name);
                eb.build(EventType::PlayerMoved)
            }
            FedEventData::InvestigationConcluded { stadium_name, team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.set_description(format!("The Crime Scene Investigation at {stadium_name} has concluded."));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "CRIME_SCENE");
                eb.push_metadata_i64("type", ModDuration::Permanent);
                eb.build(EventType::RemovedMod)
            }
            FedEventData::GrindRail { game, player_id, player_name, first_trick, success } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.set_description(format!("{player_name} hops on the Grind Rail toward third base.\nThey do a {first_trick}!\n{success}"));
                eb.set_player_tags(vec![player_id]);
                eb.build(EventType::GrindRail)
            }
            FedEventData::EnterSecretBase { game, player_id, player_name, deep_darkness } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} enters the Secret Base..."));
                eb.push_player_tag(player_id);

                if let Some(deep_darkness_event) = deep_darkness {
                    eb.push_child(deep_darkness_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description(format!("{player_name} senses a Deep Darkness..."));
                        child_eb.push_player_tag(player_id);

                        child_eb.build(EventType::InvestigationMessage)
                    })
                }

                eb.build(EventType::EnterSecretBase)
            }
            FedEventData::ExitSecretBase { game, player_id, player_name, to_fifth } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{player_name} exits the Secret Base to {} Base!",
                                            if to_fifth { "the Fifth" } else { "Second" }));
                eb.push_player_tag(player_id);
                eb.build(EventType::ExitSecretBase)
            }
            FedEventData::EchoChamber { game, team_id, player_id, player_name, which_mod, sub_event } => {
                let mod_id = match which_mod {
                    EchoChamberModAdded::Repeating => { "REPEATING" }
                    EchoChamberModAdded::Reverberating => { "REVERBERATING" }
                };
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The Echo Chamber traps a wave.\n{player_name} is temporarily {which_mod}!"));
                eb.push_player_tag(player_id);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.push_description("The Echo Chamber traps a wave.");
                    if let Some(team_id) = team_id {
                        child_eb.push_team_tag(team_id);
                    }
                    child_eb.push_player_tag(player_id);
                    child_eb.push_metadata_str("mod", mod_id);
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(EventType::AddedMod)
                });
                eb.build(EventType::EchoChamber)
            }
            FedEventData::Roam { is_super, player_id, player_name, location, new_team_id, new_team_nickname, roam_from: RoamFromLocation::Team { previous_team_id, previous_team_nickname }, connected_events } => {
                let mut events = eb.build_roam_connected_events(
                    &connected_events,
                    &player_name,
                    player_id,
                    Some(previous_team_id),
                    new_team_id,
                );

                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{player_name} {} to a new team.",
                                            if self.season < 17 { "wandered" } else if is_super { "super roamed" } else { "roamed" }));
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
            FedEventData::Roam { is_super, player_id, player_name, location, new_team_id, new_team_nickname, roam_from: RoamFromLocation::HallOfFlame { sub_event, from_team }, connected_events } => {
                let mut events = eb.build_firewalker_events(
                    connected_events.firewalker.as_ref(),
                    &player_name,
                    player_id,
                    from_team.as_ref().map(|from_team| from_team.incinerated_team_id),
                );

                if let Some(from_team) = from_team {
                    let mut left_incinerated_team_eb = eb.connected_event(from_team.sub_event);
                    left_incinerated_team_eb.set_category(EventCategory::Changes);
                    left_incinerated_team_eb.push_description(format!("{player_name} was pulled from the incinerated {}.", from_team.incinerated_team_nickname));
                    left_incinerated_team_eb.push_player_tag(player_id);
                    left_incinerated_team_eb.push_team_tag(from_team.incinerated_team_id);
                    left_incinerated_team_eb.push_metadata_uuid("playerId", player_id);
                    left_incinerated_team_eb.push_metadata_str("playerName", &player_name);
                    left_incinerated_team_eb.push_metadata_uuid("teamId", from_team.incinerated_team_id);
                    left_incinerated_team_eb.push_metadata_str("teamName", from_team.incinerated_team_nickname);
                    events.insert(0, left_incinerated_team_eb.build(EventType::PlayerRemovedFromTeam));
                }

                // In season 22 they capitalized the R
                let roamed = if is_super { "Super Roamed" } else if self.season < 21 { "roamed" } else { "Roamed" };
                // Annoying
                let sub_roamed = if self.season < 21 { "roamed" } else { "Roamed" };

                // We need to build this before consuming `eb`, but it gets added later
                let mut team_eb = eb.connected_event(sub_event);
                team_eb.set_category(EventCategory::Changes);
                team_eb.push_description(format!("{player_name} {sub_roamed} to The {new_team_nickname}."));
                team_eb.push_player_tag(player_id);
                team_eb.push_team_tag(new_team_id);
                team_eb.push_metadata_i64("location", location);
                team_eb.push_metadata_uuid("playerId", player_id);
                team_eb.push_metadata_str("playerName", &player_name);
                team_eb.push_metadata_uuid("teamId", new_team_id);
                team_eb.push_metadata_str("teamName", new_team_nickname);
                let player_added_to_team_event = team_eb.build(EventType::PlayerAddedToTeam);

                // We need to build these before consuming `eb`, but they get added later
                let connected_events = eb.build_roam_boost_connected_events(
                    &connected_events,
                    &player_name,
                    player_id,
                    None,
                    new_team_id,
                );

                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{player_name} {roamed} out of the Hall of Flame."));
                eb.push_player_tag(player_id);
                events.push(eb.build(EventType::ExitHallOfFlame));

                // Now we can add all the ones we built earlier
                events.push(player_added_to_team_event);
                events.extend(connected_events);

                return events;
            }
            FedEventData::Roam { is_super, player_id, player_name, location, new_team_id, new_team_nickname, roam_from: RoamFromLocation::Vault { sub_event }, connected_events } => {
                let mut events = eb.build_firewalker_events(
                    connected_events.firewalker.as_ref(),
                    &player_name,
                    player_id,
                    None,
                );

                // In season 22 they capitalized the R
                let roamed = if is_super { "Super Roamed" } else if self.season < 21 { "roamed" } else { "Roamed" };

                let mut team_eb = eb.connected_event(sub_event);
                team_eb.set_category(EventCategory::Changes);
                team_eb.push_description(format!("{player_name} {roamed} to the {new_team_nickname}."));
                team_eb.push_player_tag(player_id);
                team_eb.push_team_tag(new_team_id);
                team_eb.push_metadata_i64("location", location);
                team_eb.push_metadata_uuid("playerId", player_id);
                team_eb.push_metadata_str("playerName", &player_name);
                team_eb.push_metadata_uuid("teamId", new_team_id);
                team_eb.push_metadata_str("teamName", new_team_nickname);
                events.push(team_eb.build(EventType::PlayerAddedToTeam));

                events.extend(eb.build_roam_boost_connected_events(
                    &connected_events,
                    &player_name,
                    player_id,
                    None,
                    new_team_id,
                ));

                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{player_name} {roamed} out of the Vault."));
                eb.push_player_tag(player_id);
                events.insert(0, eb.build(EventType::PlayerLeftVault));

                return events;
            }
            FedEventData::GlitterCrate { game, player_name, gained_item } => {
                eb.set_game(game);
                eb.push_description("A shimmering Crate descends.");
                eb.push_gained_item(&player_name, gained_item);
                eb.build(EventType::GlitterCrateDrop)
            }
            FedEventData::ModsFromAnotherModRemoved { team_id, player_id, player_name, mods_removed, source_mod_name, source_mod_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{} mods caused by {source_mod_name} were removed.", Possessive(&player_name)));
                eb.push_player_tag(player_id);
                eb.push_team_tag(team_id);
                eb.push_metadata_str("source", source_mod_id);
                eb.push_metadata_json_vec("removes", mods_removed.iter()
                    .map(|r| json!({ "mod": r.mod_id, "type": r.mod_duration }))
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
            FedEventData::ConsumerDefended { game, exclamation, verb, defender_name_caps, defender_id, targeted_player_id, } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{exclamation}!"));
                eb.push_description(format!("{defender_name_caps} {verb} A CONSUMER!"));
                eb.push_player_tag(defender_id);
                eb.push_player_tag(targeted_player_id);
                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::ConsumerCountered { game, defender_name, defender_id, item_damaged, targeted_player_id, } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("CONSUMERS ATTACK");
                let defender_name_caps = defender_name.to_uppercase();
                let item_name_caps = item_damaged.item_name.to_uppercase();
                eb.push_description(format!("STEELED {defender_name_caps} COUNTERED WITH THE {item_name_caps}"));
                eb.push_player_tag(defender_id);
                eb.push_player_tag(targeted_player_id);

                let damage_description = format!("{defender_name} damaged their {} on a Consumer.", item_damaged.item_name);
                eb.push_item_damage_with_description(&item_damaged, &damage_description);

                eb.build(EventType::ConsumersAttack)
            }
            FedEventData::MindTrickWalk { game, pitch, strikeout_type, batter_id, batter_name, base_instincts, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_description(format!("{batter_name} strikes out {strikeout_type}."));
                eb.push_description(format!("{batter_name} uses a Mind Trick!"));
                eb.push_description("The umpire sends them to first base.");
                if let Some(base) = base_instincts {
                    eb.push_description(format!("Base Instincts take them directly to {base} base!"));
                }
                eb.push_scores(&scores, home_team_id, "scores!", false, self.season < 21);
                eb.push_player_tag(batter_id);
                eb.build(EventType::Walk)
            }
            FedEventData::CharmedMindTrickWalk { game, pitch, pitcher_id, pitcher_name, batter_id, batter_name, scores } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_description(format!("{pitcher_name} charmed {batter_name}!"));
                eb.push_description(format!("{batter_name} swings 3 times to strike out willingly!"));
                eb.push_description(format!("{batter_name} uses a Mind Trick!"));
                eb.push_description("The umpire sends them to first base.");
                // There sure are a lot of redundant player ids in this event
                eb.push_player_tag(pitcher_id);
                eb.push_player_tag(pitcher_id);
                eb.push_player_tag(batter_id);
                eb.push_player_tag(batter_id);
                eb.push_scores(&scores, home_team_id, "scores!", false, self.season < 21);
                eb.build(EventType::Walk)
            }
            FedEventData::MindTrickStrikeout { game, pitch, batter_id, batter_name, pitcher_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                // Before s18d94 the mind trick strikeout was classed as a walk and had the
                // batter_id in there twice.
                if (self.season, self.day) < (17, 93) {
                    eb.push_description(format!("{batter_name} draws a walk."));
                    eb.push_player_tag(batter_id);
                }
                eb.push_description(format!("{pitcher_name} uses a Mind Trick!"));
                eb.push_description(format!("{batter_name} strikes out thinking."));
                eb.push_player_tag(batter_id); // batter twice, apparently
                eb.build(if (self.season, self.day) < (17, 93) { EventType::Walk } else { EventType::Strikeout })
            }
            FedEventData::BlooddrainBlocked { game, is_siphon, sipper_id, sipper_name, sippee_id, sippee_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Blooddrain gurgled!");
                if is_siphon {
                    eb.push_description(format!("{sipper_name}'s Siphon activates!"));
                }
                eb.push_description(format!("{sipper_name} tried to siphon blood from {sippee_name}, but they were Sealed!"));
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
                eb.push_description(format!("The Community Chest Opens! {player_name} gained {item_name}."));
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
                eb.push_description(format!("{player_name} dropped {item_name}."));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("itemId", item_id);
                eb.push_metadata_str("itemName", item_name);
                eb.push_metadata_str_vec("mods", item_mods);
                eb.push_metadata_f64_opt("playerItemRatingAfter", player_item_rating_after);
                eb.push_metadata_f64_opt("playerItemRatingBefore", player_item_rating_before);
                eb.push_metadata_f64("playerRating", player_rating);
                eb.build(EventType::PlayerLostItem)
            }
            FedEventData::CommunityChestGameMessage { game, first_player_name, first_player_item_name, first_player_dropped_item, second_player_name, second_player_item_name, second_player_dropped_item } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Community Chest Opens!");
                if let Some(dropped_item) = first_player_dropped_item {
                    eb.push_description(format!("{first_player_name} gained {first_player_item_name} and dropped {dropped_item}."));
                } else {
                    eb.push_description(format!("{first_player_name} gained {first_player_item_name}."));
                }
                if let Some(dropped_item) = second_player_dropped_item {
                    eb.push_description(format!("{second_player_name} gained {second_player_item_name} and dropped {dropped_item}."));
                } else {
                    eb.push_description(format!("{second_player_name} gained {second_player_item_name}."));
                }
                eb.build(EventType::CommunityChestOpens)
            }
            FedEventData::Fax { game, team_id, team_nickname, exiting_pitcher_id, exiting_pitcher_name, entering_pitcher_id, entering_pitcher_name, shadows_location, rating_before, rating_after, player_swap_sub_event, enter_shadows_sub_event, yolked_blip } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("10 Runs collected.");
                eb.push_description("Incoming Shadow Fax...");
                eb.push_description(format!("{exiting_pitcher_name} is replaced by {entering_pitcher_name}."));
                eb.push_player_tag(exiting_pitcher_id);
                eb.push_player_tag(entering_pitcher_id);
                eb.push_child(player_swap_sub_event, |mut child| {
                    // They changed the text in season 18
                    child.push_description(if self.season < 17 {
                        format!("The {team_nickname} made a roster move.")
                    } else {
                        format!("{exiting_pitcher_name} was replaced by an incoming Fax.")
                    });
                    child.push_player_tag(exiting_pitcher_id);
                    child.push_player_tag(entering_pitcher_id);
                    child.push_team_tag(team_id);
                    child.push_metadata_i64("aLocation", PositionType::Rotation);
                    child.push_metadata_uuid("aPlayerId", exiting_pitcher_id);
                    child.push_metadata_str("aPlayerName", &exiting_pitcher_name);
                    child.push_metadata_i64("bLocation", shadows_location);
                    child.push_metadata_uuid("bPlayerId", entering_pitcher_id);
                    child.push_metadata_str("bPlayerName", &entering_pitcher_name);
                    child.push_metadata_uuid("teamId", team_id);
                    child.push_metadata_str("teamName", team_nickname);
                    child.build(EventType::PlayerSwap)
                });
                eb.push_child(enter_shadows_sub_event, |mut child| {
                    child.push_description(format!("{exiting_pitcher_name} entered the Shadows."));
                    child.push_player_tag(exiting_pitcher_id);
                    // TODO: Why does this specific event not have a team tag here?
                    if self.id != uuid::uuid!("c341cd11-e218-4acf-baed-8521c8f62d5a") {
                        child.push_team_tag(team_id);
                    }
                    child.push_metadata_f64("after", rating_after);
                    child.push_metadata_f64("before", rating_before);
                    child.push_metadata_i64("type", StatChangeCategory::All);
                    child.build(EventType::PlayerStatIncrease)
                });

                if let Some(blip) = yolked_blip {
                    eb.push_child(blip.removal.sub_event, |mut child_eb| {
                        // Ignoring other_player_names until it becomes relevant
                        child_eb.push_description(format!("{exiting_pitcher_name} are weaker apart."));
                        child_eb.push_player_tag(exiting_pitcher_id);
                        child_eb.push_team_tag(team_id);
                        child_eb.push_metadata_str("mod", "YOLKED");
                        child_eb.push_metadata_str("source", "HARD_BOILED");
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.build(EventType::RemovedModFromOtherMod)
                    });
                    if let Some(addition) = blip.addition {
                        eb.push_child(addition.sub_event, |mut child_eb| {
                            let names_str = iter::once(&exiting_pitcher_name)
                                .chain(addition.other_player_names.iter())
                                .join(" and ");
                            child_eb.push_description(format!("{names_str} are stronger together."));
                            child_eb.push_player_tag(exiting_pitcher_id);
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_str("mod", "YOLKED");
                            child_eb.push_metadata_str("source", "HARD_BOILED");
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                            child_eb.build(EventType::AddedModFromOtherMod)
                        });
                    }
                }

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
                eb.push_description(format!("Smithy beckons to {}.", repair.player_name));
                eb.push_player_tag(repair.player_id);
                // This one doesn't seem to do plurals
                eb.push_description(format!("{} is repaired!", repair.item_name));
                eb.push_child(repair.sub_event, |mut child| {
                    child.push_description(format!("{} {} was repaired by Smithy.", Possessive(&repair.player_name), repair.item_name));
                    child.build_item_repaired(repair)
                });
                eb.build(EventType::Smithy)
            }
            FedEventData::HolidayInning { game, inning_number } => {
                eb.set_game(game);
                eb.push_description("Hotel Motel");
                eb.push_description(format!("Inning {inning_number} is a Holiday Inning!"));
                eb.build(EventType::HolidayInning)
            }
            FedEventData::HomeFieldAdvantage { game, team_nickname } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} apply Home Field advantage!"));
                eb.build(EventType::HomeFieldAdvantage)
            }
            FedEventData::PrizeMatch { game, item_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("Prize Match!\nThe Winner gets {item_name}"));
                eb.build(EventType::PrizeMatch)
            }
            FedEventData::WonPrizeMatch { team_nickname_or_player_name, team_id, player_id, item_id, item_name, item_mods, player_item_rating_before, player_item_rating_after, player_rating } => {
                eb.set_category(EventCategory::Changes);
                match team_nickname_or_player_name {
                    TeamNicknameOrPlayerName::TeamNickname(team_nickname) => {
                        eb.push_description(format!("The {team_nickname} won the Prize Match!"));
                    }
                    TeamNicknameOrPlayerName::PlayerName(player_name) => {
                        eb.push_description(format!("{player_name} gained the Prized {item_name}."));
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
            FedEventData::GiftReceived { team_id, title_and_recipient, metadata, mut successors } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("Gift Received: {title_and_recipient}"));
                eb.push_team_tag(team_id);
                eb.set_full_metadata(metadata);
                let main = eb.build(EventType::BlessingOrGiftWon);
                successors.insert(0, main);
                return successors;
            }
            FedEventData::ReplicaFadedToDust { team_id, team_nickname, player_id, player_name, mod_added_event, weaker_apart_event } => {
                let mut dust_eb = eb.connected_event(mod_added_event);
                dust_eb.set_category(EventCategory::Changes);
                dust_eb.push_description(format!("{player_name} faded to dust."));
                dust_eb.push_team_tag(team_id);
                dust_eb.push_player_tag(player_id);
                dust_eb.push_metadata_str("mod", "DUST");
                dust_eb.push_metadata_i64("type", ModDuration::Permanent);
                let dust_add_event = dust_eb.build(EventType::AddedMod);

                let weaker_apart_event = weaker_apart_event.map(|weaker_apart| {
                    let mut weaker_apart_eb = eb.connected_event(weaker_apart.sub_event);
                    weaker_apart_eb.set_category(EventCategory::Changes);
                    let names_str = iter::once(&player_name)
                        .chain(weaker_apart.other_player_names.iter())
                        .join(" and ");
                    weaker_apart_eb.push_description(format!("{names_str} are weaker apart."));
                    weaker_apart_eb.push_team_tag(team_id);
                    weaker_apart_eb.push_player_tag(player_id);
                    weaker_apart_eb.push_metadata_str("mod", "YOLKED");
                    weaker_apart_eb.push_metadata_str("source", "HARD_BOILED");
                    weaker_apart_eb.push_metadata_i64("type", ModDuration::Permanent);
                    weaker_apart_eb.build(EventType::RemovedModFromOtherMod)
                });

                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{player_name} faded away from the {team_nickname}."));
                eb.push_team_tag(team_id);
                eb.push_player_tag(player_id);
                eb.push_metadata_uuid("playerId", player_id);
                eb.push_metadata_str("playerName", &player_name);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);
                let main_event = eb.build(EventType::PlayerRemovedFromTeam);

                let mut events = vec![main_event, dust_add_event];
                if let Some(e) = weaker_apart_event { events.push(e); }

                return events;
            }
            FedEventData::ABloodType { game, team_id, team_nickname, blood_type_mod_id, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} have A Blood Type."));
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(format!("The {team_nickname} have A Blood Type."));
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
                            child_eb.push_metadata_i64("before", Weather::PolarityMinus);
                            child_eb.push_metadata_i64("after", Weather::PolarityPlus);
                        }
                        NumbersGo::Down => {
                            child_eb.push_metadata_i64("before", Weather::PolarityPlus);
                            child_eb.push_metadata_i64("after", Weather::PolarityMinus);
                        }
                    }
                    child_eb.build(EventType::WeatherChange)
                });
                eb.build(EventType::PolarityShift)
            }
            FedEventData::DonatedShameApplied { game, team_nickname, unruns, score_summary } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Shame Donations are granted!");
                eb.push_description(format!("The {team_nickname} receive {unruns} Unruns."));

                eb.push_opt_direct_score_summary(score_summary.as_ref());

                eb.build(EventType::ShameDonor)
            }
            FedEventData::GameOver { game, earned_win, temp_stolen_players_returned } => {
                eb.set_game(game);
                eb.push_description("Game Over.");

                if let Some(win) = earned_win {
                    eb.push_earned_win(win);
                }

                for player_return in temp_stolen_players_returned {
                    eb.push_temp_stolen_player_returned(&player_return);
                }

                eb.build(EventType::GameOver)
            }
            FedEventData::BalloonsCollectedFromWin { game, stadium_name, earned_win } => {
                eb.set_game(game);
                eb.push_description(format!("{stadium_name} {} 10 Balloons!", eb.inflated_or_inflates()));
                eb.push_earned_win_opt(earned_win);

                eb.build(EventType::BalloonsInflatedFromWin)
            }
            FedEventData::Moderation { game, team_nickname, shame, score_summary } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} practice Moderation."));
                eb.push_shame(&shame, home_team_id);
                eb.push_opt_direct_score_summary(score_summary.as_ref());
                eb.build(EventType::Moderation)
            }
            FedEventData::PlacedFifthBase { game, player_id, player_name, player_team_id, player_item_rating_before, player_item_rating_after, player_rating, stadium_name, player_lost_item_event, stadium_gained_mod_event } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.push_description(format!("{player_name} placed and stole to The Fifth Base!"));
                eb.push_player_tag(player_id);

                eb.push_child(player_lost_item_event, |mut child_eb| {
                    child_eb.push_description(format!("{player_name} placed The Fifth Base in {stadium_name}."));
                    child_eb.push_player_tag(player_id);
                    child_eb.push_team_tag(player_team_id);
                    // Decided to hard-code the fifth base uuid under the "anything that can be
                    // easily deduced should not be stored" principle.
                    child_eb.push_metadata_uuid("itemId", uuid::uuid!("eecc9bf3-96b5-4ea9-9a4a-05f0a0d586f0"));
                    child_eb.push_metadata_str("itemName", "The Fifth Base");
                    child_eb.push_metadata_str_vec("mods", vec!["SUPERWANDERER".to_string()]);
                    child_eb.push_metadata_f64("playerItemRatingAfter", player_item_rating_after);
                    child_eb.push_metadata_f64("playerItemRatingBefore", player_item_rating_before);
                    child_eb.push_metadata_f64("playerRating", player_rating);

                    child_eb.build(EventType::PlayerLostItem)
                });

                eb.push_child(stadium_gained_mod_event, |mut child_eb| {
                    child_eb.push_description(format!("{player_name} placed The Fifth Base in {stadium_name}."));
                    // Base is always placed in the home team's stadium
                    child_eb.push_team_tag(home_team_id);
                    child_eb.push_metadata_str("mod", "EXTRA_BASE");
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);

                    child_eb.build(EventType::AddedMod)
                });

                eb.build(EventType::StolenBase)
            }
            FedEventData::EventHorizonActivates { game, num_unruns, away_team_nickname, } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Event Horizon activates!");
                eb.push_description(format!("It generates {num_unruns} Unruns for the {away_team_nickname}'s next game."));
                eb.build(EventType::EventHorizonActivation)
            }
            FedEventData::RenovationRatified { renovation_name, renovation_id, mod_id, mod_removals } => {
                let mut events = Vec::new();

                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{renovation_name} was Ratified into Non-Physical Law."));
                eb.push_metadata_str("id", renovation_id);
                eb.push_metadata_str("mod", &mod_id);
                eb.push_metadata_str("title", renovation_name);

                for mod_removal in mod_removals {
                    let mut child_eb = eb.connected_event(mod_removal.sub_event);
                    child_eb.set_category(EventCategory::Changes);
                    child_eb.set_description(mod_removal.description);
                    child_eb.push_team_tag(mod_removal.team_id);
                    child_eb.push_metadata_str("mod", &mod_id);
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    events.push(child_eb.build(EventType::RemovedMod));
                }

                events.insert(0, eb.build(EventType::Ratification));
                return events;
            }
            FedEventData::RunStolenThroughTunnels { game, thieving_player_name, thieving_player_id, victim_team_nickname, details, balloons, shame, free_refill } => {
                let home_team_id = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{thieving_player_name} entered the Tunnels..."));
                eb.push_description(format!("{thieving_player_name} stole a Run from the {victim_team_nickname}!"));
                eb.push_free_refill(free_refill);
                eb.push_shame(&shame, home_team_id);
                eb.push_unknown_number_of_balloons(balloons.as_ref());
                eb.push_player_tag(thieving_player_id);

                match details {
                    RunStolenThroughTunnelsDetails::NeitherKnown => {}
                    RunStolenThroughTunnelsDetails::VictimKnown { victim_team_id, away_emoji, away_score, home_emoji, home_score, run_lost_sub_event } => {
                        eb.push_child(run_lost_sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Game);
                            child_eb.push_team_tag(victim_team_id);
                            child_eb.push_description(format!("The {victim_team_nickname} scored!"));
                            child_eb.push_metadata_str("awayEmoji", &away_emoji);
                            child_eb.push_metadata_i64_or_f64("awayScore", away_score);
                            child_eb.push_metadata_str("homeEmoji", &home_emoji);
                            child_eb.push_metadata_i64_or_f64("homeScore", home_score);
                            child_eb.push_metadata_str("update", "");
                            child_eb.push_metadata_str("ledger", "");
                            child_eb.build(EventType::RunsScored)
                        });
                    }
                    RunStolenThroughTunnelsDetails::ThiefKnown { thieving_team_nickname, thieving_team_id, away_emoji, away_score, home_emoji, home_score, run_gained_sub_event } => {
                        eb.push_child(run_gained_sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Game);
                            child_eb.push_team_tag(thieving_team_id);
                            child_eb.push_description(format!("The {thieving_team_nickname} scored!"));
                            child_eb.push_metadata_str("awayEmoji", &away_emoji);
                            child_eb.push_metadata_i64_or_f64("awayScore", away_score);
                            child_eb.push_metadata_str("homeEmoji", &home_emoji);
                            child_eb.push_metadata_i64_or_f64("homeScore", home_score);
                            child_eb.push_metadata_str("update", "");
                            child_eb.push_metadata_str("ledger", "");
                            child_eb.build(EventType::RunsScored)
                        });
                    }
                    RunStolenThroughTunnelsDetails::BothKnown { victim_team_id, thieving_team_nickname, thieving_team_id, away_emoji, away_score, home_emoji, home_score, run_gained_sub_event, run_lost_sub_event, victim_event_first } => {
                        let order = if victim_event_first {
                            [
                                (run_lost_sub_event, victim_team_id, victim_team_nickname),
                                (run_gained_sub_event, thieving_team_id, thieving_team_nickname),
                            ]
                        } else {
                            [
                                (run_gained_sub_event, thieving_team_id, thieving_team_nickname),
                                (run_lost_sub_event, victim_team_id, victim_team_nickname),
                            ]
                        };

                        for (sub_event, team_id, team_nickname) in order {
                            eb.push_child(sub_event, |mut child_eb| {
                                child_eb.set_category(EventCategory::Game);
                                child_eb.push_team_tag(team_id);
                                child_eb.push_description(format!("The {team_nickname} scored!"));
                                child_eb.push_metadata_str("awayEmoji", &away_emoji);
                                child_eb.push_metadata_i64_or_f64("awayScore", away_score);
                                child_eb.push_metadata_str("homeEmoji", &home_emoji);
                                child_eb.push_metadata_i64_or_f64("homeScore", home_score);
                                child_eb.push_metadata_str("update", "");
                                child_eb.push_metadata_str("ledger", "");
                                child_eb.build(EventType::RunsScored)
                            });
                        }
                    }
                }

                eb.build(EventType::TunnelsUsed)
            },
            FedEventData::CaughtStealingItemWithTunnels { game, thief_id, thief_name, victim_id, victim_name, item_name, caught_stealing_item_sub_event, fled_elsewhere_sub_event, flipped_negative } => {
                let home_team = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{thief_name} entered the Tunnels..."));
                eb.push_description(format!("{} {item_name} caught their eye...", Possessive(&victim_name)));
                eb.push_description("...but they were caught!");
                eb.push_description(format!("{thief_name} fled Elsewhere to escape."));
                eb.push_player_tag(thief_id);

                let description = eb.description().to_string();
                eb.push_child(caught_stealing_item_sub_event, move |mut child_eb| {
                    child_eb.set_description(description);
                    child_eb.set_category(EventCategory::Outcomes);
                    child_eb.push_player_tag(thief_id);
                    child_eb.push_player_tag(victim_id);
                    child_eb.build(EventType::FailedTunnelsSteal)
                });

                if let Some(sub_event) = fled_elsewhere_sub_event {
                    // TODO Figure out what causes this to sometimes not exist and document it.
                    //   Maybe players can steal from elsewhere and when that happens there's no
                    //   event for sending them elsewhere because they're already there?
                    //   Discord diving suggests this is the case. Not sure if that's enough for me
                    //   to put it in the documentation though.
                    eb.push_child(sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{thief_name} fled Elsewhere to escape being caught in a Grand Heist."));
                        child_eb.push_team_tag(home_team);
                        child_eb.push_player_tag(thief_id);
                        child_eb.push_metadata_str("mod", "ELSEWHERE");
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.build(EventType::AddedMod)
                    });
                }

                eb.push_flipped_negative_opt(flipped_negative.as_ref(), &thief_name, thief_id, home_team);

                eb.build(EventType::TunnelsUsed)
            },
            FedEventData::StoleItemWithTunnels { game, thief_id, thief_name, victim_id, victim_name, victim_team_id, item_id, item_name, item_mods, thief_item_rating_before, thief_item_rating_after, thief_rating, victim_item_rating_before, victim_item_rating_after, victim_rating, stole_item_sub_event, item_lost_sub_event, thief_item_dropped, item_gained_sub_event } => {
                let home_team = game.home_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{thief_name} entered the Tunnels..."));
                eb.push_description(format!("{} {item_name} caught their eye...", Possessive(&victim_name)));
                eb.push_description(format!("{thief_name} stole {item_name}!"));
                eb.push_player_tag(thief_id);

                let description = eb.description().to_string();
                eb.push_child(stole_item_sub_event, |mut child_eb| {
                    child_eb.set_description(description);
                    child_eb.set_category(EventCategory::Outcomes);
                    child_eb.push_player_tag(thief_id);
                    child_eb.push_player_tag(victim_id);

                    child_eb.build(EventType::StoleItemFromTunnels)
                });

                // Conjecture: in season 22 they changed "stole" and "stolen" to "took" and "taken".
                // We'll see if that causes any errors
                eb.push_child(item_lost_sub_event, |mut child_eb| {
                    let verb = if (self.season, self.day) > (21, 2) { "taken" } else { "stolen" };
                    child_eb.push_description(format!("{} {item_name} was {verb} by {thief_name}!", Possessive(&victim_name)));
                    child_eb.push_player_tag(victim_id);
                    child_eb.push_team_tag(victim_team_id);

                    child_eb.push_metadata_uuid("itemId", item_id);
                    child_eb.push_metadata_str("itemName", item_name.clone());
                    child_eb.push_metadata_str_vec("mods", item_mods.clone());
                    child_eb.push_metadata_f64("playerItemRatingAfter", victim_item_rating_after);
                    child_eb.push_metadata_f64_opt("playerItemRatingBefore", victim_item_rating_before);
                    child_eb.push_metadata_f64("playerRating", victim_rating);

                    child_eb.build(EventType::PlayerLostItem)
                });

                if let Some(item_dropped) = thief_item_dropped {
                    eb.push_child(item_dropped.sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{thief_name} dropped {}.", item_dropped.item_name));
                        child_eb.push_player_tag(thief_id);
                        child_eb.push_team_tag(home_team);

                        child_eb.push_metadata_uuid("itemId", item_dropped.item_id);
                        child_eb.push_metadata_str("itemName", item_dropped.item_name.clone());
                        child_eb.push_metadata_str_vec("mods", item_dropped.item_mods.clone());
                        child_eb.push_metadata_f64("playerItemRatingAfter", item_dropped.player_item_rating_after);
                        child_eb.push_metadata_f64_opt("playerItemRatingBefore", item_dropped.player_item_rating_before);
                        child_eb.push_metadata_f64("playerRating", thief_rating);

                        child_eb.build(EventType::PlayerLostItem)
                    });
                }

                eb.push_child(item_gained_sub_event, |mut child_eb| {
                    let verb = if (self.season, self.day) > (21, 2) { "took" } else { "stole" };
                    child_eb.push_description(format!("{thief_name} {verb} {} {item_name}!", Possessive(&victim_name)));
                    child_eb.push_player_tag(thief_id);
                    child_eb.push_team_tag(home_team);

                    child_eb.push_metadata_uuid("itemId", item_id);
                    child_eb.push_metadata_str("itemName", item_name);
                    child_eb.push_metadata_str_vec("mods", item_mods);
                    child_eb.push_metadata_f64_opt("playerItemRatingAfter", thief_item_rating_after);
                    child_eb.push_metadata_f64("playerItemRatingBefore", thief_item_rating_before);
                    child_eb.push_metadata_f64("playerRating", thief_rating);

                    child_eb.build(EventType::PlayerGainedItem)
                });

                eb.build(EventType::TunnelsUsed)
            },
            FedEventData::NothingInterestingInTunnels { game, thief_id, thief_name, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("{thief_name} entered the Tunnels..."));
                eb.push_description("...but didn't find anything interesting.");
                eb.push_player_tag(thief_id);

                let description = eb.description().to_string();
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_description(description);
                    child_eb.set_category(EventCategory::Outcomes);
                    child_eb.push_player_tag(thief_id);

                    child_eb.build(EventType::FoundNothingInterestingInTunnels)
                });

                eb.build(EventType::TunnelsUsed)
            }
            FedEventData::SunSunRecharged { pressure_after } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("Sun(Sun) Recharged.");

                eb.push_metadata_f64("current", pressure_after);
                eb.push_metadata_i64("maximum", 99999);
                eb.push_metadata_i64("recharge", 26244);

                eb.build(EventType::SunSunPressure)
            }
            FedEventData::Sun30Smiles { game, away, home, balloons } => {
                let home_team_id = game.home_team;
                let away_team_id = game.away_team;
                eb.set_game(game);
                eb.set_category(EventCategory::Special);

                let home_team_nickname = match &home {
                    Either::Left(win) => { &win.team_nickname }
                    Either::Right(team_nickname) => { team_nickname }
                };

                if let Some(stadium_name) = balloons {
                    eb.push_description(format!("{stadium_name} inflates 10 Balloons!"));
                }
                eb.push_description(format!("The {} and {} reached Extra Innings.", home_team_nickname, away.team_nickname));
                eb.push_description("Sun 30 smiled upon them.");

                for (maybe_win, team_id) in [(home, home_team_id), (Either::Left(away), away_team_id)] {
                    match maybe_win {
                        Either::Left(win) => {
                            eb.push_child(win.sub_event, |mut child_eb| {
                                child_eb.set_category(EventCategory::Outcomes);
                                child_eb.push_description(format!("Sun 30 granted the {} a Win.", win.team_nickname));
                                child_eb.push_team_tag(team_id);
                                child_eb.push_metadata_i64("amount", 1);
                                child_eb.push_metadata_i64("before", win.wins_after - 1);
                                child_eb.push_metadata_i64("after", win.wins_after);
                                child_eb.push_metadata_str_vec("lines", if self.season < 23 {
                                    vec![
                                        "Sun 30: 1".to_string(),
                                        "Sun(Sun): 1 ^ 2 = 1".to_string(),
                                    ]
                                } else {
                                    Vec::new()
                                });
                                child_eb.build(if self.day < 99 { EventType::WinCollectedRegular } else { EventType::WinCollectedPostseason })
                            });
                        }
                        Either::Right(_) => {
                            eb.push_phantom_child();
                        }
                    }
                }

                eb.build(EventType::Sun30Smiles)
            }
            FedEventData::Voicemail { game, replaced_player_id, replaced_player_name, replacement_player_id, replacement_player_name, team_id, team_nickname, swap_sub_event, shadowed_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("Home Team Shutout.");
                eb.push_description("Incoming Voicemail...");
                eb.push_description(format!("{replaced_player_name} is replaced by {replacement_player_name}."));

                eb.push_player_tag(replaced_player_id);
                eb.push_child(swap_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{replaced_player_name} was replaced by an incoming Voicemail."));
                    // Voicemails always take from the lineup
                    child_eb.push_metadata_i64("aLocation", PositionType::Lineup);
                    child_eb.push_metadata_uuid("aPlayerId", replaced_player_id);
                    child_eb.push_player_tag(replaced_player_id);
                    child_eb.push_metadata_str("aPlayerName", &replaced_player_name);
                    // Voicemails always put you in the shadows (which shares an id with Bench after
                    // the unification
                    child_eb.push_metadata_i64("bLocation", PositionType::BenchOrShadows);
                    child_eb.push_metadata_uuid("bPlayerId", replacement_player_id);
                    child_eb.push_player_tag(replacement_player_id);
                    child_eb.push_metadata_str("bPlayerName", &replacement_player_name);

                    child_eb.push_metadata_uuid("teamId", team_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("teamName", &team_nickname);

                    child_eb.build(EventType::PlayerSwap)
                });

                eb.push_child(shadowed_sub_event.sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{replaced_player_name} entered the Shadows."));
                    child_eb.push_metadata_f64("before", shadowed_sub_event.rating_before);
                    child_eb.push_metadata_f64("after", shadowed_sub_event.rating_after);
                    child_eb.push_metadata_i64("type", StatChangeCategory::All);
                    child_eb.push_player_tag(replaced_player_id);
                    child_eb.push_team_tag(team_id);

                    child_eb.build(EventType::PlayerStatIncrease)
                });

                eb.push_player_tag(replacement_player_id);

                eb.build(EventType::Voicemail)
            }
            FedEventData::BadGatewayBroken { team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("SMASH");
                eb.push_team_tag(team_id);
                eb.build(EventType::BadGatewayBroken)
            }
            FedEventData::TumbleweedSounds { team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("[TUMBLEWEED SOUNDS]");
                eb.push_team_tag(team_id);
                eb.build(EventType::TumbleweedSounds)
            }
            FedEventData::IntentionalWalk { game, pitch, batter_name, batter_id, pitcher_name, pitcher_id, sensed_foul_play_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_pitch(pitch);
                eb.push_description(format!("{pitcher_name} senses foul play."));
                eb.push_description(format!("{batter_name} is intentionally walked."));
                eb.push_player_tag(batter_id);
                eb.push_player_tag(pitcher_id);
                eb.push_player_tag(batter_id);  // Yes, batter again

                if let Some(sensed_foul_play_sub_event) = sensed_foul_play_sub_event {
                    eb.push_child(sensed_foul_play_sub_event, |mut child_eb| {
                        child_eb.set_category(EventCategory::Special);
                        child_eb.push_description(format!("{pitcher_name} sensed foul play."));
                        child_eb.push_player_tag(pitcher_id);

                        child_eb.build(EventType::InvestigationMessage)
                    });
                }

                eb.build(EventType::Walk)
            }
            FedEventData::NothingToTrade { game, trader_name, trader_id, victim_id, sub_event } => {
                eb.set_game(game);
                let description = format!("{trader_name} sought out a trade, but nothing caught their eye.");
                eb.push_description(&description);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Outcomes);
                    child_eb.push_description(&description);
                    child_eb.push_player_tag(trader_id);
                    if let Some(victim_id) = victim_id {
                        child_eb.push_player_tag(victim_id);
                    }

                    child_eb.build(EventType::TradeFailed)
                });

                eb.build(EventType::Trade)
            }
            FedEventData::Trade { game, trader_traitor, taken_item_name, taken_item_id, victim_mods_lost, trader_mods_gained, trader_name, trader_id, trader_item_rating_before, trader_item_rating_after, trader_rating, trader_item_change_sub_event, victim_name, victim_id, victim_item_rating_before, victim_item_rating_after, victim_rating, victim_item_change_sub_event } => {
                eb.set_game(game);

                let punct = if self.season > 22 || (self.season == 22 && self.day == 116) {
                    "!"
                } else {
                    "."
                };
                let description = match &trader_traitor {
                    TraderTraitor::Trader(TradeForSomething { donated_item_name, ..}) |
                    TraderTraitor::Traitor(TradeForSomething { donated_item_name, ..}) |
                    TraderTraitor::Unknown(TradeForSomething { donated_item_name, ..}) => {
                        format!("{trader_traitor}{trader_name} traded their {donated_item_name} for {} {taken_item_name}{punct}", Possessive(&victim_name))
                    }
                    TraderTraitor::Neither(TradeForNothing { .. }) => {
                        format!("{trader_name} traded their nothing for {} {taken_item_name}!", Possessive(&victim_name))
                    }
                };
                eb.push_description(&description);

                let trader_traitor_label = trader_traitor.to_string();
                // The trade-for-nothing event (there only ever was one) has its sub-events in a
                // different order and with slightly different info / metadata keys
                match trader_traitor {
                    TraderTraitor::Trader(TradeForSomething { donated_item_name, donated_item_id, trader_mods_lost, victim_mods_gained }) |
                    TraderTraitor::Traitor(TradeForSomething { donated_item_name, donated_item_id, trader_mods_lost, victim_mods_gained }) |
                    TraderTraitor::Unknown(TradeForSomething { donated_item_name, donated_item_id, trader_mods_lost, victim_mods_gained }) => {
                        eb.push_child(trader_item_change_sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(&description);
                            child_eb.push_player_tag(trader_id);
                            // This event has no team tag, although it probably should
                            child_eb.push_metadata_uuid("itemTradedId", donated_item_id);
                            child_eb.push_metadata_str("itemTradedName", &donated_item_name);
                            child_eb.push_metadata_uuid("itemReceivedId", taken_item_id);
                            child_eb.push_metadata_str("itemReceivedName", &taken_item_name);
                            child_eb.push_metadata_str_vec("modsGained", trader_mods_gained);
                            child_eb.push_metadata_str_vec("modsLost", trader_mods_lost);
                            child_eb.push_metadata_f64_opt("playerItemRatingAfter", trader_item_rating_after);
                            child_eb.push_metadata_f64_opt("playerItemRatingBefore", trader_item_rating_before);
                            child_eb.push_metadata_f64("playerRating", trader_rating);
                            child_eb.build(EventType::ItemTraded)
                        });

                        eb.push_child(victim_item_change_sub_event, move |mut child_eb| {
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(format!("{victim_name} traded their {taken_item_name} for {trader_traitor_label}{} {donated_item_name}{punct}", Possessive(&trader_name)));
                            child_eb.push_player_tag(victim_id);
                            // This event has no team tag, even though it probably should
                            child_eb.push_metadata_uuid("itemTradedId", taken_item_id);
                            child_eb.push_metadata_str("itemTradedName", &taken_item_name);
                            child_eb.push_metadata_uuid("itemReceivedId", donated_item_id);
                            child_eb.push_metadata_str("itemReceivedName", donated_item_name);
                            child_eb.push_metadata_str_vec("modsGained", victim_mods_gained);
                            child_eb.push_metadata_str_vec("modsLost", victim_mods_lost);
                            child_eb.push_metadata_f64_opt("playerItemRatingAfter", victim_item_rating_after);
                            child_eb.push_metadata_f64_opt("playerItemRatingBefore", victim_item_rating_before);
                            child_eb.push_metadata_f64("playerRating", victim_rating);
                            child_eb.build(EventType::ItemTraded)
                        });
                    }
                    TraderTraitor::Neither(TradeForNothing { victim_team_id, trader_team_id }) => {
                        eb.push_child(victim_item_change_sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(format!("{victim_name} traded away {taken_item_name} to {trader_name} for nothing!"));
                            child_eb.push_team_tag(victim_team_id);
                            child_eb.push_player_tag(victim_id);
                            // This event has no team tag, even though it probably should
                            child_eb.push_metadata_uuid("itemId", taken_item_id);
                            child_eb.push_metadata_str("itemName", &taken_item_name);
                            child_eb.push_metadata_str_vec("mods", victim_mods_lost);
                            child_eb.push_metadata_f64_opt("playerItemRatingAfter", victim_item_rating_after);
                            child_eb.push_metadata_f64_opt("playerItemRatingBefore", victim_item_rating_before);
                            child_eb.push_metadata_f64("playerRating", victim_rating);
                            child_eb.build(EventType::PlayerLostItem)
                        });

                        eb.push_child(trader_item_change_sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Changes);
                            child_eb.push_description(&description);
                            child_eb.push_team_tag(trader_team_id);
                            child_eb.push_player_tag(trader_id);
                            child_eb.push_metadata_uuid("itemId", taken_item_id);
                            child_eb.push_metadata_str("itemName", &taken_item_name);
                            child_eb.push_metadata_str_vec("mods", trader_mods_gained);
                            child_eb.push_metadata_f64_opt("playerItemRatingAfter", trader_item_rating_after);
                            child_eb.push_metadata_f64_opt("playerItemRatingBefore", trader_item_rating_before);
                            child_eb.push_metadata_f64("playerRating", trader_rating);
                            child_eb.build(EventType::PlayerGainedItem)
                        });

                    }
                }

                eb.build(EventType::Trade)
            }
            FedEventData::NothingToOffer { game, trader_name, trader_id, victim_name, victim_id, sub_event } => {
                eb.set_game(game);
                let description = format!("{trader_name} tried to trade with {victim_name} but they had nothing to offer.");
                eb.push_description(&description);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Outcomes);
                    child_eb.push_description(&description);
                    child_eb.push_player_tag(trader_id);
                    child_eb.push_player_tag(victim_id);

                    child_eb.build(EventType::TradeFailed)
                });

                eb.build(EventType::Trade)
            }
            FedEventData::RoamFailed { player_name, player_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("Roam failed.");
                eb.push_description(format!("{player_name} was gripped by Force."));
                eb.push_player_tag(player_id);
                eb.build(EventType::PlayerMoveFailedForce)
            }
            FedEventData::ThievesGuildStolePlayer { game, thieving_team_id, thieving_team_nickname, thieving_team_stadium_name, victim_team_id, victim_team_nickname, stolen_player_id, stolen_player_name, player_moved_teams_sub_event, player_shadows_boost } => {
                eb.set_game(game);
                eb.push_player_tag(stolen_player_id);
                eb.push_description(format!("{thieving_team_stadium_name} Thieves' Guild convened."));
                eb.push_description(format!("They stole {} Shadows player {stolen_player_name}!", Possessive(&victim_team_nickname)));

                eb.push_child(player_moved_teams_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("The {victim_team_nickname} sent a player to the {thieving_team_nickname}."));
                    child_eb.push_player_tag(stolen_player_id);
                    child_eb.push_team_tag(victim_team_id);
                    child_eb.push_team_tag(thieving_team_id);

                    child_eb.push_metadata_i64("location", 2  /* Shadows */);
                    child_eb.push_metadata_i64("receiveLocation", 2 /* Shadows */);
                    child_eb.push_metadata_uuid("playerId", stolen_player_id);
                    child_eb.push_metadata_str("playerName", &stolen_player_name);
                    child_eb.push_metadata_uuid("receiveTeamId", thieving_team_id);
                    child_eb.push_metadata_str("receiveTeamName", &thieving_team_nickname);
                    child_eb.push_metadata_uuid("sendTeamId", victim_team_id);
                    child_eb.push_metadata_str("sendTeamName", victim_team_nickname);

                    child_eb.build(EventType::PlayerMoved)
                });

                eb.push_child(player_shadows_boost.sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{stolen_player_name} entered the Shadows."));
                    child_eb.push_player_tag(stolen_player_id);
                    child_eb.push_team_tag(thieving_team_id);

                    child_eb.build_boost(&player_shadows_boost)
                });

                eb.build(EventType::ThievesGuildStolePlayer)
            }
            // TODO Why is thieving_team_nickname unused?
            FedEventData::ThievesGuildStoleItem { game, thieving_team_nickname: _, thieving_team_stadium_name, beneficiary_player_name, beneficiary_gained_item, victim_team_id, victim_team_nickname, victim_player_id, victim_player_name, victim_lost_item } => {
                eb.set_game(game);
                eb.push_player_tag(beneficiary_gained_item.player_id);
                eb.push_player_tag(victim_player_id);
                eb.push_description(format!("{thieving_team_stadium_name} Thieves' Guild convened."));
                eb.push_description(format!("They stole {} from {} Shadows player {victim_player_name} and gave it to {beneficiary_player_name}.", beneficiary_gained_item.item_name, Possessive(&victim_team_nickname)));

                // Almost, but not quite, reusable from the above
                let stolen_statement = format!("{thieving_team_stadium_name} Thieves' Guild stole {} from {victim_player_name} and give it to {beneficiary_player_name}.", beneficiary_gained_item.item_name);

                // The borrow checker wants this, but it also makes our format strings look nicer
                let item_name = &beneficiary_gained_item.item_name;

                eb.push_child(victim_lost_item.sub_event, |mut child_eb| {
                    // The missing space after the stolen_statement is game-accurate
                    child_eb.push_description(format!("{stolen_statement}{} {item_name} was taken by {beneficiary_player_name}!", Possessive(&victim_player_name)));
                    child_eb.push_player_tag(victim_player_id);
                    child_eb.push_team_tag(victim_team_id);

                    child_eb.push_metadata_uuid("itemId", beneficiary_gained_item.item_id);
                    child_eb.push_metadata_str("itemName", item_name);
                    child_eb.push_metadata_str_vec("mods", victim_lost_item.item_mods);
                    child_eb.push_metadata_f64("playerItemRatingAfter", victim_lost_item.player_item_rating_after);
                    child_eb.push_metadata_f64("playerItemRatingBefore", victim_lost_item.player_item_rating_before);
                    child_eb.push_metadata_f64("playerRating", victim_lost_item.player_rating);

                    child_eb.build(EventType::PlayerLostItem)
                });

                if let Some(item_dropped) = beneficiary_gained_item.dropped_item {
                    eb.push_dropped_item(&beneficiary_player_name, beneficiary_gained_item.player_id, beneficiary_gained_item.team_id, beneficiary_gained_item.player_rating, item_dropped);
                }

                // This is just different enough to not use eb.push_gained_item
                eb.push_child(beneficiary_gained_item.sub_event, |mut child_eb| {
                    // The missing space after the stolen_statement is game-accurate
                    child_eb.push_description(format!("{stolen_statement}{beneficiary_player_name} took {} {item_name}!", Possessive(&victim_player_name)));
                    child_eb.push_player_tag(beneficiary_gained_item.player_id);
                    child_eb.push_team_tag(beneficiary_gained_item.team_id);

                    child_eb.push_metadata_uuid("itemId", beneficiary_gained_item.item_id);
                    child_eb.push_metadata_str("itemName", item_name);
                    child_eb.push_metadata_str_vec("mods", beneficiary_gained_item.item_mods);
                    child_eb.push_metadata_f64_opt("playerItemRatingAfter", beneficiary_gained_item.player_item_rating_after);
                    child_eb.push_metadata_f64("playerItemRatingBefore", beneficiary_gained_item.player_item_rating_before);
                    child_eb.push_metadata_f64("playerRating", beneficiary_gained_item.player_rating);

                    child_eb.build(EventType::PlayerGainedItem)
                });

                eb.build(EventType::ThievesGuildStoleItem)
            },
            FedEventData::RiffOpened { game, riff, new_weather } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("A Riff Opened.");
                eb.push_description(format!("🎵 {} {} 🎵", riff.iter().map(RiffElement::as_ref).join(" "), new_weather.to_str(self.season, self.day)));
                eb.build(EventType::RiffOpened)
            }
            FedEventData::BandBeginsToPlay { game, numbers_went, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("The Polarity shifted!");
                eb.push_description("The Band began to play.");

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description("The Polarity shifted!");
                    child_eb.push_description("The Band began to play.");
                    // It's always a transition from Polarity to Jazz
                    child_eb.push_metadata_i64("before", match numbers_went {
                        NumbersGo::Up => { Weather::PolarityPlus }
                        NumbersGo::Down => { Weather::PolarityMinus }
                    });
                    child_eb.push_metadata_i64("after", Weather::Jazz);
                    child_eb.build(EventType::WeatherChange)
                });

                eb.build(EventType::PolarityShift)
            }
            FedEventData::BasesReloaded { game, player_name, player_id } => {
                eb.set_game(game);
                eb.push_description(format!("{player_name} Reloaded all of the Bases!"));
                eb.push_player_tag(player_id);
                eb.build(EventType::BasesReloaded)
            }
            FedEventData::NightShift { game, team_id, shadowed_player_id, shadowed_player_name, unshadowed_player_id, unshadowed_player_name, outcome, night_shift_boost_sub_event } => {
                eb.set_game(game);
                eb.push_description("Night Shift.");
                eb.push_description(format!("Deep Darkness took {shadowed_player_name}."));
                eb.push_description(format!("{unshadowed_player_name} clocked in."));

                match outcome {
                    NightShiftOutcome::PlayersSwapped { team_nickname, active_location, player_swap_sub_event, player_shadowed_sub_event } => {
                        eb.push_child(player_swap_sub_event, |mut child_eb| {
                            child_eb.push_description(&format!("The {team_nickname} swapped two players on their roster."));
                            child_eb.push_player_tag(shadowed_player_id);
                            child_eb.push_player_tag(unshadowed_player_id);
                            child_eb.push_team_tag(team_id);
                            child_eb.push_metadata_i64("aLocation", active_location);
                            child_eb.push_metadata_uuid("aPlayerId", shadowed_player_id);
                            child_eb.push_metadata_str("aPlayerName", &shadowed_player_name);
                            child_eb.push_metadata_i64("bLocation", PositionType::BenchOrShadows);
                            child_eb.push_metadata_uuid("bPlayerId", unshadowed_player_id);
                            child_eb.push_metadata_str("bPlayerName", &unshadowed_player_name);
                            child_eb.push_metadata_uuid("teamId", team_id);
                            child_eb.push_metadata_str("teamName", &team_nickname);
                            child_eb.build(EventType::PlayerSwap)
                        });

                        eb.push_child(player_shadowed_sub_event.sub_event, |mut child_eb| {
                            child_eb.push_description(format!("{shadowed_player_name} entered the Shadows."));
                            child_eb.push_player_tag(shadowed_player_id);
                            child_eb.push_team_tag(team_id);
                            child_eb.build_boost(&player_shadowed_sub_event)
                        });
                    }
                    NightShiftOutcome::BooksCooked { books_cooked_sub_event, gained_unstable_sub_event } => {
                        eb.push_child(books_cooked_sub_event, |mut child_eb| {
                            child_eb.push_description("WARNING");
                            child_eb.push_description("EXTRAPLANAR ACTIVITY");
                            child_eb.push_description("BOOKS COOKED");
                            child_eb.push_description("INSTABILITY DETECTED");
                            child_eb.push_player_tag(shadowed_player_id);
                            child_eb.build(EventType::NecromancyOrPlunderNarration)
                        });

                        eb.push_child(gained_unstable_sub_event, |mut child_eb| {
                            child_eb.set_description(format!("{shadowed_player_name} gained the Unstable mod."));
                            child_eb.push_team_tag(team_id);
                            child_eb.push_player_tag(shadowed_player_id);
                            child_eb.push_metadata_str("mod", "MARKED");
                            // Permanent! oh shit
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                            child_eb.build(EventType::AddedMod)
                        });
                    }
                }

                eb.push_child(night_shift_boost_sub_event.sub_event, |mut child_eb| {
                    child_eb.push_description(format!("{unshadowed_player_name} clocked in."));
                    child_eb.push_player_tag(unshadowed_player_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.build_boost(&night_shift_boost_sub_event)
                });

                eb.build(EventType::NightShift)
            }
            FedEventData::TeamFormed { team_id, team_name, team_nickname, rotation_players, lineup_players, shadows_players } => {
                let mut events = Vec::new();

                // These events precede all the position events
                for position in [&rotation_players, &lineup_players, &shadows_players] {
                    for player in &position.players {
                        match &player.player_moved_from {
                            PlayerMovedFrom::Unspecified => { /* No events */}
                            PlayerMovedFrom::LeagueTeam { former_team_id, former_team_nickname, sub_event } => {
                                let mut cut_from_team_eb = eb.connected_event(*sub_event);
                                cut_from_team_eb.set_category(EventCategory::Changes);
                                cut_from_team_eb.push_description(format!("The {former_team_nickname} cut a player from their roster."));
                                cut_from_team_eb.push_player_tag(player.player_id);
                                cut_from_team_eb.push_team_tag(*former_team_id);

                                cut_from_team_eb.push_metadata_uuid("playerId", player.player_id);
                                cut_from_team_eb.push_metadata_str("playerName", &player.player_name);
                                cut_from_team_eb.push_metadata_uuid("teamId", *former_team_id);
                                cut_from_team_eb.push_metadata_str("teamName", former_team_nickname);

                                events.push(cut_from_team_eb.build(EventType::PlayerRemovedFromTeam));
                            }
                            PlayerMovedFrom::IncineratedTeam { former_team_id, former_team_nickname, pulled_from_team_sub_event, exited_hall_sub_event, gained_returned_sub_event } => {
                                let mut pulled_from_team_eb = eb.connected_event(*pulled_from_team_sub_event);
                                pulled_from_team_eb.set_category(EventCategory::Changes);
                                pulled_from_team_eb.push_description(format!("{} was pulled from the incinerated {former_team_nickname}.", player.player_name));
                                pulled_from_team_eb.push_player_tag(player.player_id);
                                pulled_from_team_eb.push_team_tag(*former_team_id);

                                pulled_from_team_eb.push_metadata_uuid("playerId", player.player_id);
                                pulled_from_team_eb.push_metadata_str("playerName", &player.player_name);
                                pulled_from_team_eb.push_metadata_uuid("teamId", *former_team_id);
                                pulled_from_team_eb.push_metadata_str("teamName", former_team_nickname);

                                events.push(pulled_from_team_eb.build(EventType::PlayerRemovedFromTeam));

                                let mut exited_hall_eb = eb.connected_event(*exited_hall_sub_event);
                                exited_hall_eb.set_category(EventCategory::Changes);
                                exited_hall_eb.push_description(format!("{} exited the Hall of Flame", player.player_name));
                                exited_hall_eb.push_player_tag(player.player_id);

                                events.push(exited_hall_eb.build(EventType::ExitHallOfFlame));

                                let mut gained_returned_eb = eb.connected_event(*gained_returned_sub_event);
                                gained_returned_eb.set_category(EventCategory::Changes);
                                gained_returned_eb.push_description(format!("{} gained the Returned mod.", player.player_name));
                                gained_returned_eb.push_player_tag(player.player_id);
                                gained_returned_eb.push_team_tag(*former_team_id);
                                gained_returned_eb.push_metadata_str("mod", "RETURNED");
                                gained_returned_eb.push_metadata_i64("type", ModDuration::Permanent);

                                events.push(gained_returned_eb.build(EventType::AddedMod));
                            }
                            PlayerMovedFrom::OtherTeam { former_team_id, former_team_nickname, sub_event } => {
                                let mut cut_from_team_eb = eb.connected_event(*sub_event);
                                cut_from_team_eb.set_category(EventCategory::Changes);
                                cut_from_team_eb.push_description(format!("{} was Collected.", player.player_name));
                                cut_from_team_eb.push_player_tag(player.player_id);
                                cut_from_team_eb.push_team_tag(*former_team_id);

                                cut_from_team_eb.push_metadata_uuid("playerId", player.player_id);
                                cut_from_team_eb.push_metadata_str("playerName", &player.player_name);
                                cut_from_team_eb.push_metadata_uuid("teamId", *former_team_id);
                                cut_from_team_eb.push_metadata_str("teamName", former_team_nickname);

                                events.push(cut_from_team_eb.build(EventType::PlayerRemovedFromTeam));
                            }
                            PlayerMovedFrom::HallOfFlame { former_team_id, exited_hall_sub_event, gained_returned_sub_event } => {
                                let mut exited_hall_eb = eb.connected_event(*exited_hall_sub_event);
                                exited_hall_eb.set_category(EventCategory::Changes);
                                exited_hall_eb.push_description(format!("{} exited the Hall of Flame", player.player_name));
                                exited_hall_eb.push_player_tag(player.player_id);

                                events.push(exited_hall_eb.build(EventType::ExitHallOfFlame));

                                let mut gained_returned_eb = eb.connected_event(*gained_returned_sub_event);
                                gained_returned_eb.set_category(EventCategory::Changes);
                                gained_returned_eb.push_description(format!("{} gained the Returned mod.", player.player_name));
                                gained_returned_eb.push_player_tag(player.player_id);
                                if let Some(team_id) = former_team_id {
                                    gained_returned_eb.push_team_tag(*team_id);
                                }
                                gained_returned_eb.push_metadata_str("mod", "RETURNED");
                                gained_returned_eb.push_metadata_i64("type", ModDuration::Permanent);

                                events.push(gained_returned_eb.build(EventType::AddedMod));
                            }
                        }

                        if let Some(sub_event) = player.player_visited_vault {
                            let mut visited_vault_eb = eb.connected_event(sub_event);
                            visited_vault_eb.set_category(EventCategory::Changes);
                            visited_vault_eb.push_description(format!("{} visited the Vault.", &player.player_name));
                            visited_vault_eb.push_player_tag(player.player_id);

                            events.push(visited_vault_eb.build(EventType::PlayerEnteredVault));
                        }
                    }
                }

                // These events are in a big block and not in order, so they need to be collected,
                // ordered properly, and then added to `events`.
                let mut shuffled_events = Vec::new();

                let team_nickname_ref = &team_nickname;
                let mut push_events_for_position = |players: PlayersAddedToTeam, position: PositionType| {
                    let mut position_eb = eb.connected_event(players.sub_event);

                    // The only time the message said "Replicas" instead of "Players" was for the
                    // Vault Legends' shadows, which was formed by taking all Dusted Replicas. That
                    // can be detected by looking for the undusting message, but we can't know
                    // exactly what criteria would have caused the game to say "Replicas".
                    let who_entered = if players.players.iter().all(|p| p.replica_dusted_off.is_some()) {
                        "Replicas"
                    } else {
                        "Players"
                    };

                    position_eb.set_category(EventCategory::Changes);
                    position_eb.push_description(format!("{} {who_entered} entered The {team_nickname}' {}.", players.players.len(), position.name_post_merge()));
                    position_eb.push_team_tag(team_id);
                    for player in &players.players {
                        position_eb.push_player_tag(player.player_id);
                    }
                    position_eb.push_metadata_i64("location", position);
                    position_eb.push_metadata_uuid_vec("playerIds", players.players.iter()
                        .map(|player| &player.player_id)
                    );
                    position_eb.push_metadata_str_vec("playerNames", players.players.iter()
                        // This clone could be avoided with some reordering, but the reordering
                        // itself would require allocations, so this is probably better
                        .map(|player| player.player_name.clone())
                        .collect()
                    );
                    position_eb.push_metadata_uuid("teamId", team_id);
                    position_eb.push_metadata_str("teamName", team_nickname_ref);

                    let ev = position_eb.build(EventType::PlayersAddedToTeam);
                    events.push(ev);

                    for player in &players.players {
                        if let Some(on_an_odyssey) = &player.odyssey_boost {
                            let mut odyssey_eb = eb.connected_event(on_an_odyssey.sub_event);
                            odyssey_eb.set_category(EventCategory::Changes);
                            odyssey_eb.push_player_tag(player.player_id);
                            odyssey_eb.push_team_tag(team_id);
                            odyssey_eb.push_description(format!("{} was boosted.", player.player_name));
                            events.push(odyssey_eb.build_boost(&on_an_odyssey))
                        }

                        if let Some((boost_event, order)) = &player.shadow_boost {
                            let mut successor_eb = eb.connected_event(boost_event.sub_event);
                            successor_eb.set_category(EventCategory::Changes);
                            successor_eb.push_player_tag(player.player_id);
                            successor_eb.push_team_tag(team_id);
                            successor_eb.push_description(format!("{} entered the Shadows.", player.player_name));
                            shuffled_events.push((*order, successor_eb.build_boost(&boost_event)));
                        }

                        if let Some((dusted_off_event, order)) = player.replica_dusted_off {
                            let mut successor_eb = eb.connected_event(dusted_off_event);
                            successor_eb.set_category(EventCategory::Changes);
                            successor_eb.push_player_tag(player.player_id);
                            successor_eb.push_team_tag(team_id);
                            successor_eb.push_description(format!("{} dusts off.", player.player_name));
                            successor_eb.push_metadata_str("mod", "DUST");
                            successor_eb.push_metadata_i64("type", ModDuration::Permanent);
                            shuffled_events.push((order, successor_eb.build(EventType::RemovedMod)));
                        }

                        if let Some((yolked_removed_event, order)) = player.yolked_removed {
                            let mut successor_eb = eb.connected_event(yolked_removed_event);
                            successor_eb.set_category(EventCategory::Changes);
                            successor_eb.push_player_tag(player.player_id);
                            successor_eb.push_team_tag(team_id);
                            successor_eb.push_description(format!("{} is weaker on their own.", player.player_name));
                            successor_eb.push_metadata_str("mod", "YOLKED");
                            successor_eb.push_metadata_str("source", "HARD_BOILED");
                            successor_eb.push_metadata_i64("type", ModDuration::Permanent);
                            shuffled_events.push((order, successor_eb.build(EventType::RemovedModFromOtherMod)));
                        }
                    }

                    if let Some(togetherness) = players.stronger_together {
                        let mut togetherness_eb = eb.connected_event(togetherness.sub_event);
                        togetherness_eb.set_category(EventCategory::Changes);
                        togetherness_eb.push_team_tag(team_id);
                        for player in &togetherness.players {
                            togetherness_eb.push_player_tag(player.player_id);
                        }

                        let (first, rest) = togetherness.players.split_first()
                            .expect("This event should never have an empty list of names");
                        let description = iter::once(&first.player_name)
                            .chain(&togetherness.extra_player_names)
                            .chain(rest.iter().map(|p| &p.player_name))
                            .with_position()
                            .flat_map(|(position, name)| {
                                match position {
                                    Position::First | Position::Only => ["", name.as_str()],
                                    Position::Middle => [", ", name.as_str()],
                                    Position::Last =>  [", and ", name.as_str()],
                                }
                            })
                            .chain(iter::once(" are stronger together."))
                            .join("");

                        togetherness_eb.push_description(description);
                        togetherness_eb.push_metadata_str("mod", "YOLKED");
                        togetherness_eb.push_metadata_str("source", "HARD_BOILED");
                        togetherness_eb.push_metadata_i64("type", ModDuration::Permanent);
                        events.push(togetherness_eb.build(EventType::AddedModFromOtherMod))
                    }
                };

                push_events_for_position(rotation_players, PositionType::Rotation);
                push_events_for_position(lineup_players, PositionType::Lineup);
                push_events_for_position(shadows_players, PositionType::BenchOrShadows);

                // Now is the time to sort the shuffled events and add them
                shuffled_events.sort_unstable_by_key(|(order, _)| *order);
                events.extend(shuffled_events.into_iter().map(|(_, event)| event));

                eb.set_category(EventCategory::Changes);
                eb.push_description(&format!("The {team_name} formed."));
                eb.push_team_tag(team_id);
                eb.push_metadata_uuid("id", team_id);

                events.insert(0, eb.build(EventType::TeamFormed));
                return events;
            },
            FedEventData::WeatherReport { game, original_season, original_season_tagline_all_caps, weather_report, weather_before, weather_after, sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description("A new Weather Report arrived from History.");
                eb.push_description(format!("SEASON {original_season}: {original_season_tagline_all_caps}"));
                eb.push_description("");
                eb.push_description(&weather_report);

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(weather_report);
                    child_eb.push_metadata_i64("before", weather_before);
                    child_eb.push_metadata_i64("after", weather_after);
                    child_eb.build(EventType::WeatherChange)
                });

                eb.build(EventType::WeatherReport)
            },
            FedEventData::TeamTunnelHeistBegins { game, thieving_team_id, thieving_team_nickname, target_team_id, target_player_id, target_player_name, sub_event, player_collected } => {
                eb.set_game(game);
                eb.push_description(format!("The {thieving_team_nickname} attempted a Heist..."));

                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Outcomes);
                    if player_collected.is_some() {
                        child_eb.push_description(format!("The {thieving_team_nickname} collected {target_player_name} in a Heist!"));
                    } else {
                        child_eb.push_description(format!("The {thieving_team_nickname} attempted a Heist..."));
                        child_eb.push_description(format!("...but {target_player_name} evaded them!"));
                    }
                    child_eb.push_player_tag(target_player_id);
                    child_eb.push_team_tag(thieving_team_id);
                    child_eb.push_team_tag(target_team_id);
                    if player_collected.is_some() {
                        child_eb.build(EventType::StoleItemFromTunnels)
                    } else {
                        child_eb.build(EventType::FailedTunnelsSteal)
                    }
                });

                if let Some(player_collected) = player_collected {
                    eb.push_child(player_collected.player_collected_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("The {thieving_team_nickname} collected {target_player_name}!"));
                        child_eb.push_player_tag(target_player_id);
                        child_eb.push_team_tag(target_team_id);
                        child_eb.push_team_tag(thieving_team_id);

                        child_eb.push_metadata_i64("location", player_collected.location);
                        child_eb.push_metadata_uuid("playerId", target_player_id);
                        child_eb.push_metadata_str("playerName", &target_player_name);
                        child_eb.push_metadata_uuid("sendTeamId", target_team_id);
                        child_eb.push_metadata_str("sendTeamName", player_collected.target_team_nickname);
                        child_eb.push_metadata_i64("receiveLocation", player_collected.location);
                        child_eb.push_metadata_uuid("receiveTeamId", thieving_team_id);
                        child_eb.push_metadata_str("receiveTeamName", thieving_team_nickname);

                        child_eb.build(EventType::PlayerMoved)
                    });

                    eb.push_child(player_collected.artificially_forged_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{target_player_name} was Artificially Forged!\n\nSun(Sun)'s Pressure built..."));
                        child_eb.push_player_tag(target_player_id);
                        child_eb.push_team_tag(thieving_team_id);

                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.push_metadata_str("to", "LEGENDARY");
                        child_eb.push_metadata_str("from", player_collected.replaced_mod_id);

                        child_eb.build(EventType::ModChange)
                    });

                    if let Some(stronger_together) = player_collected.stronger_together {
                        eb.push_child(stronger_together.sub_event, |mut togetherness_eb| {
                            togetherness_eb.push_team_tag(thieving_team_id);
                            for player_id in stronger_together.player_ids {
                                togetherness_eb.push_player_tag(player_id);
                            }

                            let description = stronger_together.player_names.iter()
                                .with_position()
                                .flat_map(|(position, name)| {
                                    match position {
                                        Position::First | Position::Only => ["", name.as_str()],
                                        Position::Middle => [", ", name.as_str()],
                                        Position::Last => [", and ", name.as_str()],
                                    }
                                })
                                .chain(iter::once(" are stronger together."))
                                .join("");

                            togetherness_eb.push_description(description);
                            togetherness_eb.push_metadata_str("mod", "YOLKED");
                            togetherness_eb.push_metadata_str("source", "HARD_BOILED");
                            togetherness_eb.push_metadata_i64("type", ModDuration::Permanent);
                            togetherness_eb.build(EventType::AddedModFromOtherMod)
                        });
                    }
                }

                eb.build(EventType::TunnelsUsed)
            }
            FedEventData::TeamTunnelHeistContinues { game, target_player_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("...They approached {target_player_name}."));
                eb.build(EventType::TunnelsUsed)
            }
            FedEventData::TeamTunnelHeistFailed { game, target_player_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("But {target_player_name} evaded them!"));
                eb.build(EventType::TunnelsUsed)
            }
            FedEventData::TeamTunnelHeistSucceeded { game, target_team_nickname, target_player_name } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {target_team_nickname} collected {target_player_name}!"));
                eb.push_description(format!("{target_player_name} was Artificially Forged!"));
                eb.push_description("");
                eb.push_description("Sun(Sun)'s Pressure built...");
                eb.build(EventType::TunnelsUsed)
            }
            FedEventData::PitcherCyclesOut { game, team_nickname, outgoing_pitcher_name, incoming_pitcher_name } => {
                eb.set_game(game);
                eb.push_description(format!("{team_nickname}' pitcher {outgoing_pitcher_name} Cycles out for {incoming_pitcher_name}!"));
                eb.build(EventType::PitcherCyclesOut)
            }
            FedEventData::SunSunPressureBuilt { pressure_after } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("Sun(Sun)'s Pressure built...");

                eb.push_metadata_f64("current", pressure_after);
                eb.push_metadata_i64("maximum", 99999);
                eb.push_metadata_i64("recharge", 26244);

                eb.build(EventType::SunSunPressure)
            }
            FedEventData::HorsePower { game, away_team_emoji, away_team_name, away_team_score, home_team_emoji, home_team_name, home_team_score, stabled_players, scoring_team_id, scoring_team_name, unruns_scored, unruns_sub_event } => {
                eb.set_game(game);
                eb.push_description("Horse Power Achieved.");
                eb.push_description(format!("The {} and {} were Stabled!", away_team_name, home_team_name));

                for stabled_player in stabled_players {
                    eb.push_child(stabled_player.sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} was Stabled in The Vault.", stabled_player.player_name));
                        child_eb.push_team_tag(stabled_player.team_id);
                        child_eb.push_player_tag(stabled_player.player_id);
                        child_eb.push_metadata_str("mod", "MARKED");
                        child_eb.push_metadata_i64("type", stabled_player.duration);

                        child_eb.build(EventType::RemovedMod)
                    });
                }

                eb.push_child(unruns_sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Game);
                    child_eb.push_description(format!("The {scoring_team_name} scored!"));
                    child_eb.push_team_tag(scoring_team_id);

                    child_eb.push_metadata_str("ledger", format!("Stables: {unruns_scored} Unruns"));
                    child_eb.push_metadata_str("update", format!("{unruns_scored} Unruns scored!"));
                    child_eb.push_metadata_str("awayEmoji", away_team_emoji);
                    child_eb.push_metadata_i64_or_f64("awayScore", away_team_score);
                    child_eb.push_metadata_str("homeEmoji", home_team_emoji);
                    child_eb.push_metadata_i64_or_f64("homeScore", home_team_score);

                    child_eb.build(EventType::RunsScored)
                });

                eb.build(EventType::HorsePower)
            }
            FedEventData::Supernova { game } => {
                eb.set_game(game);
                eb.push_description("SUN(SUN) SUPERNOVA");

                eb.build(EventType::Supernova)
            }
            FedEventData::GameCanceled { game } => {
                eb.set_game(game);
                eb.push_description("GAME CANCELLED.");

                eb.build(EventType::GameCanceled)
            }
            FedEventData::PlayersCutFromTeam { team_nickname, team_id, location, players } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {team_nickname} cut {} players from their {}.", players.len(), location.name_post_merge()));
                eb.push_team_tag(team_id);
                for player in &players {
                    eb.push_player_tag(player.player_id);
                }

                eb.push_metadata_i64("location", location);
                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", &team_nickname);
                eb.push_metadata_str_vec("playerIds", players.iter().map(|p| p.player_id.to_string()).collect());
                eb.push_metadata_str_vec("playerNames", players.into_iter().map(|p| p.player_name).collect());

                eb.build(EventType::PlayersCutFromTeam)
            }
            FedEventData::PlayerLeftVault { player_name, player_id, new_team_nickname, new_team_id, location, add_to_team_sub_event, shadow_boost } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{player_name} left the Vault."));
                eb.push_player_tag(player_id);

                let mut add_to_team_eb = eb.connected_event(add_to_team_sub_event);
                add_to_team_eb.set_description(format!("The {new_team_nickname} added a player to their roster."));
                add_to_team_eb.push_metadata_i64("location", location);
                add_to_team_eb.push_metadata_uuid("teamId", new_team_id);
                add_to_team_eb.push_metadata_str("teamName", new_team_nickname);
                add_to_team_eb.push_metadata_uuid("playerId", player_id);
                add_to_team_eb.push_metadata_str("playerName", &player_name);
                add_to_team_eb.push_team_tag(new_team_id);
                let add_to_team_event = add_to_team_eb.build(EventType::PlayerAddedToTeam);

                let shadow_boost_event = shadow_boost.map(|shadow_boost| {
                    let mut shadow_boost_event = eb.connected_event(shadow_boost.sub_event);
                    shadow_boost_event.set_description(format!("{player_name} entered the Shadows."));
                    shadow_boost_event.push_team_tag(new_team_id);
                    shadow_boost_event.build_boost(&shadow_boost)
                });

                let main_event = eb.build(EventType::PlayerLeftVault);

                return if let Some(shadow_boost_event) = shadow_boost_event {
                    vec![main_event, add_to_team_event, shadow_boost_event]
                } else {
                    vec![main_event, add_to_team_event]
                }
            }
            FedEventData::TeamIncineration { game, incinerated_team_name, incinerated_team_nickname, incinerated_team_id, replacement_team_name, replacement_team_nickname, replacement_team_id, division_name, division_id, surviving_players, incinerated_players, weather_sub_event, team_entered_hall_sub_event, replacement_team_source, team_replaced_sub_event, instability_chain } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                if let Some(instability) = &instability_chain {
                    eb.push_description(format!("The {} are Unstable!", instability.incinerated_team_nickname));
                    eb.push_description("A Debt was collected.");
                }
                eb.push_description(format!("A Rogue Umpire incinerated the {incinerated_team_name}!"));
                eb.push_description(format!("They're replaced by the {replacement_team_name}!"));
                if !surviving_players.is_empty() {
                    let names_str = surviving_players.iter()
                        .map(|player| &player.player_name)
                        .join(" and ");
                    eb.push_description(format!("{names_str} joined the {replacement_team_nickname}!"));
                }
                if let Some(instability) = &instability_chain {
                    eb.push_description(format!("The Instability chains to the {}!", instability.chained_to_team_nickname));
                }

                for surviving_player in &surviving_players {
                    eb.push_child(surviving_player.jumped_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} jumped from the incinerated {incinerated_team_nickname}.", surviving_player.player_name));
                        child_eb.push_player_tag(surviving_player.player_id);
                        child_eb.push_team_tag(incinerated_team_id);
                        child_eb.push_metadata_str("teamName", &incinerated_team_nickname);
                        child_eb.push_metadata_uuid("teamId", incinerated_team_id);
                        child_eb.push_metadata_str("playerName", &surviving_player.player_name);
                        child_eb.push_metadata_uuid("playerId", surviving_player.player_id);
                        child_eb.build(EventType::PlayerRemovedFromTeam)
                    });
                }

                eb.push_child(weather_sub_event, |mut child_eb| {
                    child_eb.set_category(EventCategory::Special);
                    child_eb.push_description(format!("A Rogue Umpire incinerated the {incinerated_team_name}!"));
                    child_eb.push_team_tag(incinerated_team_id);
                    for victim in &incinerated_players {
                        child_eb.push_player_tag(victim.player_id);
                    }
                    child_eb.push_metadata_str("effect", "Team Incineration");
                    child_eb.push_metadata_i64("weather", Weather::SupernovaEclipse);
                    child_eb.build(EventType::WeatherEvent)
                });

                eb.push_child(team_entered_hall_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("The {incinerated_team_nickname} entered the Hall of Flame."));
                    child_eb.push_team_tag(incinerated_team_id);
                    child_eb.build(EventType::EnterHallOfFlame)
                });

                for victim in incinerated_players {
                    eb.push_child(victim.player_entered_hall_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} entered the Hall of Flame.", victim.player_name));
                        child_eb.push_player_tag(victim.player_id);
                        child_eb.build(EventType::EnterHallOfFlame)
                    });
                }

                for surviving_player in &surviving_players {
                    eb.push_child(surviving_player.fire_eater_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} ate some flame.", surviving_player.player_name));
                        child_eb.push_player_tag(surviving_player.player_id);
                        child_eb.push_team_tag(incinerated_team_id);
                        child_eb.push_metadata_i64("type", ModDuration::Permanent);
                        child_eb.push_metadata_str("mod", "MAGMATIC");
                        child_eb.build(EventType::AddedMod)
                    });
                }

                match replacement_team_source {
                    TeamIncinerationReplacementSource::NewTeam { team_formed_sub_event, new_players } => {
                        eb.push_child(team_formed_sub_event, |mut child_eb| {
                            child_eb.push_description(format!("The {replacement_team_name} formed."));
                            child_eb.push_team_tag(replacement_team_id);
                            child_eb.push_metadata_uuid("id", replacement_team_id);
                            child_eb.build(EventType::TeamFormed)
                        });

                        for new_player in new_players {
                            eb.push_child(new_player.player_born_sub_event, |mut child_eb| {
                                // The rare child event in the Game category
                                child_eb.set_category(EventCategory::Game);
                                child_eb.push_description(format!("{} was a founding member of the {replacement_team_nickname}.", new_player.player_name));
                                child_eb.build(EventType::PlayerDivisionMove)
                            });
                        }
                    }
                    TeamIncinerationReplacementSource::Squiddish { gained_squiddish_sub_event, exited_hall_sub_event, resurrected_players } => {
                        eb.push_child(gained_squiddish_sub_event, |mut child_eb| {
                            child_eb.push_description(format!("The {replacement_team_nickname} became Squiddish!"));
                            child_eb.push_team_tag(replacement_team_id);
                            child_eb.push_metadata_str("mod", "SQUIDDISH");
                            child_eb.push_metadata_i64("type", ModDuration::Permanent);
                            child_eb.build(EventType::AddedMod)
                        });

                        eb.push_child(exited_hall_sub_event, |mut child_eb| {
                            child_eb.push_description(format!("The {replacement_team_nickname} exited the Hall of Flame"));
                            child_eb.push_team_tag(replacement_team_id);
                            child_eb.build(EventType::ExitHallOfFlame)
                        });

                        for resurrected_player in resurrected_players {
                            eb.push_child(resurrected_player.player_born_sub_event, |mut child_eb| {
                                child_eb.push_description(format!("{} exited the Hall of Flame", resurrected_player.player_name));
                                child_eb.push_player_tag(resurrected_player.player_id);
                                child_eb.build(EventType::ExitHallOfFlame)
                            });
                        }
                    }
                }

                eb.push_child(team_replaced_sub_event, |mut child_eb| {
                    child_eb.push_description(format!("The {replacement_team_name} replaced the incinerated {incinerated_team_name}."));
                    child_eb.push_team_tag(incinerated_team_id);
                    child_eb.push_team_tag(replacement_team_id);
                    child_eb.push_metadata_uuid("inTeamId", replacement_team_id);
                    child_eb.push_metadata_str("inTeamName", &replacement_team_nickname);
                    child_eb.push_metadata_uuid("outTeamId", incinerated_team_id);
                    child_eb.push_metadata_str("outTeamName", incinerated_team_nickname);
                    child_eb.push_metadata_uuid("divisionId", division_id);
                    child_eb.push_metadata_str("divisionName", division_name);
                    child_eb.build(EventType::TeamIncinerationReplacement)
                });

                for surviving_player in surviving_players {
                    eb.push_child(surviving_player.join_team_sub_event, |mut child_eb| {
                        child_eb.push_description(format!("{} joined the {replacement_team_name}.", surviving_player.player_name));
                        child_eb.push_player_tag(surviving_player.player_id);
                        child_eb.push_team_tag(replacement_team_id);
                        child_eb.push_metadata_i64("location", surviving_player.roster_location);
                        child_eb.push_metadata_str("teamName", &replacement_team_nickname);
                        child_eb.push_metadata_uuid("teamId", replacement_team_id);
                        child_eb.push_metadata_str("playerName", &surviving_player.player_name);
                        child_eb.push_metadata_uuid("playerId", surviving_player.player_id);
                        child_eb.build(EventType::PlayerAddedToTeam)
                    });
                }

                if let Some(instability_chain) = instability_chain {
                    eb.push_child(instability_chain.sub_event, |mut child_eb| {
                        child_eb.push_description(format!("The Instability chains to the {}!", instability_chain.chained_to_team_nickname));
                        child_eb.push_team_tag(instability_chain.chained_to_team_id);
                        child_eb.push_metadata_str("mod", "MARKED");
                        child_eb.push_metadata_i64("type", ModDuration::Weekly);
                        child_eb.build(EventType::AddedMod)
                    });
                }

                eb.build(EventType::Incineration)
            }
            FedEventData::PlayerBecameStuck { game, player_name, player_id, team_id, sub_event } => {
                eb.set_game(game);
                let description = format!("{player_name} became Stuck!");
                eb.push_description(&description);
                eb.push_child(sub_event, |mut child_eb| {
                    child_eb.push_description(description);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("mod", "STUCK");
                    child_eb.push_metadata_str("source", "AVOIDANCE");
                    child_eb.push_metadata_i64("type", ModDuration::Game);
                    child_eb.build(EventType::AddedModFromOtherMod)
                });

                eb.build(EventType::PlayerBecameStuck)
            },
            FedEventData::SupernovaLeagueReassignment { black_hole_mod_added_sub_event, pulsar_mod_added_sub_event } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description("EMERGENCY ALERT");
                eb.push_description("RIFFING INTENSIFIES");
                eb.push_description("SUPERNOVA COLLAPSES");
                eb.push_description("REALITY TEARS");
                eb.push_description("STRANDS BRIDGED");
                eb.push_description("ENDS ZONE");
                // I've only ever seen this with an empty vec, so the type
                // is anybody's guess
                eb.push_metadata_str_vec("beings", Vec::new());

                let mut events = Vec::new();
                let mut black_hole_mod_added_event = eb.connected_event(black_hole_mod_added_sub_event);
                black_hole_mod_added_event.set_category(EventCategory::Changes);
                black_hole_mod_added_event.set_description("BLACK HOLE (BLACK HOLE) DRAINS".to_string());
                black_hole_mod_added_event.push_metadata_str("mod", "SMBH");
                black_hole_mod_added_event.push_metadata_i64("type", ModDuration::Permanent);
                events.push(black_hole_mod_added_event.build(EventType::LeagueModificationAdded));

                let mut pulsar_mod_added_event = eb.connected_event(pulsar_mod_added_sub_event);
                pulsar_mod_added_event.set_category(EventCategory::Changes);
                pulsar_mod_added_event.set_description("PULSAR (PULSAR) BEAMS".to_string());
                pulsar_mod_added_event.push_metadata_str("mod", "PULSAR");
                pulsar_mod_added_event.push_metadata_i64("type", ModDuration::Permanent);
                events.push(pulsar_mod_added_event.build(EventType::LeagueModificationAdded));

                events.insert(0, eb.build(EventType::Announcement));
                return events;
            },
            FedEventData::BlackHoleBlackHoleNullifiedLeagueModification { game, team_nickname, nullified_mod_name, nullified_mod_id, nullified_mod_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} collected 10!"));
                eb.push_description("Black Hole (Black Hole) became Agitated.");
                let nullified_description = format!("Black Hole (Black Hole) nullified {nullified_mod_name}!");
                eb.push_description(&nullified_description);

                eb.push_child(nullified_mod_sub_event, |mut child_eb| {
                    child_eb.push_description(nullified_description);
                    child_eb.push_metadata_str("mod", nullified_mod_id);
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::LeagueModificationRemoved)
                });

                eb.build(EventType::BlackHoleAgitated)
            },
            FedEventData::BlackHoleBlackHoleNullifiedStadiumModification { game, team_nickname, someones_team_id, stadium_name, nullified_mod_name, nullified_mod_id, nullified_mod_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} collected 10!"));
                eb.push_description("Black Hole (Black Hole) became Agitated.");
                let nullified_description = format!("Black Hole (Black Hole) nullified {} {nullified_mod_name}!", Possessive(&stadium_name));
                eb.push_description(&nullified_description);

                eb.push_child(nullified_mod_sub_event, |mut child_eb| {
                    child_eb.push_description(nullified_description);
                    child_eb.push_team_tag(someones_team_id);
                    child_eb.push_metadata_str("mod", nullified_mod_id);
                    child_eb.push_metadata_i64("type", ModDuration::Permanent);
                    child_eb.build(EventType::RemovedMod)
                });

                eb.build(EventType::BlackHoleAgitated)
            },
            FedEventData::BlackHoleBlackHoleNullifiedItem { game, player_name, player_id, team_nickname, team_id, item_name, item_id, item_mods, player_item_rating_before, player_item_rating_after, player_rating, item_removed_sub_event } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} collected 10!"));
                eb.push_description("Black Hole (Black Hole) became Agitated.");
                let nullified_description = format!("Black Hole (Black Hole) nullified {} {item_name}!", Possessive(&player_name));
                eb.push_description(&nullified_description);

                eb.push_child(item_removed_sub_event, |mut child_eb| {
                    child_eb.push_description(nullified_description);
                    child_eb.push_player_tag(player_id);
                    child_eb.push_team_tag(team_id);
                    child_eb.push_metadata_str("itemName", item_name);
                    child_eb.push_metadata_uuid("itemId", item_id);
                    child_eb.push_metadata_str_vec("mods", item_mods);
                    child_eb.push_metadata_f64_opt("playerItemRatingBefore", player_item_rating_before);
                    child_eb.push_metadata_f64("playerItemRatingAfter", player_item_rating_after);
                    child_eb.push_metadata_f64("playerRating", player_rating);
                    child_eb.build(EventType::PlayerLostItem)
                });

                eb.build(EventType::BlackHoleAgitated)
            },
            FedEventData::BlackHoleBlackHoleNullifiedTeamInGame { game, team_nickname } => {
                eb.set_game(game);
                eb.set_category(EventCategory::Special);
                eb.push_description(format!("The {team_nickname} collected 10!"));
                eb.push_description("Black Hole (Black Hole) became Agitated.");
                eb.push_description(format!("Black Hole (Black Hole) nullified the {team_nickname}!"));

                eb.build(EventType::BlackHoleAgitated)
            },
            FedEventData::TeamShiftedDivision { team_id, team_nickname, from_division_id, from_division_name, to_division_id, to_division_name } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {team_nickname} Shifted from the {from_division_name} to the {to_division_name}."));
                eb.push_team_tag(team_id);

                eb.push_metadata_uuid("teamId", team_id);
                eb.push_metadata_str("teamName", team_nickname);eb.push_metadata_uuid("fromDivisionId", from_division_id);
                eb.push_metadata_str("fromDivisionName", from_division_name);
                eb.push_metadata_uuid("toDivisionId", to_division_id);
                eb.push_metadata_str("toDivisionName", to_division_name);

                eb.build(EventType::TeamDivisionMove)
            },
            FedEventData::TeamTouchedDown { team_nickname_caps, team_id, players } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("{team_nickname_caps}, TOUCH DOWN"));
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", "SCATTERED");
                eb.push_metadata_i64("type", ModDuration::Permanent);

                let mut events = players.into_iter()
                    .map(|player| {
                        let mut eb = eb.connected_event(player.sub_event);
                        // TODO I think I clear or reset the description now
                        //   more than I set it. Make that the default.
                        eb.clear_description();
                        eb.push_description(format!("{}, TOUCH DOWN", player.player_name_all_caps));
                        eb.push_player_tag(player.player_id);
                        eb.push_metadata_str("mod", "SCATTERED");
                        eb.push_metadata_i64("type", ModDuration::Permanent);

                        eb.build(EventType::AddedMod)
                    })
                    .collect_vec();
                events.insert(0, eb.build(EventType::AddedMod));
                return events;
            }
            FedEventData::CoinScattered { attacking_team_name } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("The {attacking_team_name} Scattered the Coin!"));
                eb.build(EventType::CoinHit)
            }
            FedEventData::CoinIncinerated { attacking_division_name } => {
                eb.set_category(EventCategory::Outcomes);
                eb.push_description(format!("{attacking_division_name} Teams Incinerated the Coin!"));
                eb.build(EventType::CoinHit)
            }
            FedEventData::TeamExitedHallOfFlame { team_id, team_name, division_id, division_name, players, team_joined_division_sub_event } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("The {team_name} exited the Hall of Flame"));
                eb.push_team_tag(team_id);
                eb.push_metadata_bool("hideOnResults", true);

                let mut events = players.iter()
                    .map(|player| {
                        let mut conn_eb = eb.connected_event(player.sub_event);
                        // TODO This is only set_* because connected_event copies the description.
                        //   It should probably not do that.
                        conn_eb.set_description(format!("{} exited the Hall of Flame", player.player_name));
                        conn_eb.push_player_tag(player.player_id);
                        conn_eb.set_team_tags(Vec::new());
                        conn_eb.push_metadata_bool("hideOnResults", true);
                        conn_eb.build(EventType::ExitHallOfFlame)
                    })
                    .collect_vec();

                let mut team_joined_division_eb = eb.connected_event(team_joined_division_sub_event);
                team_joined_division_eb.clear_description(); // TODO connected_event shouldn't save description
                team_joined_division_eb.push_description(format!("The {team_name} have joined the {division_name} division."));
                team_joined_division_eb.push_metadata_uuid("teamId", team_id);
                team_joined_division_eb.push_metadata_str("teamName", team_name);
                team_joined_division_eb.push_metadata_uuid("divisionId", division_id);
                team_joined_division_eb.push_metadata_str("divisionName", division_name);
                team_joined_division_eb.push_metadata_bool("hideOnResults", true);

                events.push(team_joined_division_eb.build(EventType::TeamDivisionMove));

                let first_event = eb.build(EventType::ExitHallOfFlame);
                events.insert(0, first_event);
                return events;
            }
            FedEventData::HallOfFlameOpened => {
                eb.set_category(EventCategory::Changes);
                eb.push_description("The Hall of Flame was opened.");
                eb.push_metadata_str_vec("beings", vec!["monitor".to_string()]);
                eb.build(EventType::Announcement)
            }
            FedEventData::TeamEnteredEndZone { quadrant, team_name, team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(match quadrant {
                    EndZone::Vault => format!("TODO"),
                    EndZone::Horizon => format!("The {team_name} were Entangled in the Black Hole (Black Hole)."),
                    EndZone::Hall => format!("The {team_name} went Rogue."),
                    EndZone::Desert => format!("TODO"),
                    EndZone::TODOWhereDoesForceComeFrom => format!("The {team_name} were Forced into Position."),
                });
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", match quadrant {
                    EndZone::Vault => "TODO",
                    EndZone::Horizon => "ENTANGLED",
                    EndZone::Hall => "ROGUE",
                    EndZone::Desert => "TODO",
                    EndZone::TODOWhereDoesForceComeFrom => "FORCE",
                });
                eb.push_metadata_i64("type", ModDuration::Permanent);

                eb.build(EventType::AddedMod)
            }
            FedEventData::GameEndFromNullification { game, non_loser } => {
                eb.set_game(game);

                match non_loser {
                    None => {
                        eb.push_description("{nullteam} and {nullteam} were both nullified.");
                        eb.push_description("Game canceled.");
                        eb.push_description("Neither Team non-lost.");
                    }
                    Some(non_loss_sub_event) => {
                        eb.push_description(format!("The {{nullteam}} were nullified.\nThe {} non-lost the game.", non_loss_sub_event.team_nickname));

                        assert!(non_loss_sub_event.balloons.is_none(), "No support for balloons in GameEndFromNullification yet");

                        eb.push_child(non_loss_sub_event.sub_event, |mut child_eb| {
                            child_eb.set_category(EventCategory::Outcomes);
                            child_eb.push_description(format!("The {} non-lost due to nullification.", non_loss_sub_event.team_nickname));
                            child_eb.push_team_tag(non_loss_sub_event.team_id);
                            child_eb.push_metadata_i64("after", non_loss_sub_event.wins_after);
                            child_eb.push_metadata_i64("amount", 1);
                            child_eb.push_metadata_i64("before", non_loss_sub_event.wins_after - 1);
                            child_eb.push_metadata_str_vec("lines", Vec::new()); // Always empty so far
                            child_eb.build(EventType::WinCollectedRegular)
                        });

                    }
                }

                eb.build(EventType::GameEndedFromNullification)
            },
            FedEventData::BlackHoleBlackHoleNullifiedTeamOnMap { team_nickname, team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(format!("Black Hole (Black Hole) nullified the {team_nickname}!"));
                eb.push_team_tag(team_id);

                eb.build(EventType::BlackHoleAgitated)
            },
            FedEventData::TeamExitedEndZone { quadrant, team_name, team_id } => {
                eb.set_category(EventCategory::Changes);
                eb.push_description(match quadrant {
                    EndZone::Vault => format!("The {team_name} stopped going Rogue."),
                    EndZone::Horizon => format!("TODO"),
                    EndZone::Hall => format!("TODO"),
                    EndZone::Desert => format!("TODO"),
                    EndZone::TODOWhereDoesForceComeFrom => format!("TODO"),
                });
                eb.push_team_tag(team_id);
                eb.push_metadata_str("mod", match quadrant {
                    EndZone::Vault => "ROGUE",
                    EndZone::Horizon => "TODO",
                    EndZone::Hall => "TODO",
                    EndZone::Desert => "TODO",
                    EndZone::TODOWhereDoesForceComeFrom => "TODO",
                });
                eb.push_metadata_i64("type", ModDuration::Permanent);

                eb.build(EventType::RemovedMod)
            }
        };

        vec![item]
    }
        }