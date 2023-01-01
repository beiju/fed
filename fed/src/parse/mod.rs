pub mod error;
pub mod event_schema;
mod feed_event_util;
pub mod builder;
pub mod event_builder_new;
mod parsers;
pub mod stream;
mod parse_wrapper;

use serde::Deserialize;
use uuid::{Uuid, uuid};
// the second one is a macro
use eventually_api::{EventCategory, EventType, EventuallyEvent, Weather};

use crate::parse::error::FeedParseError;
use crate::parse::event_schema::*;
use crate::parse::parsers::*;
use crate::parse::parse_wrapper::EventParseWrapper;

pub use stream::expansion_era_events;

const KNOWN_TEAM_NICKNAMES: [&'static str; 24] = [
    "Fridays", "Moist Talkers", "Lovers", "Jazz Hands", "Sunbeams", "Tigers", "Wild Wings",
    "Flowers", "Millennials", "Pies", "Garages", "Dale", "Lift", "Firefighters", "Steaks",
    "Magic", "Breath Mints", "Spies", "Shoe Thieves", "Tacos", "Georgias", "Worms", "Crabs",
    "Mechanics",
];

const TAROT_EVENTS: [Uuid; 10] = [
    uuid!("0d96d9ed-8e40-47ca-a543-b27518b276ef"), // Curry gets Over Under
    uuid!("6dd0204e-213b-4798-9fad-e042a232edc6"), // Krod gets Under Over
    uuid!("760ee47b-7698-4216-9612-e67c13ba12ef"), // Fridays get Sinking Ship
    uuid!("17df7d13-41df-4caf-af56-da75577a43e8"), // Lovers get Base Dealing
    uuid!("6a9e3ad7-f6a7-437c-9bd5-22b602a32cc3"), // Quitter gets Receiver
    uuid!("b0457046-0e88-482a-b3b4-aed27c598a5c"), // Moses gets Receiver
    uuid!("77df7273-e3c3-49b1-9ce5-4baec629d75a"), // Mints get Middling
    uuid!("9cd56488-5ee2-436e-9196-37a76593cdaf"), // Flowers get After Party
    uuid!("1bb3708a-a43f-472b-a7df-a4b2f52c313f"), // Melon loses Alternate
    uuid!("00bb210e-d0c6-41bf-a6f7-01de9070582a"), // Jimmy loses Superyummy
];

#[allow(non_snake_case)]
fn ParseOk<T>(v: T) -> Result<T, FeedParseError> {
    Ok(v)
}

pub fn parse_feed_event(feed_event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    if feed_event.metadata.siblings.is_empty() {
        parse_single_feed_event(feed_event)
    } else {
        todo!()
    }
}

fn parse_single_feed_event(event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    let mut event = EventParseWrapper::new(event)?;
    // This variable exists just for me to look at in the debugger, because the debugger
    // representation of the Uuid type is to low-level to copy-paste
    let _id_string = event.id.to_string();

    // This can happen on the majority of events, so I handle it outside
    let unscatter = event.next_child_if_mod_effect(EventType::RemovedMod, "SCATTERED")?.map(|mut child| {
        let player_name = child.next_parse(parse_terminated(" was Unscattered."))?;
        ParseOk(Unscatter {
            sub_event: child.as_sub_event(),
            team_id: child.next_team_id()?,
            player_id: child.next_player_id()?,
            player_name: player_name.to_string(),
        })
    }).transpose()?;

    // Ditto
    let attractor_secret_base = event.next_parse_opt(parse_terminated(" enters the Secret Base...\n"))
        .map(|name| {
            ParseOk(PlayerInfo {
                player_id: event.next_player_id()?,
                player_name: name.to_string(),
            })
        })
        .transpose()?;

    let data = match event.event_type {
        EventType::Undefined => { todo!() }
        EventType::LetsGo => {
            event.next_parse_tag("Let's Go!")?;
            FedEventData::LetsGo {
                game: event.game(unscatter, attractor_secret_base)?,
                weather: Weather::try_from(event.metadata_i64("weather")? as i32)
                    .map_err(|err| FeedParseError::UnknownWeather(err.number))?,
                stadium_id: event.metadata_uuid("stadium").ok(),
            }
        }
        EventType::PlayBall => {
            event.next_parse_tag("Play ball!")?;

            FedEventData::PlayBall {
                game: event.game(unscatter, attractor_secret_base)?,
            }
        }
        EventType::HalfInning => {
            // Starting in s16, subseasonal mods (mods that apply only during Earl/Mid/Lateseason)
            // announce when they start or end in the first HalfInning of the game. It's
            // theoretically possible that there would be some starting and others ending in the
            // same event (the days coincide) but it didn't happen in Beta, so I don't know the
            // order it would apply in. I'm assuming it would be interleaved.
            let mut subseasonal_mod_effects = Vec::new();
            while let Some(mut child) = event.next_child_any_opt(&[EventType::AddedModFromOtherMod, EventType::RemovedModFromOtherMod])? {
                // The nickname and mod name are on both child and parent, but we need to consume
                // the description from the parent anyway, so it's better to parse it from there
                let (team_nickname, source_mod_name) = event.next_parse(parse_subseasonal_mod_change)?;
                assert!(is_known_team_nickname(team_nickname));
                subseasonal_mod_effects.push(TeamPerformingChanged {
                    team_id: child.next_team_id()?,
                    team_nickname: team_nickname.to_string(),
                    source_mod_id: child.metadata_str("source")?.to_string(),
                    source_mod_name: source_mod_name.to_string(),
                    was_added: child.event_type == EventType::AddedModFromOtherMod,
                    sub_event: child.as_sub_event(),
                })
            }

            let (top_of_inning, inning, team_name) = event.next_parse(parse_half_inning)?;
            assert!(is_known_team_name(team_name));

            FedEventData::HalfInningStart {
                game: event.game(unscatter, attractor_secret_base)?,
                top_of_inning,
                inning,
                batting_team_name: team_name.to_string(),
                subseasonal_mod_effects,
            }
        }
        EventType::PitcherChange => {
            let (victim_name, team_name) = event.next_parse(parse_pitcher_change)?;

            assert!(is_known_team_nickname(team_name));

            FedEventData::PitcherChange {
                game: event.game(unscatter, attractor_secret_base)?,
                team_nickname: team_name.to_string(),
                pitcher_id: event.next_player_id()?,
                pitcher_name: victim_name.to_string(),
            }
        }
        EventType::StolenBase => {
            let (runner_name, base_stolen, is_successful, blaserunning, free_refiller) = event.next_parse(parse_stolen_base)?;
            if is_successful {
                let runner_id = event.next_player_id()?;

                let runner_item_damage = event.parse_item_damage(runner_name)?;

                FedEventData::StolenBase {
                    game: event.game(unscatter, attractor_secret_base)?,
                    runner_name: runner_name.to_string(),
                    runner_id,
                    base_stolen,
                    blaserunning,
                    free_refill: free_refiller.map(|refiller_name| {
                        let mut sub_event = event.next_child(EventType::RemovedMod)?;
                        ParseOk(FreeRefill {
                            sub_event: sub_event.as_sub_event(),
                            player_name: refiller_name.to_string(),
                            player_id: sub_event.next_player_id()?,
                            team_id: sub_event.next_team_id_opt(),
                        })
                    }).transpose()?,
                    runner_item_damage,
                    is_special: event.category == EventCategory::Special,
                }
            } else {
                FedEventData::CaughtStealing {
                    game: event.game(unscatter, attractor_secret_base)?,
                    runner_name: runner_name.to_string(),
                    base_stolen,
                }
            }
        }
        EventType::Walk => {
            let parsed_walk = event.next_parse(parse_walk)?;
            match parsed_walk {
                ParsedWalk::Ordinary((batter_name, base_instincts)) => {
                    let batter_id = event.next_player_id()?;
                    let scores = event.parse_scores(" scores!")?;

                    let batter_item_damage = event.parse_item_damage(batter_name)?;
                    let stopped_inhabiting = event.parse_stopped_inhabiting(Some(batter_id))?;
                    FedEventData::Walk {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        scores,
                        base_instincts,
                        batter_item_damage,
                        stopped_inhabiting,
                        is_special: event.category == EventCategory::Special,
                    }
                }
                ParsedWalk::Charm((broken_item, batter_name, pitcher_name)) => {
                    let batter_id = event.next_player_id()?;
                    let charmer_id = event.next_player_id()?;
                    assert_eq!(batter_id, charmer_id);

                    let scores = event.parse_scores(" scores!")?;
                    let (batter_item_damage, pitcher_item_damage) = match broken_item {
                        None => { (None, None) }
                        Some((ActivePositionType::Lineup, _item_name)) => {
                            (Some(event.next_item_damage()?), None)
                        }
                        Some((ActivePositionType::Rotation, _item_name)) => {
                            (None, Some(event.next_item_damage()?))
                        }
                    };

                    FedEventData::CharmWalk {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        pitcher_name: pitcher_name.to_string(),
                        batter_item_damage,
                        pitcher_item_damage,
                        scores,
                    }
                }
            }
        }
        EventType::Strikeout => {
            match event.next_parse(parse_strikeout)? {
                ParsedStrikeout::Swinging(batter_name) => {
                    let stopped_inhabiting = event.parse_stopped_inhabiting(None)?;
                    let pitcher_item_damage = event.parse_item_damage_and_name(true)?;
                    let free_refill = event.parse_free_refill()?;
                    FedEventData::StrikeoutSwinging {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                        pitcher_item_damage,
                        free_refill,
                        is_special: event.category == EventCategory::Special,
                    }
                }
                ParsedStrikeout::Looking(batter_name) => {
                    let stopped_inhabiting = event.parse_stopped_inhabiting(None)?;
                    let pitcher_item_damage = event.parse_item_damage_and_name(true)?;
                    let free_refill = event.parse_free_refill()?;
                    FedEventData::StrikeoutLooking {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                        pitcher_item_damage,
                        free_refill,
                        is_special: event.category == EventCategory::Special,
                    }
                }
                ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings } => {
                    let charmer_id = event.next_player_id()?;
                    let charmer_id_2 = event.next_player_id()?;
                    let charmed_id = event.next_player_id()?;
                    assert_eq!(charmer_id, charmer_id_2);
                    FedEventData::CharmStrikeout {
                        game: event.game(unscatter, attractor_secret_base)?,
                        charmer_id,
                        charmer_name: charmer_name.to_string(),
                        charmed_id,
                        charmed_name: charmed_name.to_string(),
                        num_swings,
                    }
                }
            }
        }
        EventType::FlyOut => {
            // Order matters
            let (batter_name, fielder_name) = event.next_parse(parse_flyout)?;
            let batter_debt = event.parse_batter_debt(batter_name, fielder_name)?;
            let fielder_item_damage = event.parse_item_damage(fielder_name)?;
            let scores = event.parse_scores(" tags up and scores!")?;
            let batter_item_damage = event.parse_item_damage(batter_name)?;
            let other_player_item_damage = event.parse_item_damage_and_name(true)?;
            let cooled_off = event.parse_cooled_off(batter_name)?;
            let stopped_inhabiting = event.parse_stopped_inhabiting(None)?; // Not sure about order here
            FedEventData::Flyout {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_name: batter_name.to_string(),
                fielder_name: fielder_name.to_string(),
                scores,
                stopped_inhabiting,
                cooled_off,
                is_special: event.category == EventCategory::Special,
                batter_debt,
                batter_item_damage,
                fielder_item_damage,
                other_player_item_damage,
            }
        }
        EventType::GroundOut => {
            match event.next_parse(parse_ground_out)? {
                ParsedGroundOut::Simple { batter_name, fielder_name } => {
                    let batter_debt = event.parse_batter_debt(batter_name, fielder_name)?;
                    let fielder_item_damage = event.parse_item_damage(fielder_name)?;
                    // just guessing about where this belongs
                    let pitcher_item_damage = event.parse_item_damage_and_name(true)?;
                    let scores = event.parse_scores(" advances on the sacrifice.")?;
                    let batter_item_damage = event.parse_item_damage(batter_name)?;
                    let stopped_inhabiting = event.parse_stopped_inhabiting(None)?;
                    let cooled_off = event.parse_cooled_off(batter_name)?;
                    FedEventData::GroundOut {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        fielder_name: fielder_name.to_string(),
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                        is_special: event.category == EventCategory::Special,
                        batter_debt,
                        batter_item_damage,
                        pitcher_item_damage,
                        fielder_item_damage,
                    }
                }
                ParsedGroundOut::FieldersChoice { runner_out_name, base } => {
                    // Breaking up the call to insert "reaches on fielders choice" in the middle
                    let scoring_players = event.parse_scoring_players(" scores!")?;
                    let batter_name = event.next_parse(parse_reaches_on_fielders_choice)?;
                    let scores = event.parse_scores_with_scoring_players(scoring_players)?;
                    let stopped_inhabiting = event.parse_stopped_inhabiting(None)?;
                    let cooled_off = event.parse_cooled_off(batter_name)?;
                    FedEventData::FieldersChoice {
                        game: event.game(unscatter, attractor_secret_base)?,
                        runner_out_name: runner_out_name.to_string(),
                        batter_name: batter_name.to_string(),
                        out_at_base: base,
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                        is_special: event.category == EventCategory::Special,
                    }
                }
                ParsedGroundOut::DoublePlay { batter_name } => {
                    let scores = event.parse_scores(" scores!")?;
                    let stopped_inhabiting = event.parse_stopped_inhabiting(None)?;
                    let cooled_off = event.parse_cooled_off(batter_name)?;
                    FedEventData::DoublePlay {
                        game: event.game(unscatter, attractor_secret_base)?,
                        batter_name: batter_name.to_string(),
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                    }
                }
            }
        }
        EventType::HomeRun => {
            let damaged_items = event.parse_item_damages_and_names(false)?;
            // In addition to getting a magmatic event, get a player name and id to check against
            // the batter name and id
            let magmatic_expanded = event.next_parse(parse_magmatic)?
                .map(|player_name| {
                    let mut child = event.next_child(EventType::RemovedMod)?;
                    let magmatic = ModChangeSubEvent {
                        sub_event: child.as_sub_event(),
                        team_id: child.next_team_id()?,
                    };

                    ParseOk((magmatic, player_name, child.next_player_id()?))
                })
                .transpose()?;

            let (batter_name, num_runs) = event.next_parse(parse_hr)?;

            let attraction = event.next_parse(parse_attract_player)?
                .map(|(team_nickname, player_name)| {
                    assert!(is_known_team_nickname(team_nickname));

                    let mut child = event.next_child(EventType::PlayerAddedToTeam)?;
                    ParseOk(AttractionWithPlayer {
                        team_nickname: team_nickname.to_string(),
                        team_id: child.next_team_id()?,
                        player_name: player_name.to_string(),
                        player_id: child.next_player_id()?,
                        sub_event: child.as_sub_event(),
                    })
                })
                .transpose()?;

            let big_bucket = event.next_parse(parse_big_bucket)?;
            let free_refills = event.parse_free_refills()?;
            let spicy_status = event.parse_spicy_status(batter_name)?;

            let batter_id = event.next_player_id()?;
            let stopped_inhabiting = event.parse_stopped_inhabiting(Some(batter_id))?;

            FedEventData::HomeRun {
                game: event.game(unscatter, attractor_secret_base)?,
                // TODO Verify batter name and id against magmatic
                magmatic: magmatic_expanded.map(|(m, _, _)| m),
                batter_name: batter_name.to_string(),
                batter_id,
                num_runs,
                stopped_inhabiting,
                free_refills,
                spicy_status,
                is_special: event.category == EventCategory::Special,
                big_bucket,
                attraction,
                damaged_items,
            }
        }
        EventType::Hit => {
            let (batter_name, hit_bases, batter_item_broke, pitcher_item_broke) = event.next_parse(parse_hit)?;
            // resim research says pitcher goes first
            let pitcher_item_damage = pitcher_item_broke
                .map(|(_item_name, player_name)| {
                    event.next_item_damage().map(|d| (player_name.to_string(), d))
                })
                .transpose()?;
            let batter_item_damage = batter_item_broke
                .map(|_item_name| {
                    event.next_item_damage()
                })
                .transpose()?;

            let batter_id = event.next_player_id()?;
            let stopped_inhabiting = event.parse_stopped_inhabiting(Some(batter_id))?;
            let scores = event.parse_scores(" scores!")?;
            let spicy_status = event.parse_spicy_status(batter_name)?;
            let other_player_item_damage = event.parse_item_damage_and_name(true)?;

            FedEventData::Hit {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_name: batter_name.to_string(),
                batter_id,
                hit_bases,
                scores,
                spicy_status,
                stopped_inhabiting,
                is_special: event.category == EventCategory::Special,
                pitcher_item_damage,
                batter_item_damage,
                other_player_item_damage,
            }
        }
        EventType::GameEnd => {
            let ((winning_team_name, winning_team_score), (losing_team_name, losing_team_score)) = event.next_parse(parse_game_end)?;

            let temp_stolen_player_returned = event.next_child_opt(EventType::PlayerMoved)?
                .map(|child| {
                    Ok::<_, FeedParseError>(PlayerMovedTeams {
                        player_id: child.metadata_uuid("playerId")?,
                        player_name: child.metadata_str("playerName")?.to_string(),
                        location: child.metadata_enum("location")?,
                        previous_team_id: child.metadata_uuid("sendTeamId")?,
                        previous_team_nickname: child.metadata_str("sendTeamName")?.to_string(),
                        new_team_id: child.metadata_uuid("receiveTeamId")?,
                        new_team_nickname: child.metadata_str("receiveTeamName")?.to_string(),
                        sub_event: child.as_sub_event(),
                    })
                })
                .transpose()?;

            FedEventData::GameEnd {
                game: event.game(unscatter, attractor_secret_base)?,
                winner_id: event.metadata_uuid("winner")?,
                winning_team_name: winning_team_name.to_string(),
                winning_team_score,
                losing_team_name: losing_team_name.to_string(),
                losing_team_score,
                temp_stolen_player_returned,
            }
        }
        EventType::BatterUp => {
            let (batter_name, inhabited, team_name, wielding_item, is_repeating) =
                event.next_parse(parse_batter_up)?;

            // I missed `team_name: "Millennials, wielding An Actual Airplane"` once and I don't
            // want something like that to happen again
            assert!(is_known_team_nickname(team_name));

            FedEventData::BatterUp {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_name: batter_name.to_string(),
                team_name: team_name.to_string(),
                wielding_item: wielding_item.map(|s| s.to_string()),
                inhabiting: inhabited.map(|inhabited| {
                    // Haunting doesn't have a sub-event if the player who Haunted already has the
                    // Inhabiting mod
                    let child = event.next_child_if_mod_effect(EventType::AddedMod, "INHABITING")?;

                    // These live on the parent
                    let inhabiting_player_id = event.next_player_id()?;
                    let inhabited_player_id = event.next_player_id()?;

                    ParseOk(Inhabiting {
                        sub_event: child.map(|c| c.as_sub_event()),
                        inhabited_player_name: inhabited.to_string(),
                        inhabiting_player_id,
                        inhabited_player_id,
                        inhabiting_player_team_id: child.and_then(|mut c| c.next_team_id_opt()),
                    })
                }).transpose()?,
                is_repeating,
            }
        }
        EventType::Strike => {
            let (strike_type, balls, strikes) = event.next_parse(parse_strike)?;
            let pitcher_item_damage = event.parse_item_damage_and_name(true)?;
            let game = event.game(unscatter, attractor_secret_base)?;
            match strike_type {
                StrikeType::Swinging => FedEventData::StrikeSwinging { game, balls, strikes, pitcher_item_damage },
                StrikeType::Looking => FedEventData::StrikeLooking { game, balls, strikes, pitcher_item_damage },
                StrikeType::Flinching => FedEventData::StrikeFlinching { game, balls, strikes, pitcher_item_damage },
            }
        }
        EventType::Ball => {
            let (balls, strikes) = event.next_parse(parse_ball)?;
            let batter_item_damage = event.parse_item_damage_and_name(true)?;
            FedEventData::Ball {
                game: event.game(unscatter, attractor_secret_base)?,
                balls,
                strikes,
                batter_item_damage,
            }
        }
        EventType::FoulBall => {
            // Eventually this will need very foul support, but I'll get to that when it comes up
            let (balls, strikes) = event.next_parse(parse_foul_ball)?;
            let batter_item_damage = event.parse_item_damage_and_name(true)?;
            FedEventData::FoulBall {
                game: event.game(unscatter, attractor_secret_base)?,
                balls,
                strikes,
                batter_item_damage,
            }
        }
        EventType::RunsOverflowing => {
            let (team_nickname, num_runs, unruns) = event.next_parse(parse_runs_overflowing)?;
            FedEventData::RunsOverflowing {
                game: event.game(unscatter, attractor_secret_base)?,
                team_nickname: team_nickname.to_string(),
                num_runs: if unruns { -num_runs } else { num_runs },
            }
        }
        EventType::HomeFieldAdvantage => { todo!() }
        EventType::HitByPitch => {
            let (pitcher_name, batter_name) = event.next_parse(parse_hit_by_pitch)?;
            let pitcher_id = event.next_player_id()?;
            let batter_id = event.next_player_id()?;
            let mut sub_event = event.next_child(EventType::AddedMod)?;

            let scores = event.parse_scores(" scores!")?;

            FedEventData::HitByPitch {
                game: event.game(unscatter, attractor_secret_base)?,
                pitcher_id,
                pitcher_name: pitcher_name.to_string(),
                batter_team_id: sub_event.next_team_id()?,
                batter_id,
                batter_name: batter_name.to_string(),
                sub_event: sub_event.as_sub_event(),
                scores,
            }
        }
        EventType::BatterSkipped => {
            let (player_name, reason) = event.next_parse(parse_batter_skipped)?;
            FedEventData::BatterSkipped {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_name: player_name.to_string(),
                reason: match reason {
                    ParsedBatterSkippedReason::Shelled => { BatterSkippedReason::Shelled }
                    ParsedBatterSkippedReason::Elsewhere => {
                        BatterSkippedReason::Elsewhere(event.next_player_id()?)
                    }
                },
            }
        }
        EventType::Party => {
            let player_name = event.next_parse(parse_party)?;
            let mut sub_event = event.next_child(EventType::PlayerStatIncrease)?;
            FedEventData::Party {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                player_id: sub_event.next_player_id()?,
                player_name: player_name.to_string(),
                sub_event: sub_event.as_sub_event(),
                rating_before: sub_event.metadata_f64("before")?,
                rating_after: sub_event.metadata_f64("after")?,
            }
        }
        EventType::StrikeZapped => {
            let _ = event.next_parse_tag("The Electricity zaps a strike away!")?;
            FedEventData::StrikeZapped {
                game: event.game(unscatter, attractor_secret_base)?,
            }
        }
        EventType::WeatherChange => { todo!() }
        EventType::MildPitch => {
            let (pitcher_name, pitch_type) = event.next_parse(parse_mild_pitch)?;
            let pitcher_id = event.next_player_id()?;

            match pitch_type {
                MildPitchType::Ball((balls, strikes)) => {
                    let runners_advance = event.next_parse(parse_runners_advance_on_mild_pitch)?;
                    let scores = event.parse_scores(" scores!")?;

                    FedEventData::MildPitch {
                        game: event.game(unscatter, attractor_secret_base)?,
                        pitcher_id,
                        pitcher_name: pitcher_name.to_string(),
                        balls,
                        strikes,
                        runners_advance,
                        scores,
                    }
                }
                MildPitchType::Walk(batter_name) => {
                    let batter_id = event.next_player_id()?;
                    let scores = event.parse_scores(" scores!")?;

                    FedEventData::MildPitchWalk {
                        game: event.game(unscatter, attractor_secret_base)?,
                        pitcher_id,
                        pitcher_name: pitcher_name.to_string(),
                        batter_id,
                        batter_name: batter_name.to_string(),
                        scores,
                    }
                }
            }
        }
        EventType::InningEnd => {
            let (inning_num, lost_triple_threat_names) = event.next_parse(parse_inning_end)?;

            FedEventData::InningEnd {
                game: event.game(unscatter, attractor_secret_base)?,
                inning_num,
                lost_triple_threat: zip_mod_change_events(&mut event, lost_triple_threat_names)?,
            }
        }
        EventType::BigDeal => {
            FedEventData::BeingSpeech {
                being: Being::try_from(event.metadata_i64("being")? as i32)
                    .map_err(|e| FeedParseError::UnknownBeing(e.number))?,
                message: event.consume_description().to_string(),
            }
        }
        EventType::BlackHole => {
            let (scoring_team, victim_team) = event.next_parse(parse_black_hole)?;
            assert!(is_known_team_nickname(scoring_team));
            assert!(is_known_team_nickname(victim_team));

            let carcinization = event.next_parse_opt(parse_carcinization)
                .map(|(team_name, _player_name)| {
                    assert!(is_known_team_name(team_name));
                    let child = event.next_child(EventType::PlayerMoved)?;
                    let mod_add_child = event.next_child(EventType::AddedMod)?;
                    Ok::<_, FeedParseError>(Carcinization {
                        mv: PlayerMovedTeams {
                            player_id: child.metadata_uuid("playerId")?,
                            player_name: child.metadata_str("playerName")?.to_string(),
                            location: child.metadata_enum("location")?,
                            previous_team_id: child.metadata_uuid("sendTeamId")?,
                            previous_team_nickname: child.metadata_str("sendTeamName")?.to_string(),
                            new_team_id: child.metadata_uuid("receiveTeamId")?,
                            new_team_nickname: child.metadata_str("receiveTeamName")?.to_string(),
                            sub_event: child.as_sub_event(),
                        },
                        new_team_name: team_name.to_string(),
                        mod_added_sub_event: mod_add_child.as_sub_event(),
                    })
                })
                .transpose()?;

            let compressed_by_gamma = event.next_parse_opt(parse_compressed_by_gamma)
                .map(|player_name| {
                    let mut child = event.next_child(EventType::PlayerStatDecrease)?;
                    Ok::<_, FeedParseError>(PlayerStatChange {
                        team_id: child.next_team_id()?,
                        player_id: child.next_player_id()?,
                        player_name: player_name.to_string(),
                        rating_before: child.metadata_f64("before")?,
                        rating_after: child.metadata_f64("after")?,
                        sub_event: child.as_sub_event(),
                    })
                })
                .transpose()?;

            FedEventData::BlackHole {
                game: event.game(unscatter, attractor_secret_base)?,
                scoring_team_nickname: scoring_team.to_string(),
                victim_team_nickname: victim_team.to_string(),
                carcinization,
                compressed_by_gamma,
            }
        }
        EventType::Sun2 => {
            let (scoring_team, rays_player) = event.next_parse(parse_sun2)?;
            assert!(is_known_team_nickname(scoring_team));

            let caught_some_rays = if let Some(player_name) = rays_player {
                let mut child = event.next_child(EventType::PlayerStatIncrease)?;
                Some(PlayerStatChange {
                    sub_event: child.as_sub_event(),
                    team_id: child.next_team_id()?,
                    player_id: child.next_player_id()?,
                    player_name: player_name.to_string(),
                    rating_before: child.metadata_f64("before")?,
                    rating_after: child.metadata_f64("after")?,
                })
            } else {
                None
            };

            FedEventData::Sun2 {
                game: event.game(unscatter, attractor_secret_base)?,
                team_nickname: scoring_team.to_string(),
                caught_some_rays,
            }
        }
        EventType::BirdsCircle => {
            event.next_parse_tag("The Birds circle ... but they don't find what they're looking for.")?;
            FedEventData::BirdsCircle {
                game: event.game(unscatter, attractor_secret_base)?,
            }
        }
        EventType::AmbushedByCrows => {
            let (pitcher_name, batter_name) = event.next_parse(parse_friend_of_crows)?;
            let (pitcher, batter_id) = if let Some(name) = pitcher_name {
                let pitcher_id = event.next_player_id()?;
                let batter_id = event.next_player_id()?;
                (Some(PitcherInfo { pitcher_id, pitcher_name: name.to_string() }), batter_id)
            } else {
                (None, event.next_player_id()?)
            };

            FedEventData::AmbushedByCrows {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_id,
                batter_name: batter_name.to_string(),
                friend_of_crows: pitcher,
            }
        }
        EventType::BirdsUnshell => {
            let player_name = event.next_parse(parse_birds_unshell)?;

            let mut pecked_free = event.next_child(EventType::RemovedMod)?;
            let mut superallergy = event.next_child(EventType::AddedMod)?;
            let team_id = pecked_free.next_team_id()?;
            assert_eq!(team_id, superallergy.next_team_id()?);
            let player_id = pecked_free.next_player_id()?;
            assert_eq!(player_id, superallergy.next_player_id()?);

            FedEventData::BirdsUnshell {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id,
                player_id,
                player_name: player_name.to_string(),
                pecked_free_event: pecked_free.as_sub_event(),
                superallergy_event: superallergy.as_sub_event(),
            }
        }
        EventType::BecomeTripleThreat => {
            let names = event.next_parse(parse_become_triple_threat)?;

            let pitchers = names.into_iter()
                .map(|pitcher_name| {
                    let mut sub_event = event.next_child(EventType::AddedMod)?;
                    ParseOk(ModChangeSubEventWithNamedPlayer {
                        sub_event: sub_event.as_sub_event(),
                        team_id: sub_event.next_team_id()?,
                        player_id: sub_event.next_player_id()?,
                        player_name: pitcher_name.to_string(),
                    })
                })
                .collect::<Result<_, _>>()?;

            FedEventData::BecomeTripleThreat {
                game: event.game(unscatter, attractor_secret_base)?,
                pitchers,
            }
        }
        EventType::GainFreeRefill => {
            let (player_name, roast, ingredient1, ingredient2) = event.next_parse(parse_gain_free_refill)?;
            let mut sub_event = event.next_child(EventType::AddedMod)?;
            let player_id = event.next_player_id()?;
            // The player ID should match in the sub event
            assert_eq!(player_id, sub_event.next_player_id()?);
            FedEventData::GainFreeRefill {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id,
                player_name: player_name.to_string(),
                roast: roast.to_string(),
                ingredient1: ingredient1.to_string(),
                ingredient2: ingredient2.to_string(),
                sub_event: sub_event.as_sub_event(),
                team_id: sub_event.next_team_id_opt(),
            }
        }
        EventType::CoffeeBean => {
            let (player_name, roast, notes, wired, gained_mod) = event.next_parse(parse_coffee_bean)?;
            let mut sub_event = event.next_child_any(&[EventType::AddedMod, EventType::ModChange, EventType::RemovedMod])?;
            let player_id = event.next_player_id()?;
            let prev_mod = if sub_event.event_type == EventType::ModChange {
                let mod_str = sub_event.metadata_str("to")?;
                // Check that the added mod matches what was parsed
                assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
                Some(sub_event.metadata_str("from")?)
            } else {
                let mod_str = sub_event.metadata_str("mod")?;
                // Check that the added mod matches what was parsed
                assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
                None
            };
            // The player ID should match in the sub event
            assert_eq!(player_id, sub_event.next_player_id()?);
            FedEventData::CoffeeBean {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id,
                player_name: player_name.to_string(),
                roast: roast.to_string(),
                notes: notes.to_string(),
                which_mod: if wired { CoffeeBeanMod::Wired } else { CoffeeBeanMod::Tired },
                gained_mod,
                sub_event: sub_event.as_sub_event(),
                team_id: sub_event.next_team_id_opt(),
                previous: prev_mod.map(|s| s.try_into()
                    .map_err(|_| FeedParseError::UnexpectedMetadataValue {
                        event_type: sub_event.event_type,
                        field: "from",
                        value: s.to_string(),
                    })
                ).transpose()?,
            }
        }
        EventType::FeedbackBlocked => {
            let (resisted_name, tangled_name) = event.next_parse(parse_feedback_blocked)?;
            let resisted_id = event.next_player_id()?;
            let tangled_id = event.next_player_id()?;
            let mut sub_event = event.next_child(EventType::PlayerStatDecrease)?;

            FedEventData::FeedbackBlocked {
                game: event.game(unscatter, attractor_secret_base)?,
                resisted_id,
                resisted_name: resisted_name.to_string(),
                tangled_id,
                tangled_team_id: sub_event.next_team_id()?,
                tangled_name: tangled_name.to_string(),
                tangled_rating_before: sub_event.metadata_f64("before")?,
                tangled_rating_after: sub_event.metadata_f64("after")?,
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::FeedbackSwap => {
            let (player1_name, player2_name, position) = event.next_parse(parse_feedback)?;
            let sub_event = event.next_child(EventType::PlayerTraded)?;

            macro_rules! get_player_data {
                ($event:ident, $prefix:literal, $expected_name:ident) => {
                    {
                        let team_nickname = sub_event.metadata_str(concat!($prefix, "TeamName"))?.to_string();
                        assert!(is_known_team_nickname(&team_nickname));
                        let player_name = sub_event.metadata_str(concat!($prefix, "PlayerName"))?.to_string();
                        assert_eq!(player_name, $expected_name);
                        FeedbackPlayerData {
                            team_id: sub_event.metadata_uuid(concat!($prefix, "TeamId"))?,
                            team_nickname,
                            player_id: sub_event.metadata_uuid(concat!($prefix, "PlayerId"))?,
                            player_name,
                            location: sub_event.metadata_i64(concat!($prefix, "Location"))?.try_into()?,
                        }
                    }
                };
            }

            FedEventData::Feedback {
                game: event.game(unscatter, attractor_secret_base)?,
                players: (
                    get_player_data!(sub_event, "a", player1_name),
                    get_player_data!(sub_event, "b", player2_name),
                ),
                position_type: position,
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::SuperallergicReaction => { todo!() }
        EventType::AllergicReaction => {
            let player_name = event.next_parse(parse_allergic_reaction)?;
            let player_id = event.next_player_id()?;
            let mut sub_event = event.next_child(EventType::PlayerStatDecrease)?;
            assert_eq!(player_id, sub_event.next_player_id()?);
            FedEventData::AllergicReaction {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                player_id,
                player_name: player_name.to_string(),
                sub_event: sub_event.as_sub_event(),
                rating_before: sub_event.metadata_f64("before")?,
                rating_after: sub_event.metadata_f64("after")?,
            }
        }
        EventType::ReverbBestowsReverberating => {
            let player_name = event.next_parse(parse_bestow_reverberating)?;
            let player_id = event.next_player_id()?;
            let mut sub_event = event.next_child(EventType::AddedMod)?;
            assert_eq!(player_id, sub_event.next_player_id()?);
            FedEventData::BestowReverberating {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                player_id,
                player_name: player_name.to_string(),
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::ReverbRosterShuffle => {
            let (team_nickname, reverb_type, gravity_player_names) = event.next_parse(parse_roster_shuffle)?;

            let gravity_players = gravity_player_names.into_iter()
                .map(|player_name| {
                    ParseOk(PlayerInfo {
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                    })
                })
                .collect::<Result<_, _>>()?;

            match reverb_type {
                ParsedReverbType::Rotation => {
                    let mut sub_event = event.next_child(EventType::ReverbRotationShuffle)?;
                    FedEventData::Reverb {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Rotation(sub_event.as_sub_event()),
                        gravity_players,
                    }
                }
                ParsedReverbType::Lineup => {
                    let mut sub_event = event.next_child(EventType::ReverbLineupShuffle)?;
                    FedEventData::Reverb {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Lineup(sub_event.as_sub_event()),
                        gravity_players,
                    }
                }
                ParsedReverbType::Full => {
                    let mut sub_event = event.next_child(EventType::ReverbFullShuffle)?;
                    FedEventData::Reverb {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Full(sub_event.as_sub_event()),
                        gravity_players,
                    }
                }
                ParsedReverbType::SeveralPlayers => {
                    let mut reverbs = Vec::new();
                    let mut team_id = None;
                    while let Some(mut child) = event.next_child_opt(EventType::PlayerSwap)? {
                        reverbs.push(PlayerReverb {
                            first_player_id: child.metadata_uuid("aPlayerId")?,
                            first_player_name: child.metadata_str("aPlayerName")?.to_string(),
                            first_player_new_location: child.metadata_enum("aLocation")?,
                            second_player_id: child.metadata_uuid("bPlayerId")?,
                            second_player_name: child.metadata_str("bPlayerName")?.to_string(),
                            second_player_new_location: child.metadata_enum("bLocation")?,
                            sub_event: child.as_sub_event(),
                        });
                        if let Some(team_id) = team_id {
                            // TODO: Make this a Result
                            assert_eq!(team_id, child.next_team_id()?);
                        } else {
                            team_id = Some(child.next_team_id()?);
                        }
                    }
                    FedEventData::Reverb {
                        game: event.game(unscatter, attractor_secret_base)?,
                        // TODO Turn this Expect into a Result
                        team_id: team_id.expect("There must be at least one child to set the team id"),
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::SeveralPlayers(reverbs),
                        gravity_players,
                    }
                }
            }
        }
        EventType::Blooddrain => {
            let (sipper_name, sipped_name, sipped_category) = event.next_parse(parse_blooddrain)?;
            let sipper_id = event.next_player_id()?;
            let sipped_id = event.next_player_id()?;

            let mut sipped_event = event.next_child(EventType::PlayerStatDecrease)?;
            let mut sipper_event = event.next_child(EventType::PlayerStatIncrease)?;

            FedEventData::Blooddrain {
                game: event.game(unscatter, attractor_secret_base)?,
                is_siphon: false,
                sipper: PlayerStatChange {
                    team_id: sipper_event.next_team_id()?,
                    player_id: sipper_id,
                    player_name: sipper_name.to_string(),
                    rating_before: sipper_event.metadata_f64("before")?,
                    rating_after: sipper_event.metadata_f64("after")?,
                    sub_event: sipper_event.as_sub_event(),
                },
                sipped: PlayerStatChange {
                    team_id: sipped_event.next_team_id()?,
                    player_id: sipped_id,
                    player_name: sipped_name.to_string(),
                    rating_before: sipped_event.metadata_f64("before")?,
                    rating_after: sipped_event.metadata_f64("after")?,
                    sub_event: sipped_event.as_sub_event(),
                },
                sipped_category,
            }
        }
        EventType::BlooddrainSiphon => {
            let (sipper_name, sipped_name, sipped_category, action) = event.next_parse(parse_blooddrain_siphon)?;

            match action {
                None => {
                    let mut sipped_event = event.next_child(EventType::PlayerStatDecrease)?;
                    let mut sipper_event = event.next_child(EventType::PlayerStatIncrease)?;
                    let sipper_id = event.next_player_id()?;
                    let sipped_id = event.next_player_id()?;

                    FedEventData::Blooddrain {
                        game: event.game(unscatter, attractor_secret_base)?,
                        is_siphon: true,
                        sipper: PlayerStatChange {
                            team_id: sipper_event.next_team_id()?,
                            player_id: sipper_id,
                            player_name: sipper_name.to_string(),
                            rating_before: sipper_event.metadata_f64("before")?,
                            rating_after: sipper_event.metadata_f64("after")?,
                            sub_event: sipper_event.as_sub_event(),
                        },
                        sipped: PlayerStatChange {
                            team_id: sipped_event.next_team_id()?,
                            player_id: sipped_id,
                            player_name: sipped_name.to_string(),
                            rating_before: sipped_event.metadata_f64("before")?,
                            rating_after: sipped_event.metadata_f64("after")?,
                            sub_event: sipped_event.as_sub_event(),
                        },
                        sipped_category,
                    }
                }
                Some(action) => {
                    let mut stat_decrease_event = event.next_child(EventType::PlayerStatDecrease)?;
                    // These are in the opposite order for normal vs special blooddrains! fun!
                    let sipper_id = event.next_player_id()?;
                    let sipped_id = event.next_player_id()?;
                    FedEventData::SpecialBlooddrain {
                        game: event.game(unscatter, attractor_secret_base)?,
                        sipper_id,
                        sipped_team_id: stat_decrease_event.next_team_id()?,
                        sipper_name: sipper_name.to_string(),
                        sipped_id,
                        sipped_name: sipped_name.to_string(),
                        sipped_category,
                        action: match action {
                            ParsedBlooddrainAction::AddBall => { BlooddrainAction::AddBall }
                            ParsedBlooddrainAction::RemoveBall => { BlooddrainAction::RemoveBall }
                            ParsedBlooddrainAction::AddStrike(name) => { BlooddrainAction::AddStrike(name.map(|s| s.to_string())) }
                            ParsedBlooddrainAction::RemoveStrike => { BlooddrainAction::RemoveStrike }
                            ParsedBlooddrainAction::AddOut => { BlooddrainAction::AddOut }
                            ParsedBlooddrainAction::RemoveOut => { BlooddrainAction::RemoveOut }
                        },
                        sipped_event: stat_decrease_event.as_sub_event(),
                        rating_before: stat_decrease_event.metadata_f64("before")?,
                        rating_after: stat_decrease_event.metadata_f64("after")?,
                    }
                }
            }
        }
        EventType::BlooddrainBlocked => { todo!() }
        EventType::Incineration => {
            let (victim_name, replacement_name) = event.next_parse(parse_incineration)?;
            let mut incin_child = event.next_child(EventType::Incineration)?;
            let enter_hall_child = event.next_child(EventType::EnterHallOfFlame)?;
            let mut hatch_child = event.next_child(EventType::PlayerHatched)?;
            let replace_child = event.next_child(EventType::PlayerBornFromIncineration)?;

            let team_nickname = replace_child.metadata_str("teamName")?;
            assert!(is_known_team_nickname(team_nickname));
            FedEventData::Incineration {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: incin_child.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                victim_id: incin_child.next_player_id()?,
                victim_name: victim_name.to_string(),
                replacement_id: hatch_child.next_player_id()?,
                replacement_name: replacement_name.to_string(),
                location: replace_child.metadata_enum("location")?,
                sub_events: (
                    incin_child.as_sub_event(),
                    enter_hall_child.as_sub_event(),
                    hatch_child.as_sub_event(),
                    replace_child.as_sub_event(),
                ),
            }
        }
        EventType::IncinerationBlocked => {
            // For now I only support magmatic, that may have to change
            let (player_name, blocked_reason) = event.next_parse(parse_incineration_blocked)?;
            match blocked_reason {
                IncinerationBlockedReason::Magmatic => {
                    let mut sub_event = event.next_child(EventType::AddedMod)?;
                    FedEventData::BecameMagmatic {
                        game: event.game(unscatter, attractor_secret_base)?,
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                        team_id: sub_event.next_team_id()?,
                        mod_add_event: sub_event.as_sub_event(),
                    }
                }
                IncinerationBlockedReason::Fireproof => {
                    FedEventData::FireproofIncineration {
                        game: event.game(unscatter, attractor_secret_base)?,
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                    }
                }
            }
        }
        EventType::FlagPlanted => {
            let (team_nickname, park_name, prefab_name, is_first) = event.next_parse(parse_flag_planted)?;

            FedEventData::FlagPlanted {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                ballpark_name: park_name.to_string(),
                prefab_name: prefab_name.to_string(),
                renovation_id: event.metadata_str("renoId")?.to_string(),
                votes: event.metadata_i64("votes")?,
                is_first,
            }
        }
        EventType::RenovationBuilt => {
            // Funnily enough, fraudulent renos' make-good events have string values for the
            // metadata instead of ints.
            let is_fraudulent_reno_fix = event.metadata()
                .as_object()
                .and_then(|obj| obj.get("votes"))
                .ok_or_else(|| FeedParseError::MissingMetadata {
                    event_type: event.event_type,
                    field: "votes".to_string(),
                })?
                .is_string();

            // It may be valuable to parse which reno is built, but there isn't one unified syntax
            // so I'm not going to put in the work now. Contributions welcome.
            FedEventData::RenovationBuilt {
                team_id: event.next_team_id()?,
                description: event.description().to_string(),
                renovation_id: event.metadata_str("renoId")?.to_string(),
                renovation_title: event.metadata_str("title")?.to_string(),
                votes: if is_fraudulent_reno_fix {
                    RenovationVotes::Manual(event.metadata_str("votes")?.to_string())
                } else {
                    RenovationVotes::Normal(event.metadata_i64("votes")?)
                },
            }
        }
        EventType::LightSwitchToggled => { todo!() }
        EventType::DecreePassed => {
            let decree_title = event.next_parse(parse_decree_passed)?;

            FedEventData::DecreePassed {
                decree_title: decree_title.into(),
                metadata: event.full_metadata().clone(),
            }
        }
        EventType::BlessingOrGiftWon => {
            let blessing_title = event.next_parse(parse_blessing_won)?;

            FedEventData::BlessingWon {
                team_tags: event.team_tags().into(),
                blessing_title: blessing_title.into(),
                metadata: event.full_metadata().clone(),
            }
        }
        EventType::WillRecieved => {
            let will_title = event.next_parse(parse_will_received)?;

            FedEventData::WillReceived {
                team_id: event.next_team_id()?,
                will_title: will_title.to_string(),
                metadata: event.full_metadata().clone(),
            }
        }
        EventType::FloodingSwept => {
            let (parsed_effects, flood_pumps) = event.next_parse(parse_flooding_swept)?;

            let effects = parsed_effects.into_iter()
                .map(|effect| ParseOk(match effect {
                    ParsedFloodingEffect::Elsewhere(player_name) => {
                        let mut sub_event = event.next_child(EventType::AddedMod)?;

                        FloodingSweptEffect::Elsewhere(ModChangeSubEventWithNamedPlayer {
                            sub_event: sub_event.as_sub_event(),
                            team_id: sub_event.next_team_id()?,
                            player_id: sub_event.next_player_id()?,
                            player_name: player_name.to_string(),
                        })
                    }
                    ParsedFloodingEffect::Flippers(player_name) => {
                        FloodingSweptEffect::Flippers(PlayerInfo {
                            player_id: event.next_player_id()?,
                            player_name: player_name.to_string(),
                        })
                    }
                    ParsedFloodingEffect::Ego(player_name) => {
                        FloodingSweptEffect::Ego(PlayerInfo {
                            player_id: event.next_player_id()?,
                            player_name: player_name.to_string(),
                        })
                    }
                }))
                .collect::<Result<Vec<_>, _>>()?;

            let free_refills = event.parse_free_refills()?;

            FedEventData::FloodingSwept {
                game: event.game(unscatter, attractor_secret_base)?,
                effects,
                free_refills,
                flood_pumps,
            }
        }
        EventType::SalmonSwim => {
            let (inning_num, parsed_runs_lost) = event.next_parse(parse_salmon)?;
            let item_restored = event.next_parse_opt(parse_item_restored)
                .map(|(player_name, _item_name)| {
                    let mut child = event.next_child_any(&[EventType::BrokenItemRepaired, EventType::DamagedItemRepaired])?;
                    Ok::<_, FeedParseError>(ItemRepaired {
                        item_id: child.metadata_uuid("itemId")?,
                        item_name: child.metadata_str("itemName")?.to_string(),
                        item_mods: child.metadata_str_vec("mods")?.into_iter().map(|s| s.to_string()).collect(),
                        durability: child.metadata_i64("itemDurability")?,
                        health: child.metadata_i64("itemHealthAfter")?,
                        player_item_rating_before: child.metadata_f64("playerItemRatingBefore")?,
                        player_item_rating_after: child.metadata_f64("playerItemRatingAfter")?,
                        player_rating: child.metadata_f64("playerRating")?,
                        team_id: child.next_team_id()?,
                        player_id: child.next_player_id()?,
                        player_name: player_name.to_string(),
                        sub_event: child.as_sub_event(),
                    })
                })
                .transpose()?;

            let player_expelled = event.next_parse_opt(parse_caught_in_the_bind)
                .map(|player_name| {
                    let mut child = event.next_child(EventType::AddedMod)?;
                    Ok::<_, FeedParseError>(ModChangeSubEventWithNamedPlayer {
                        sub_event: child.as_sub_event(),
                        team_id: child.next_team_id()?,
                        player_id: child.next_player_id()?,
                        player_name: player_name.to_string(),
                    })
                })
                .transpose()?;

            FedEventData::SalmonSwim {
                game: event.game(unscatter, attractor_secret_base)?,
                inning_num,
                run_losses: match parsed_runs_lost {
                    ParsedSalmonRunsLost::None => { RunLossesFromSalmon::None }
                    ParsedSalmonRunsLost::OneTeam(ParsedTeamRunsLost { runs, name }) => {
                        RunLossesFromSalmon::OneTeam(TeamRunsLost { runs_lost: runs, team_name: name.to_string() })
                    }
                    ParsedSalmonRunsLost::BothTeams((a, b)) => {
                        RunLossesFromSalmon::BothTeams((
                            TeamRunsLost { runs_lost: a.runs, team_name: a.name.to_string() },
                            TeamRunsLost { runs_lost: b.runs, team_name: b.name.to_string() },
                        ))
                    }
                },
                item_restored,
                player_expelled,
            }
        }
        EventType::PolarityShift => { todo!() }
        EventType::EnterSecretBase => {
            let player_name = event.next_parse(parse_terminated(" enters the Secret Base..."))?;

            FedEventData::EnterSecretBase {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
            }
        }
        EventType::ExitSecretBase => {
            let player_name = event.next_parse(parse_terminated(" exits the Secret Base to Second Base!"))?;

            FedEventData::ExitSecretBase {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
            }
        }
        EventType::ConsumersAttack => {
            let (player_name, item_breaks, scattered) = event.next_parse(parse_consumer_attack)?;

            let (team_id, effect) = if item_breaks.is_some() {
                let mut break_child = event.next_child_any(&[EventType::ItemBreaks, EventType::ItemDamaged])?;
                let team_id = break_child.next_team_id()?;

                let item_breaks = ItemDamaged {
                    item_id: break_child.metadata_uuid("itemId")?,
                    item_name: break_child.metadata_str("itemName")?.to_string(),
                    item_mods: vec![],
                    durability: break_child.metadata_i64("itemDurability")?,
                    health: break_child.metadata_i64("itemHealthAfter")?,
                    player_item_rating_before: break_child.metadata_f64("playerItemRatingBefore")?,
                    player_item_rating_after: break_child.metadata_f64("playerItemRatingAfter")?,
                    player_rating: break_child.metadata_f64("playerRating")?,
                    team_id,
                    player_id: break_child.next_player_id()?,
                    sub_event: break_child.as_sub_event(),
                };

                (team_id, ConsumerAttackEffect::DefendedWithItem(item_breaks))
            } else {
                // I'm hoping that detectives only sense something fishy if the attack hit
                // TODO: If this is true, move the something fishy inside the effect
                let mut chomp_child = event.next_child(EventType::PlayerStatDecrease)?;
                let team_id = chomp_child.next_team_id()?;
                (team_id, ConsumerAttackEffect::Chomp {
                    rating_before: chomp_child.metadata_f64("before")?,
                    rating_after: chomp_child.metadata_f64("after")?,
                    sub_event: chomp_child.as_sub_event(),
                })
            };
            let sensed_something_fishy = event.next_child_if(EventType::InvestigationMessage, |_| true)?
                .map(|mut fishy_event| {
                    let detective_name = fishy_event.next_parse(parse_terminated(" sensed something fishy."))?;
                    ParseOk(DetectiveActivity {
                        detective_id: fishy_event.next_player_id()?,
                        detective_name: detective_name.to_string(),
                        sub_event: fishy_event.as_sub_event(),
                    })
                })
                .transpose()?;


            FedEventData::ConsumerAttack {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id,
                player_id: event.next_player_id()?,
                player_name_all_caps: player_name.to_string(),
                effect,
                sensed_something_fishy,
                scattered,
            }
        }
        EventType::EchoChamber => {
            let (player_name, which_mod) = event.next_parse(parse_echo_chamber)?;

            let mut child = event.next_child(EventType::AddedMod)?;
            FedEventData::EchoChamber {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: child.next_team_id()?,
                player_id: child.next_player_id()?,
                player_name: player_name.to_string(),
                which_mod,
                sub_event: child.as_sub_event(),
            }
        }
        EventType::GrindRail => {
            let (player_name, first_trick, success) = event.next_parse(parse_grind_rail)?;

            fn trick_from_parsed(parsed: ParsedGrindRailTrick) -> GrindRailTrick {
                GrindRailTrick {
                    trick_name: parsed.name.to_string(),
                    points: parsed.score,
                }
            }

            FedEventData::GrindRail {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
                first_trick: trick_from_parsed(first_trick),
                success: match success {
                    ParsedGrindRailSuccess::Safe(trick) => {
                        GrindRailSuccess::Safe(trick_from_parsed(trick))
                    }
                    ParsedGrindRailSuccess::TaggedOut(trick) => {
                        GrindRailSuccess::TaggedOut(trick_from_parsed(trick))
                    }
                    ParsedGrindRailSuccess::Bailed => {
                        GrindRailSuccess::Bailed
                    }
                },
            }
        }
        EventType::TunnelsUsed => { todo!() }
        EventType::PeanutMister => {
            let (player_name, cured_superallergy) = event.next_parse(parse_peanut_mister)?;

            let superallergy = if cured_superallergy {
                let mut sub_event = event.next_child(EventType::RemovedMod)?;
                Some(ModChangeSubEvent {
                    sub_event: sub_event.as_sub_event(),
                    team_id: sub_event.next_team_id()?,
                })
            } else {
                None
            };

            FedEventData::PeanutMister {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
                superallergy,
            }
        }
        EventType::PeanutFlavorText => {
            FedEventData::PeanutFlavorText {
                game: event.game(unscatter, attractor_secret_base)?,
                message: event.description().into(),
            }
        }
        EventType::TasteTheInfinite => {
            let (sheller_name, shellee_name) = event.next_parse(parse_taste_the_infinite)?;
            let sheller_id = event.next_player_id()?;
            let shellee_id = event.next_player_id()?;

            let mut sub_event = event.next_child(EventType::AddedMod)?;
            FedEventData::TasteTheInfinite {
                game: event.game(unscatter, attractor_secret_base)?,
                sheller_id,
                sheller_name: sheller_name.to_string(),
                shellee_team_id: sub_event.next_team_id()?,
                shellee_id,
                shellee_name: shellee_name.to_string(),
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::EventHorizonActivation => { todo!() }
        EventType::EventHorizonAwaits => { todo!() }
        EventType::SolarPanelsAwait => {
            let _ = event.next_parse_tag("The Solar Panels are angled toward Sun 2.")?;
            FedEventData::SolarPanelsAwait {
                game: event.game(unscatter, attractor_secret_base)?,
            }
        }
        EventType::SolarPanelsActivation => {
            let (num_runs, team_nickname) = event.next_parse(parse_solar_panels)?;
            assert!(is_known_team_nickname(team_nickname));

            FedEventData::SolarPanelsActivate {
                game: event.game(unscatter, attractor_secret_base)?,
                num_runs,
                team_nickname: team_nickname.to_string(),
            }
        }
        EventType::TarotReading => {
            FedEventData::TarotReading {
                description: event.description().into(),
                metadata: event.metadata().clone(),
                player_tags: event.player_tags().into(),
                team_tags: event.team_tags().into(),
            }
        }
        EventType::EmergencyAlert => {
            FedEventData::EmergencyAlert {
                message: event.description().into(),
                team_tags: event.team_tags().into(),
            }
        }
        EventType::ReturnFromElsewhere => {
            let (player_name, flavor) = match event.next_parse(parse_return_from_elsewhere)? {
                ParsedReturnFromElsewhere::Normal((player_name, time_elsewhere)) => {
                    let scattered = event.next_child_if_mod_effect(EventType::AddedMod, "SCATTERED")?
                        .map(|mut scattered_sub_event| {
                            let scattered_name = scattered_sub_event.next_parse(parse_terminated(" was Scattered..."))?;

                            ParseOk(Scattered {
                                scattered_name: scattered_name.to_string(),
                                sub_event: scattered_sub_event.as_sub_event(),
                            })
                        })
                        .transpose()?;

                    let mut return_sub_event = event.next_child(EventType::RemovedMod)?;

                    let recongealed_differently = event.next_child_any_opt(&[EventType::PlayerStatIncrease, EventType::PlayerStatDecrease])?
                        .map(|mut child| {
                            let player_name = child.next_parse(parse_terminated(" re-congealed differently."))?;
                            Ok::<_, FeedParseError>(PlayerStatChange {
                                team_id: child.next_team_id()?,
                                player_id: child.next_player_id()?,
                                player_name: player_name.to_string(),
                                rating_before: child.metadata_f64("before")?,
                                rating_after: child.metadata_f64("after")?,
                                sub_event: child.as_sub_event(),
                            })
                        })
                        .transpose()?;

                    (player_name, ReturnFromElsewhereFlavor::Full {
                        team_id: return_sub_event.next_team_id()?,
                        player_id: return_sub_event.next_player_id()?,
                        sub_event: return_sub_event.as_sub_event(),
                        time_elsewhere,
                        scattered,
                        recongealed_differently,
                    })
                }
                ParsedReturnFromElsewhere::Short(player_name) => {
                    if let Some(mut return_sub_event) = event.next_child_if_mod_effect(EventType::RemovedMod, "ELSEWHERE")? {
                        (player_name, ReturnFromElsewhereFlavor::Short {
                            team_id: return_sub_event.next_team_id()?,
                            player_id: return_sub_event.next_player_id()?,
                            sub_event: return_sub_event.as_sub_event(),
                        })
                    } else {
                        (player_name, ReturnFromElsewhereFlavor::False)
                    }
                }
            };

            FedEventData::ReturnFromElsewhere {
                game: event.game(unscatter, attractor_secret_base)?,
                player_name: player_name.to_string(),
                flavor,
            }
        }
        EventType::OverUnder => {
            let (player_name, on) = event.next_parse(parse_under_over_over_under("Over Under"))?;

            let mut sub_event = event.next_child(if on {
                EventType::AddedModFromOtherMod
            } else {
                EventType::RemovedModFromOtherMod
            })?;
            FedEventData::OverUnder {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                player_id: sub_event.next_player_id()?,
                player_name: player_name.to_string(),
                on,
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::UnderOver => {
            let (player_name, on) = event.next_parse(parse_under_over_over_under("Under Over"))?;

            let mut sub_event = event.next_child(if on {
                EventType::AddedModFromOtherMod
            } else {
                EventType::RemovedModFromOtherMod
            })?;
            FedEventData::UnderOver {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                player_id: sub_event.next_player_id()?,
                player_name: player_name.to_string(),
                on,
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::Undersea => {
            let team_name = event.next_parse(parse_undersea)?;
            assert!(is_known_team_name(team_name));

            let mut mod_add_event = event.next_child(EventType::AddedModFromOtherMod)?;

            FedEventData::Undersea {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: mod_add_event.next_team_id()?,
                team_name: team_name.to_string(),
                sub_event: mod_add_event.as_sub_event(),
            }
        }
        EventType::Homebody => {
            let players = event.next_parse(parse_homebody)?;

            let homebodies = players.into_iter()
                .map(|(player_name, is_overperforming)| {
                    let mut mod_add_event = event.next_child_any(&[EventType::AddedModFromOtherMod, EventType::ChangedModFromOtherMod])?;
                    ParseOk(TogglePerforming {
                        player_id: mod_add_event.next_player_id()?,
                        team_id: mod_add_event.next_team_id()?,
                        player_name: player_name.to_string(),
                        is_overperforming,
                        is_first_proc: mod_add_event.event_type == EventType::AddedModFromOtherMod,
                        sub_event: mod_add_event.as_sub_event(),
                    })
                })
                .collect::<Result<_, _>>()?;

            FedEventData::HomebodyGameStart {
                game: event.game(unscatter, attractor_secret_base)?,
                homebodies,
            }
        }
        EventType::Superyummy => {
            let (player_name, peanuts_present) = event.next_parse(parse_superyummy)?;

            let expected_types = [EventType::AddedModFromOtherMod, EventType::ChangedModFromOtherMod];
            if let Some(mut mod_add_event) = event.next_child_if_any(&expected_types, |child| {
                expected_types.iter().any(|t| t == &child.event_type)
            })? {
                FedEventData::SuperyummyGameStart {
                    game: event.game(unscatter, attractor_secret_base)?,
                    toggle: TogglePerforming {
                        player_name: player_name.to_string(),
                        is_overperforming: peanuts_present,
                        is_first_proc: mod_add_event.event_type == EventType::AddedModFromOtherMod,
                        sub_event: mod_add_event.as_sub_event(),
                        player_id: mod_add_event.next_player_id()?,
                        team_id: mod_add_event.next_team_id()?,
                    },
                }
            } else {
                // Then this must have come from an Echoed Superyummy
                FedEventData::EchoedSuperyummyGameStart {
                    game: event.game(unscatter, attractor_secret_base)?,
                    player_name: player_name.to_string(),
                    peanuts_present,
                }
            }
        }
        EventType::Perk => {
            let player_names = event.next_parse(parse_perk_up)?;

            let players = player_names.into_iter()
                .map(|player_name| {
                    let mut mod_add_event = event.next_child(EventType::AddedModFromOtherMod)?;
                    assert_eq!(format!("{player_name} Perks up."), mod_add_event.description());
                    ParseOk(ModChangeSubEventWithNamedPlayer {
                        player_name: player_name.to_string(),
                        sub_event: mod_add_event.as_sub_event(),
                        player_id: mod_add_event.next_player_id()?,
                        team_id: mod_add_event.next_team_id()?,
                    })
                })
                .collect::<Result<_, _>>()?;

            FedEventData::PerkUp {
                game: event.game(unscatter, attractor_secret_base)?,
                players,
            }
        }
        EventType::Earlbird => {
            match event.next_parse(parse_earlbird)? {
                EarlbirdsChange::Added(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    let mut sub_event = event.next_child(EventType::AddedModFromOtherMod)?;
                    FedEventData::EarlbirdsAdded {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        sub_event: sub_event.as_sub_event(),
                    }
                }
                EarlbirdsChange::Removed => {
                    let mut sub_event = event.next_child(EventType::RemovedModFromOtherMod)?;
                    FedEventData::EarlbirdsRemoved {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.next_team_id()?,
                        sub_event: sub_event.as_sub_event(),
                    }
                }
            }
        }
        EventType::LateToTheParty => {
            match event.next_parse(parse_late_to_the_party)? {
                LateToThePartyChange::Added(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    let mut sub_event = event.next_child_if_mod_effect(EventType::AddedModFromOtherMod, "OVERPERFORMING")?;
                    FedEventData::LateToThePartyAdded {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_id: sub_event.as_mut().map(|e| e.next_team_id()).transpose()?,
                        team_nickname: team_nickname.to_string(),
                        sub_event: sub_event.map(|e| e.as_sub_event()),
                    }
                }
                LateToThePartyChange::Removed(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    FedEventData::LateToThePartyRemoved {
                        game: event.game(unscatter, attractor_secret_base)?,
                        team_nickname: team_nickname.to_string(),
                    }
                }
            }
        }
        EventType::ShameDonor => { todo!() }
        EventType::AddedMod => {
            if TAROT_EVENTS.iter().any(|uuid| uuid == &event.id) {
                // Then it's a tarot event and we can forget parsing. Thankfully
                make_tarot_event(&mut event, false)?
            } else {
                match event.next_parse(parse_added_mod)? {
                    ParsedAddedMod::EnteredPartyTime(team_nickname) => {
                        assert!(is_known_team_nickname(team_nickname));
                        FedEventData::TeamEnteredPartyTime {
                            team_id: event.next_team_id()?,
                            team_nickname: team_nickname.to_string(),
                        }
                    }
                    ParsedAddedMod::GainFreeWill(team_nickname) => {
                        assert!(is_known_team_nickname(team_nickname));
                        FedEventData::TeamGainedFreeWill {
                            team_id: event.next_team_id()?,
                            team_nickname: team_nickname.to_string(),
                        }
                    }
                    ParsedAddedMod::MVP(player_name) => {
                        FedEventData::PlayerNamedMvp {
                            team_id: event.next_team_id()?,
                            player_id: event.next_player_id()?,
                            player_name: player_name.to_string(),
                            level: 1,
                        }
                    }
                }
            }
        }
        EventType::RemovedMod => {
            if TAROT_EVENTS.iter().any(|uuid| uuid == &event.id) {
                // Then it's a tarot event and we can forget parsing. Thankfully
                make_tarot_event(&mut event, true)?
            } else {
                match event.next_parse(parse_removed_mod)? {
                    ParsedRemovedMod::TeamRemovedFromPartyTimeForPostseason(team_nickname) => {
                        assert!(is_known_team_nickname(team_nickname));
                        FedEventData::TeamLeftPartyTimeForPostseason {
                            team_id: event.next_team_id()?,
                            team_nickname: team_nickname.to_string(),
                        }
                    }
                    ParsedRemovedMod::TeamUsedFreeWill(team_nickname) => {
                        assert!(is_known_team_nickname(team_nickname));
                        FedEventData::TeamUsedFreeWill {
                            team_id: event.next_team_id()?,
                            team_nickname: team_nickname.to_string(),
                        }
                    }
                    ParsedRemovedMod::PlayerLostMod((player_name, mod_name)) => {
                        FedEventData::PlayerLostMod {
                            team_id: event.next_team_id()?,
                            player_id: event.next_player_id()?,
                            player_name: player_name.to_string(),
                            r#mod: event.metadata_str("mod")?.to_string(),
                            mod_name: mod_name.to_string(),
                        }
                    }
                    ParsedRemovedMod::InvestigationConcluded(stadium_name) => {
                        FedEventData::InvestigationConcluded {
                            team_id: event.next_team_id()?,
                            stadium_name: stadium_name.to_string(),
                        }
                    }
                }
            }
        }
        EventType::ModExpires => {
            let mods = event.metadata_str_vec("mods")?
                .into_iter().map(String::from).collect();
            if let Some(player_id) = event.next_player_id_opt() {
                let (player_name, mod_duration) = event.next_parse(parse_player_mod_expires)?;
                FedEventData::PlayerModExpires {
                    team_id: event.next_team_id()?,
                    player_id,
                    player_name: player_name.to_string(),
                    mods,
                    mod_duration,
                }
            } else {
                let (team_nickname, mod_duration) = event.next_parse(parse_team_mod_expires)?;
                assert!(is_known_team_nickname(team_nickname));
                FedEventData::TeamModExpires {
                    team_id: event.next_team_id()?,
                    team_nickname: team_nickname.to_string(),
                    mods,
                    mod_duration,
                }
            }
        }
        EventType::PlayerAddedToTeam => {
            match event.next_parse(parse_player_added_to_team)? {
                ParsedPlayerAddedToTeam::PostseasonBirth(team_nickname) => {
                    FedEventData::PostseasonBirth {
                        team_id: event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        player_id: event.next_player_id()?,
                        player_name: event.metadata_str("playerName")?.to_string(),
                        location: event.metadata_enum("location")?,
                    }
                }
                ParsedPlayerAddedToTeam::Localized { player_name, team_nickname, .. } => {
                    // TODO Check location from parsing against location from metadata
                    FedEventData::PlayerLocalized {
                        team_id: event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                        location: event.metadata_enum("location")?,
                    }
                }
            }
        }
        EventType::PlayerReplacedByNecromancy => { todo!() }
        EventType::PlayerReplacesReturned => {
            let team_nickname = event.next_parse(parse_player_replaces_returned)?;

            FedEventData::ReplaceReturnedPlayerFromShadows {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                promoted_player_id: event.metadata_uuid("promotePlayerId")?,
                promoted_player_name: event.metadata_str("promotePlayerName")?.to_string(),
                promoted_location: event.metadata_enum("promoteLocation")?,
                removed_player_id: event.metadata_uuid("removePlayerId")?,
                removed_player_name: event.metadata_str("removePlayerName")?.to_string(),
                removed_location: event.metadata_enum("removeLocation")?,
            }
        }
        EventType::PlayerRemovedFromTeam => { todo!() }
        EventType::PlayerTraded => { todo!() }
        EventType::PlayerSwap => { todo!() }
        EventType::PlayerMoved => {
            match event.next_parse(parse_player_moved)? {
                ParsedPlayerMoved::ReturnFromInvestigation((_player_name, emptyhanded)) => {
                    FedEventData::ReturnFromInvestigation {
                        player_id: event.metadata_uuid("playerId")?,
                        player_name: event.metadata_str("playerName")?.to_string(),
                        previous_team_id: event.metadata_uuid("sendTeamId")?,
                        previous_team_name: event.metadata_str("sendTeamName")?.to_string(),
                        new_location: event.metadata_enum("receiveLocation")?,
                        new_team_id: event.metadata_uuid("receiveTeamId")?,
                        new_team_name: event.metadata_str("receiveTeamName")?.to_string(),
                        emptyhanded,
                    }
                }
                ParsedPlayerMoved::Roamin(_player_name) => {
                    FedEventData::Roam {
                        player_id: event.metadata_uuid("playerId")?,
                        player_name: event.metadata_str("playerName")?.to_string(),
                        location: event.metadata_enum("location")?,
                        previous_team_id: event.metadata_uuid("sendTeamId")?,
                        previous_team_nickname: event.metadata_str("sendTeamName")?.to_string(),
                        new_team_id: event.metadata_uuid("receiveTeamId")?,
                        new_team_nickname: event.metadata_str("receiveTeamName")?.to_string(),
                    }
                }
            }
        }
        EventType::PlayerBornFromIncineration => { todo!() }
        EventType::PlayerStatIncrease => {
            match event.next_parse(parse_player_stat_increase)? {
                ParsedPlayerStatIncrease::PlayerBoosted(player_name) => {
                    FedEventData::PlayerBoosted {
                        team_id: event.next_team_id()?,
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                        rating_before: event.metadata_f64("before")?,
                        rating_after: event.metadata_f64("after")?,
                    }
                }
                ParsedPlayerStatIncrease::BottomDwellers(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));
                    FedEventData::BottomDwellers {
                        team_id: event.next_team_id()?,
                        team_nickname: team_nickname.to_string(),
                        rating_before: event.metadata_f64("before")?,
                        rating_after: event.metadata_f64("after")?,
                    }
                }
            }
        }
        EventType::PlayerStatDecrease => { todo!() }
        EventType::PlayerStatReroll => { todo!() }
        EventType::PlayerStatDecreaseFromSuperallergic => { todo!() }
        EventType::PlayerMoveFailedForce => { todo!() }
        EventType::EnterHallOfFlame => {
            // In Beta, this event type is only top-level for return-to-hall events. That was no
            // longer true in Short Circuits.
            assert_eq!(event.sim, "thisidisstaticyo");

            let player_name = event.next_parse(parse_terminated(" entered the Hall of Flame."))?;

            FedEventData::PlayerCalledBackToHall {
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
            }
        }
        EventType::ExitHallOfFlame => { todo!() }
        EventType::PlayerGainedItem => { todo!() }
        EventType::PlayerLostItem => { todo!() }
        EventType::ReverbFullShuffle => { todo!() }
        EventType::ReverbLineupShuffle => { todo!() }
        EventType::ReverbRotationShuffle => { todo!() }
        EventType::PlayerHatched => {
            // For now this only has the breach events, it will need to be updated for s24
            let player_name = event.next_parse(parse_player_hatched)?;

            FedEventData::PlayerHatched {
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
            }
        }
        EventType::PlayerEvolves => { todo!() }
        EventType::TeamDivisionMove => {
            // For now this only has the breach events, it will need to be updated for s24
            let (team_nickname, division_name) = event.next_parse(parse_team_division_move)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(team_nickname, event.metadata_str("teamName")?);
            assert_eq!(division_name, event.metadata_str("divisionName")?);
            let team_id = event.next_team_id()?;
            assert_eq!(team_id, event.metadata_uuid("teamId")?);

            FedEventData::TeamJoinedILB {
                team_id,
                team_nickname: team_nickname.to_string(),
                division_id: event.metadata_uuid("divisionId")?,
                division_name: division_name.to_string(),
            }
        }
        EventType::PlayerDivisionMove => {
            match event.next_parse(parse_player_division_move)? {
                ParsedPlayerDivisionMove::JoinedIlb(player_name) => {
                    FedEventData::PlayerJoinedILB {
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                    }
                }
                ParsedPlayerDivisionMove::PulledThroughRift(player_name) => {
                    FedEventData::PlayerPulledThroughRift {
                        player_id: event.next_player_id()?,
                        player_name: player_name.to_string(),
                    }
                }
            }
        }
        EventType::TeamWonInternetSeries => {
            let (team_nickname, season_num) = event.next_parse(parse_team_won_internet_series)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(season_num, event.season + 1);

            FedEventData::TeamWonInternetSeries {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                championships: event.metadata_i64("championships")?,
            }
        }
        EventType::EarnedPostseasonSlot => {
            let (team_nickname, season_num) = event.next_parse(parse_earned_postseason_slot)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(season_num, event.season + 1);

            FedEventData::EarnedPostseasonSlot {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
            }
        }
        EventType::FinalStandings => {
            let (team_nickname, place, division_name) = event.next_parse(parse_final_standings)?;
            assert!(is_known_team_nickname(team_nickname));

            FedEventData::FinalStandings {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                place,
                division_name: division_name.to_string(),
            }
        }
        EventType::ModChange => {
            // This is only a top-level event for MVPs
            let (player_name, level) = event.next_parse(parse_repeat_mvp)?;

            FedEventData::PlayerNamedMvp {
                team_id: event.next_team_id()?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
                level,
            }
        }
        EventType::PlayerAlternated => { todo!() }
        EventType::AddedModFromOtherMod => { todo!() }
        EventType::ChangedModFromOtherMod => { todo!() }
        EventType::NecromancyOrPlunderNarration => { todo!() }
        EventType::PlayerPermittedToStay => {
            let player_name = event.next_parse(parse_terminated(" has been permitted to stay."))?;

            FedEventData::PlayerPermittedToStay {
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
            }
        }
        EventType::DecreeNarration => { todo!() }
        EventType::WillResults => { todo!() }
        EventType::TeamStatAdjustment => { todo!() }
        EventType::TeamWasShamed => {
            let (shaming_team, shamed_team) = event.next_parse(parse_team_was_shamed)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            FedEventData::TeamWasShamed {
                shamed_team_id: event.next_team_id()?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: event.metadata_i64("totalShames")?,
                total_shamings: event.metadata_i64("totalShamings")?,
            }
        }
        EventType::TeamDidShame => {
            let (shaming_team, shamed_team) = event.next_parse(parse_team_did_shame)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            FedEventData::TeamDidShame {
                shaming_team_id: event.next_team_id()?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: event.metadata_i64("totalShames")?,
                total_shamings: event.metadata_i64("totalShamings")?,
            }
        }
        EventType::Echo => {
            // This could be written better with the new interface but I'm just doing a
            // straightforward transformation for now. It was hard enough to write once.
            let (echoer_name, echoee_name) = event.next_parse(parse_echo)?;
            let first_remove_mods_event = event.next_child_opt(EventType::RemovedModsFromAnotherMod)?;
            let first_add_mods_event = event.next_child(EventType::AddedModsFromAnotherMod)?;
            let main_echo_event = (first_remove_mods_event, first_add_mods_event);

            let mut sub_echo_events = Vec::new();
            loop {
                let remove_mods_event = event.next_child_opt(EventType::RemovedModsFromAnotherMod)?;
                let add_mods_event = event.next_child_opt(EventType::AddedModsFromAnotherMod)?;

                if let Some(add_mods_event) = add_mods_event {
                    sub_echo_events.push((remove_mods_event, add_mods_event))
                } else {
                    break;
                }
            }

            let parse_str = format!("'s Echoed an Echo from {echoer_name}!");
            let sub_echos = sub_echo_events.into_iter()
                .map(|(removed, mut added)| {
                    let echoer_name = added.next_parse(parse_terminated(&parse_str))?;
                    make_echo(echoer_name, (removed, added))
                })
                .collect::<Result<_, _>>()?;

            FedEventData::Echo {
                game: event.game(unscatter, attractor_secret_base)?,
                echoee_name: echoee_name.to_string(),
                primary_echo: make_echo(echoer_name, main_echo_event)?,
                receiver_echos: sub_echos,
            }
        }
        EventType::EchoIntoStatic => {
            let (echoer_name, echoee_name) = event.next_parse(parse_echo_into_static)?;
            let echoer_removed = event.next_child(EventType::PlayerRemovedFromTeam)?;
            let echoee_removed = event.next_child(EventType::PlayerRemovedFromTeam)?;
            let echoer_mod_change = event.next_child(EventType::ModChange)?;
            let echoee_mod_change = event.next_child(EventType::ModChange)?;


            let make_echo_into_static = |name: &str, removed_event: EventParseWrapper, mod_change_event: EventParseWrapper| {
                let nickname = removed_event.metadata_str("teamName")?;
                assert!(is_known_team_nickname(nickname));
                ParseOk(EchoIntoStatic {
                    team_id: removed_event.metadata_uuid("teamId")?,
                    team_nickname: nickname.to_string(),
                    player_id: removed_event.metadata_uuid("playerId")?,
                    player_name: name.to_string(),
                    removed_from_team_sub_event: removed_event.as_sub_event(),
                    mod_changed_sub_event: mod_change_event.as_sub_event(),
                })
            };

            FedEventData::EchoIntoStatic {
                game: event.game(unscatter, attractor_secret_base)?,
                echoer: make_echo_into_static(echoer_name, echoer_removed, echoer_mod_change)?,
                echoee: make_echo_into_static(echoee_name, echoee_removed, echoee_mod_change)?,
            }
        }
        EventType::AddedModsFromAnotherMod => { todo!() }
        EventType::RemovedModsFromAnotherMod => {
            let (player_name, mod_name) = event.next_parse(parse_mods_from_other_mod_removed)?;

            let mods_removed = event.get_metadata("removes")?
                .as_array()
                .ok_or_else(|| {
                    FeedParseError::MetadataTypeError {
                        event_type: event.event_type,
                        field: "removes".to_string(),
                        ty: "array",
                    }
                })?
                .iter()
                .enumerate()
                .map(|(i, removes)| {
                    let obj = removes.as_object()
                        .ok_or_else(|| {
                            FeedParseError::MetadataTypeError {
                                event_type: event.event_type,
                                field: format!("removes[{i}]"),
                                ty: "object",
                            }
                        })?;

                    let mod_id = obj.get("mod")
                        .ok_or_else(|| {
                            FeedParseError::MissingMetadata {
                                event_type: event.event_type,
                                field: format!("removes[{i}].mod"),
                            }
                        })?
                        .as_str()
                        .ok_or_else(|| {
                            FeedParseError::MetadataTypeError {
                                event_type: event.event_type,
                                field: format!("removes[{i}].mod"),
                                ty: "str",
                            }
                        })?
                        .to_string();

                    let mod_duration = obj.get("type")
                        .ok_or_else(|| {
                            FeedParseError::MissingMetadata {
                                event_type: event.event_type,
                                field: format!("removes[{i}].type"),
                            }
                        })?
                        .as_i64()
                        .ok_or_else(|| {
                            FeedParseError::MetadataTypeError {
                                event_type: event.event_type,
                                field: format!("removes[{i}].type"),
                                ty: "i64",
                            }
                        })?
                        .try_into()
                        .map_err(|err: <i64 as TryInto<ModDuration>>::Error| {
                            FeedParseError::MetadataIntToEnumError {
                                event_type: event.event_type,
                                field: format!("removes[{i}].type"),
                                err: err.to_string(),
                            }
                        })?;

                    ParseOk(ModDesc { mod_id, mod_duration })
                })
                .collect::<Result<Vec<_>, _>>()?;

            FedEventData::ModsFromAnotherModRemoved {
                team_id: event.next_team_id()?,
                player_id: event.next_player_id()?,
                player_name: player_name.to_string(),
                mods_removed,
                source_mod_name: mod_name.to_string(),
                source_mod_id: event.metadata_str("source")?.to_string(),
            }
        }
        EventType::Psychoacoustics => {
            // For some reason the description on the main event is empty and the description is
            // only on the child event
            let mut child = event.next_child(EventType::AddedModFromOtherMod)?;
            // They changed the format slightly in the middle of s16
            let (stadium_name, mod_name, team_nickname) = child.next_parse(parse_psychoacoustics((event.season, event.day) < (15, 33)))?;
            assert!(is_known_team_nickname(team_nickname));
            FedEventData::Psychoacoustics {
                game: event.game(unscatter, attractor_secret_base)?,
                stadium_name: stadium_name.to_string(),
                team_id: child.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                mod_name: mod_name.to_string(),
                mod_id: child.metadata_str("mod")?.to_string(),
                sub_event: child.as_sub_event(),
            }
        }
        EventType::EchoReciever => {
            let (echoer_name, echoee_name) = event.next_parse(parse_echo_receiver)?;

            let mut child = event.next_child(EventType::ModChange)?;
            FedEventData::EchoReceiver {
                game: event.game(unscatter, attractor_secret_base)?,
                echoer_name: echoer_name.to_string(),
                echoee_name: echoee_name.to_string(),
                echoee_id: child.next_player_id()?,
                echoee_team_id: child.next_team_id()?,
                sub_event: child.as_sub_event(),
            }
        }
        EventType::InvestigationMessage => {
            FedEventData::InvestigationMessage {
                player_id: event.next_player_id()?,
                message: event.description().into(),
            }
        }
        EventType::Tidings => {
            FedEventData::Tidings {
                message: event.description().into(),
                metadata: event.full_metadata().clone(),
                player_tags: event.player_tags().into(),
            }
        }
        EventType::GlitterCrateDrop => {
            let (player_name, _gained_item_name, lost_item_name) = event.next_parse(parse_glitter)?;

            // Drop event is first in the data
            let dropped_item = lost_item_name
                .map(|(_name, item_was_broken)| {
                    let drop_event = event.next_child(EventType::PlayerLostItem)?;
                    Ok::<_, FeedParseError>(ItemDroppedForNewItem {
                        item_id: drop_event.metadata_uuid("itemId")?,
                        item_name: drop_event.metadata_str("itemName")?.to_string(),
                        item_mods: drop_event.metadata_str_vec("mods")?.into_iter().map(|s| s.to_string()).collect(),
                        player_item_rating_before: drop_event.metadata_f64("playerItemRatingBefore")?,
                        player_item_rating_after: drop_event.metadata_f64("playerItemRatingAfter")?,
                        item_was_broken,
                        sub_event: drop_event.as_sub_event(),
                    })
                })
                .transpose()?;

            let mut gain_event = event.next_child(EventType::PlayerGainedItem)?;
            let gained_item = ItemGained {
                item_id: gain_event.metadata_uuid("itemId")?,
                item_name: gain_event.metadata_str("itemName")?.to_string(),
                item_mods: gain_event.metadata_str_vec("mods")?.into_iter().map(|s| s.to_string()).collect(),
                player_item_rating_before: gain_event.metadata_f64("playerItemRatingBefore")?,
                player_item_rating_after: gain_event.metadata_f64("playerItemRatingAfter")?,
                player_rating: gain_event.metadata_f64("playerRating")?,
                team_id: gain_event.next_team_id()?,
                player_id: gain_event.next_player_id()?,
                sub_event: gain_event.as_sub_event(),
                dropped_item,
            };

            FedEventData::GlitterCrate {
                game: event.game(unscatter, attractor_secret_base)?,
                player_name: player_name.to_string(),
                gained_item,
            }
        }
        EventType::Middling => {
            let (team_nickname, is_middling) = event.next_parse(parse_middling)?;
            assert!(is_known_team_nickname(team_nickname));

            let mut child = event.next_child(if is_middling {
                EventType::AddedModFromOtherMod
            } else {
                EventType::RemovedModFromOtherMod
            })?;
            FedEventData::Middling {
                game: event.game(unscatter, attractor_secret_base)?,
                team_nickname: team_nickname.to_string(),
                is_middling,
                change_event: ModChangeSubEvent {
                    sub_event: child.as_sub_event(),
                    team_id: child.next_team_id()?,
                },
            }
        }
        EventType::PlayerAttributeIncrease => { todo!() }
        EventType::PlayerAttributeDecrease => { todo!() }
        EventType::EnterCrimeScene => {
            let (_player_name, stadium_nickname) = event.next_parse(parse_enter_crime_scene)?;

            let crime_scene_event = event.next_child(EventType::PlayerMoved)?;
            let shadows_event = event.next_child(EventType::PlayerStatIncrease)?;

            FedEventData::EnterCrimeScene {
                game: event.game(unscatter, attractor_secret_base)?,
                player_id: crime_scene_event.metadata_uuid("playerId")?,
                player_name: crime_scene_event.metadata_str("playerName")?.to_string(),
                previous_team_id: crime_scene_event.metadata_uuid("sendTeamId")?,
                previous_team_name: crime_scene_event.metadata_str("sendTeamName")?.to_string(),
                previous_location: crime_scene_event.metadata_enum("location")?,
                new_team_id: crime_scene_event.metadata_uuid("receiveTeamId")?,
                new_team_name: crime_scene_event.metadata_str("receiveTeamName")?.to_string(),
                stadium_name: stadium_nickname.to_string(),
                rating_before: shadows_event.metadata_f64("before")?,
                rating_after: shadows_event.metadata_f64("after")?,
                enter_crime_scene_sub_event: crime_scene_event.as_sub_event(),
                enter_shadows_sub_event: shadows_event.as_sub_event(),
            }
        }
        EventType::ItemBreaks => { todo!() }
        EventType::ItemDamaged => { todo!() }
        EventType::BrokenItemRepaired => { todo!() }
        EventType::DamagedItemRepaired => { todo!() }
        EventType::Announcement => { todo!() }
        EventType::RunsScored => { todo!() }
        EventType::WinCollectedRegular => { todo!() }
        EventType::WinCollectedPostseason => { todo!() }
        EventType::GameOver => { todo!() }
        EventType::StormWarning => { todo!() }
        EventType::Snowflakes => { todo!() }
        EventType::Sun2SetWin => {
            let team_name = event.next_parse(parse_sun2_set_win)?;
            assert!(is_known_team_nickname(team_name));
            FedEventData::Sun2SetWin {
                team_id: event.next_team_id()?,
                team_nickname: team_name.to_string(),
            }
        }
        EventType::BlackHoleSwallowedWin => {
            let team_name = event.next_parse(parse_black_hole_swallowed_win)?;
            assert!(is_known_team_nickname(team_name));
            FedEventData::BlackHoleSwallowedWin {
                team_id: event.next_team_id()?,
                team_nickname: team_name.to_string(),
            }
        }
        EventType::RemovedModFromOtherMod => { todo!() }
        EventType::PostseasonAdvance => {
            let (team_nickname, round_num, season_num) = event.next_parse(parse_postseason_advance)?;
            assert!(is_known_team_nickname(team_nickname));
            FedEventData::PostseasonAdvance {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                round: round_num,
                displayed_season: season_num,
            }
        }
        EventType::GainBloodType => { todo!() }
        EventType::HighPressure => {
            let (team_nickname, is_on) = event.next_parse(parse_high_pressure)?;
            assert!(is_known_team_nickname(team_nickname));
            let mut sub_event = event.next_child_any(&[EventType::AddedModFromOtherMod, EventType::RemovedModFromOtherMod])?;
            FedEventData::HighPressure {
                game: event.game(unscatter, attractor_secret_base)?,
                team_id: sub_event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                is_on,
                sub_event: sub_event.as_sub_event(),
            }
        }
        EventType::LineupSorted => {
            // This happened as a top-level event exactly once (and really it should have been a
            // child of the lovers' getting Base Dealing)
            let _ = event.next_parse_tag("The Lovers' lineup has been optimized.")?;
            FedEventData::LineupSorted {
                team_id: event.next_team_id()?,
                team_nickname: "Lovers".to_string(),
            }
        }
        EventType::NutButton => { todo!() }
        EventType::PostseasonEliminated => {
            let (team_nickname, season_num) = event.next_parse(parse_postseason_eliminated)?;
            assert!(is_known_team_nickname(team_nickname));
            FedEventData::PostseasonEliminated {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                displayed_season: season_num,
            }
        }
    };

    event.to_fed(data)
}

fn make_tarot_event(event: &mut EventParseWrapper, mod_removed: bool) -> Result<FedEventData, FeedParseError> {
    Ok(FedEventData::TarotReadingAddedOrRemovedMod {
        team_id: event.next_team_id()?,
        player_id: event.next_player_id_opt(),
        description: event.description().into(),
        r#mod: event.metadata_str("mod")?.to_string(),
        mod_duration: event.metadata_enum("type")?,
        mod_removed,
    })
}

fn make_echo(echoer_name: &str, events: (Option<EventParseWrapper>, EventParseWrapper)) -> Result<Echo, FeedParseError> {
    let (removed, mut added) = events;
    // I could verify that the IDs all match, but the round-trip test should verify that
    Ok(Echo {
        receiver_team_id: added.next_team_id()?,
        receiver_id: added.next_player_id()?,
        receiver_name: echoer_name.to_string(),
        mods_removed: removed.map(get_mods_removed).transpose()?,
        mods_added: get_mods_added(added)?,
    })
}

#[derive(Deserialize)]
struct ModAndType {
    r#mod: String,
    // r#type: i32,
}

fn get_mods_removed(event: EventParseWrapper) -> Result<MultipleModsAddedOrRemoved, FeedParseError> {
    #[derive(Deserialize)]
    struct EchoMetadata {
        removes: Vec<ModAndType>,
    }

    let des: EchoMetadata = serde_json::from_value(event.metadata().clone())
        .map_err(|_| FeedParseError::MissingMetadata {
            event_type: event.event_type,
            field: "removes".to_string(),
        })?;

    let mod_ids = des.removes.into_iter()
        .map(|mod_and_type| mod_and_type.r#mod)
        .collect();
    Ok(MultipleModsAddedOrRemoved { mod_ids, sub_event: event.as_sub_event() })
}

fn get_mods_added(event: EventParseWrapper) -> Result<MultipleModsAddedOrRemoved, FeedParseError> {
    #[derive(Deserialize)]
    struct EchoMetadata {
        adds: Vec<ModAndType>,
    }

    let des: EchoMetadata = serde_json::from_value(event.metadata().clone())
        .map_err(|_| FeedParseError::MissingMetadata {
            event_type: event.event_type,
            field: "adds".to_string(),
        })?;

    let mod_ids = des.adds.into_iter()
        .map(|mod_and_type| mod_and_type.r#mod)
        .collect();
    Ok(MultipleModsAddedOrRemoved { mod_ids, sub_event: event.as_sub_event() })
}

fn zip_mod_change_events(event: &mut EventParseWrapper, names: Vec<&str>) -> Result<Vec<ModChangeSubEventWithNamedPlayer>, FeedParseError> {
    names.into_iter()
        .map(|name| {
            let mut sub_event = event.next_child(EventType::RemovedMod)?;
            Ok(ModChangeSubEventWithNamedPlayer {
                sub_event: sub_event.as_sub_event(),
                team_id: sub_event.next_team_id()?,
                player_id: sub_event.next_player_id()?,
                player_name: name.to_string(),
            })
        })
        .collect::<Result<_, _>>()
}

// fn get_one_player_id_advanced(player_tags: &[Uuid], event_type: EventType, has_extra_id: bool) -> Result<Uuid, FeedParseError> {
//     if has_extra_id {
//         let (&id1, &id2) = player_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//                 event_type,
//                 tag_type: "player",
//                 expected_num: 2,
//                 actual_num: player_tags.len(),
//             })?;
//         if id1 != id2 {
//             Err(FeedParseError::ExpectedEqualTags {
//                 event_type,
//                 tag_type: "player",
//                 tag1: id1,
//                 tag2: id2,
//             })
//         } else {
//             Ok(id1)
//         }
//     } else {
//         get_one_player_id(&player_tags, event_type)
//     }
// }
// fn make_free_refill(event_type: EventType, children: &mut Iter<EventuallyEvent>, refiller_name: &str) -> Result<FreeRefill, FeedParseError> {
//     let child = children.next()
//         .ok_or_else(|| {
//             FeedParseError::MissingChild {
//                 event_type,
//                 expected_num_children: -1, // Unknown at this point in the computation
//             }
//         })?;
//
//     let (&team_id, ) = child.team_tags.iter().collect_tuple()
//         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//             event_type,
//             tag_type: "team",
//             expected_num: 1,
//             actual_num: child.team_tags.len(),
//         })?;
//
//     let (&player_id, ) = child.player_tags.iter().collect_tuple()
//         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//             event_type,
//             tag_type: "player",
//             expected_num: 1,
//             actual_num: child.player_tags.len(),
//         })?;
//
//     Ok(FreeRefill {
//         sub_event: child.as_sub_event(),
//         player_name: refiller_name.to_string(),
//         player_id,
//         team_id,
//         sub_play: get_sub_play(child)?,
//     })
// }

fn is_known_team_name(name: &str) -> bool {
    vec!["Hawai'i Fridays", "Canada Moist Talkers", "San Francisco Lovers", "Seattle Garages",
         "Breckenridge Jazz Hands", "Hellmouth Sunbeams", "Hades Tigers", "Mexico City Wild Wings",
         "Boston Flowers", "New York Millennials", "Philly Pies", "Miami Dale", "Tokyo Lift",
         "Chicago Firefighters", "Dallas Steaks", "Yellowstone Magic", "Kansas City Breath Mints",
         "Houston Spies", "Charleston Shoe Thieves", "LA Unlimited Tacos", "Atlantis Georgias",
         "Ohio Worms", "Baltimore Crabs", "Core Mechanics",
    ].contains(&name)
}

fn is_known_team_nickname(name: &str) -> bool {
    KNOWN_TEAM_NICKNAMES.contains(&name)
}

fn sort_children(event: &mut EventuallyEvent) {
    if event.metadata.children.iter().all(|child| child.metadata.sub_play.is_some()) {
        event.metadata.children.sort_by_key(|e| e.metadata.sub_play
            .expect("Shouldn't get here if sub_play is None"));
    }
    for child in event.metadata.children.as_mut_slice() {
        sort_children(child);
    }
}

pub fn feed_event_from_json(str: &String) -> serde_json::Result<EventuallyEvent> {
    let mut feed_event: EventuallyEvent = serde_json::from_str(&str)?;

    sort_children(&mut feed_event);

    Ok(feed_event)
}
