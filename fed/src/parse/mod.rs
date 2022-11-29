pub mod error;
pub mod event_schema;
mod feed_event_util;
pub mod builder;

use std::slice::Iter;
use itertools::{Itertools, zip_eq};
use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_till, take_till1, take_until1};
use nom::{AsChar, Finish, IResult, Parser};
use nom::character::complete::{char, digit1};
use nom::combinator::{eof, fail, map_res, opt, recognize, verify};
use nom::error::{convert_error};
use nom::multi::{many0, separated_list1};
use nom::number::complete::float;
use nom::sequence::{pair, preceded, terminated};
use uuid::Uuid;
use fed_api::{EventCategory, EventType, EventuallyEvent, Weather};
use crate::parse::error::FeedParseError;
use crate::parse::event_schema::{AttrCategory, BatterSkippedReason, Being, BlooddrainAction, CoffeeBeanMod, FedEvent, FedEventData, FeedbackPlayerData, FreeRefill, GameEvent, Inhabiting, ModChangeSubEvent, ModChangeSubEventWithNamedPlayer, ModChangeSubEventWithPlayer, ModDuration, PlayerInfo, PlayerStatChange, ActivePositionType, ReverbType, ScoreInfo, ScoringPlayer, SpicyStatus, StoppedInhabiting, SubEvent, Scattered, Unscatter};
use crate::parse::feed_event_util::{get_one_player_id, get_one_team_id, get_one_sub_event, get_str_metadata, get_float_metadata, get_str_vec_metadata, get_int_metadata, get_two_player_ids, get_two_sub_events, get_uuid_metadata, get_sub_play, get_one_or_zero_sub_events};

const KNOWN_TEAM_NICKNAMES: [&'static str; 24] = [
    "Fridays", "Moist Talkers", "Lovers", "Jazz Hands", "Sunbeams", "Tigers", "Wild Wings",
    "Flowers", "Millennials", "Pies", "Garages", "Dale", "Lift", "Firefighters", "Steaks",
    "Magic", "Breath Mints", "Spies", "Shoe Thieves", "Tacos", "Georgias", "Worms", "Crabs",
    "Mechanics",
];

pub fn parse_feed_event(feed_event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    if feed_event.metadata.siblings.is_empty() {
        parse_single_feed_event(feed_event)
    } else {
        todo!()
    }
}

fn parse_single_feed_event(event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    // This variable exists just for me to look at in the debugger, because the debugger
    // representation of the Uuid type is to low-level to copy-paste
    let _id_string = event.id.to_string();

    // This can happen on the majority of events, so I handle it outside
    let (unscatter, children) = if let Some(first_child) = event.metadata.children.first() &&
        first_child.r#type == EventType::RemovedMod &&
        get_str_metadata(first_child, "mod").map_or(false, |m| m == "SCATTERED") {
        let player_name = run_parser(first_child, parse_terminated(" was Unscattered."))?;

        (Some(Unscatter {
            sub_event: SubEvent::from_event(first_child),
            team_id: get_one_team_id(first_child)?,
            player_id: get_one_player_id(first_child)?,
            player_name: player_name.to_string(),
        }), &event.metadata.children.as_slice()[1..])
    } else {
        (None, event.metadata.children.as_slice())
    };

    match event.r#type {
        EventType::Undefined => { todo!() }
        EventType::LetsGo => {
            let missing_weather_error = || {
                FeedParseError::MissingMetadata { event_type: event.r#type, field: "weather" }
            };
            let weather = event.metadata.other
                .as_object()
                .ok_or_else(missing_weather_error)?
                .get("weather")
                .ok_or_else(missing_weather_error)?
                .as_i64()
                .ok_or_else(missing_weather_error)?;

            parse_fixed_description(event, "Let's Go!", FedEventData::LetsGo {
                game: GameEvent::try_from_event(event, unscatter)?,
                weather: Weather::try_from(weather as i32)
                    .map_err(|_| FeedParseError::UnknownWeather(weather))?,
                stadium_id: get_uuid_metadata(event, "stadium").ok(),
            })
        }
        EventType::PlayBall => {
            parse_fixed_description(event, "Play ball!", FedEventData::PlayBall {
                game: GameEvent::try_from_event(event, unscatter)?,
            })
        }
        EventType::HalfInning => {
            let (top_of_inning, inning, team_name) = run_parser(&event, parse_half_inning)?;

            assert!(is_known_team_name(team_name));

            make_fed_event(event, FedEventData::HalfInningStart {
                game: GameEvent::try_from_event(event, unscatter)?,
                top_of_inning,
                inning,
                batting_team_name: team_name.to_string(),
            })
        }
        EventType::PitcherChange => {
            let (victim_name, team_name) = run_parser(&event, parse_pitcher_change)?;

            assert!(is_known_team_nickname(team_name));

            make_fed_event(event, FedEventData::PitcherChange {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_nickname: team_name.to_string(),
                pitcher_id: get_one_player_id(event)?,
                pitcher_name: victim_name.to_string(),
            })
        }
        EventType::StolenBase => {
            let (runner_name, base_stolen, is_successful, blaserunning, free_refiller) = run_parser(&event, parse_stolen_base)?;
            if is_successful {
                let runner_id = get_one_player_id_advanced(event, blaserunning)?;

                make_fed_event(event, FedEventData::StolenBase {
                    game: GameEvent::try_from_event(event, unscatter)?,
                    runner_name: runner_name.to_string(),
                    runner_id,
                    base_stolen,
                    blaserunning,
                    free_refill: free_refiller.map(|refiller_name| {
                        let sub_event = get_one_sub_event(event)?;
                        Ok(FreeRefill {
                            sub_event: SubEvent::from_event(sub_event),
                            player_name: refiller_name.to_string(),
                            player_id: get_one_player_id(sub_event)?,
                            team_id: get_one_team_id(sub_event)?,
                            sub_play: get_sub_play(sub_event)?,
                        })
                    }).transpose()?,
                })
            } else {
                make_fed_event(event, FedEventData::CaughtStealing {
                    game: GameEvent::try_from_event(event, unscatter)?,
                    runner_name: runner_name.to_string(),
                    base_stolen,
                })
            }
        }
        EventType::Walk => {
            let parsed_walk = run_parser(&event, parse_walk)?;
            match parsed_walk {
                ParsedWalk::Ordinary((batter_name, scores, base_instincts)) => {
                    let (&batter_id, scorer_ids) = event.player_tags.split_first()
                        .ok_or_else(|| {
                            FeedParseError::WrongNumberOfTags {
                                event_type: event.r#type,
                                tag_type: "player",
                                expected_num: 1 + scores.scorers.len(),
                                actual_num: event.player_tags.len(),
                            }
                        })?;

                    let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, &children, event.r#type, 0)?;

                    make_fed_event(event, FedEventData::Walk {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        scores,
                        stopped_inhabiting,
                        base_instincts,
                    })
                }
                ParsedWalk::Charm((batter_name, pitcher_name, scores)) => {
                    let (&batter_id, rest_ids) = event.player_tags.split_first()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 2,
                            actual_num: event.player_tags.len(),
                        })?;
                    let (&charmer_id, rest_ids) = rest_ids.split_first()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 2,
                            actual_num: event.player_tags.len(),
                        })?;
                    assert_eq!(batter_id, charmer_id);

                    let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_ids, &children, event.r#type, 0)?;

                    make_fed_event(event, FedEventData::CharmWalk {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        pitcher_name: pitcher_name.to_string(),
                        scores,
                        stopped_inhabiting,
                    })
                }
            }
        }
        EventType::Strikeout => {
            match run_parser(&event, parse_strikeout)? {
                ParsedStrikeout::Swinging(batter_name) => {
                    let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &event.player_tags, &children, event.r#type, 0)?;
                    make_fed_event(event, FedEventData::StrikeoutSwinging {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                        is_special: event.category == EventCategory::Special,
                    })
                }
                ParsedStrikeout::Looking(batter_name) => {
                    let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &event.player_tags, &children, event.r#type, 0)?;
                    make_fed_event(event, FedEventData::StrikeoutLooking {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                        is_special: event.category == EventCategory::Special,
                    })
                }
                ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings } => {
                    if let Some((&charmer_id, &charmer_id_2, &charmed_id)) = event.player_tags.iter().collect_tuple() {
                        assert_eq!(charmer_id, charmer_id_2);
                        make_fed_event(event, FedEventData::CharmStrikeout {
                            game: GameEvent::try_from_event(event, unscatter)?,
                            charmer_id,
                            charmer_name: charmer_name.to_string(),
                            charmed_id,
                            charmed_name: charmed_name.to_string(),
                            num_swings,
                        })
                    } else {
                        Err(FeedParseError::WrongNumberOfTags {
                            event_type: EventType::Strikeout,
                            tag_type: "players",
                            expected_num: 3,
                            actual_num: event.player_tags.len(),
                        })
                    }
                }
            }
        }
        EventType::FlyOut => {
            let (batter_name, fielder_name, scores, cooled_off) = run_parser(&event, parse_flyout)?;
            let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, children, cooled_off, &event.player_tags)?;
            let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
            make_fed_event(event, FedEventData::Flyout {
                game: GameEvent::try_from_event(event, unscatter)?,
                batter_name: batter_name.to_string(),
                fielder_name: fielder_name.to_string(),
                scores,
                stopped_inhabiting,
                cooled_off,
            })
        }
        EventType::GroundOut => {
            let (parsed_out, scores, cooled_off) = run_parser(&event, parse_ground_out)?;
            let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, children, cooled_off, &event.player_tags)?;
            let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
            match parsed_out {
                ParsedGroundOut::Simple { batter_name, fielder_name } => {
                    make_fed_event(event, FedEventData::GroundOut {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        fielder_name: fielder_name.to_string(),
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                        is_special: event.category == EventCategory::Special,
                    })
                }
                ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base } => {
                    make_fed_event(event, FedEventData::FieldersChoice {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        runner_out_name: runner_out_name.to_string(),
                        batter_name: batter_name.to_string(),
                        out_at_base: base,
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                    })
                }
                ParsedGroundOut::DoublePlay { batter_name } => {
                    make_fed_event(event, FedEventData::DoublePlay {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        batter_name: batter_name.to_string(),
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                    })
                }
            }
        }
        EventType::HomeRun => {
            let (is_magmatic, batter_name, num_runs, free_refillers, spicy_status) = run_parser(&event, parse_hr)?;
            let (remaining_children, spicy_status) = extract_spicy_event(children, spicy_status)?;
            let (remaining_children, magmatic_event) = if is_magmatic {
                let expected_num_children = children.len() - remaining_children.len() + 1;
                remaining_children.split_first()
                    .map(|(magmatic_event, remaining_children)| {
                        (remaining_children, Some(magmatic_event))
                    })
                    .ok_or_else(move || {
                        FeedParseError::MissingChild {
                            event_type: event.r#type,
                            expected_num_children: expected_num_children as i32,
                        }
                    })?
            } else {
                (remaining_children, None)
            };

            let (remaining_children, stopped_inhabiting) = if remaining_children.is_empty() {
                (remaining_children, None)
            } else if let Some((sub_event, remaining)) = remaining_children.split_last() {
                run_parser(sub_event, parse_terminated(" stopped Inhabiting."))
                    .map(|name| {
                        Ok((remaining, Some(StoppedInhabiting {
                            sub_event: SubEvent::from_event(sub_event),
                            inhabiting_player_name: name.to_string(),
                            inhabiting_player_id: get_one_player_id(sub_event)?,
                            inhabiting_player_team_id: if sub_event.team_tags.is_empty() {
                                None
                            } else {
                                Some(get_one_team_id(sub_event)?)
                            },
                        })))
                    })
                    .unwrap_or(Ok((remaining_children, None)))?
            } else {
                let expected_num_children = children.len() - remaining_children.len() + 1;
                Err(FeedParseError::MissingChild {
                    event_type: event.r#type,
                    expected_num_children: expected_num_children as i32,
                })?
            };

            let batter_id = get_one_player_id_advanced(event, !spicy_status.is_none())?;
            make_fed_event(event, FedEventData::HomeRun {
                game: GameEvent::try_from_event(event, unscatter)?,
                magmatic: magmatic_event.map(|e| {
                    Ok(ModChangeSubEvent {
                        sub_event: SubEvent::from_event(e),
                        team_id: get_one_team_id(e)?,
                    })
                }).transpose()?,
                batter_name: batter_name.to_string(),
                batter_id,
                num_runs,
                stopped_inhabiting,
                free_refills: free_refillers.into_iter()
                    .map(|refiller_name| {
                        let mut remaining_children = remaining_children.iter();
                        make_free_refill(event.r#type, &mut remaining_children, refiller_name)
                    })
                    .collect::<Result<_, _>>()?,
                spicy_status,
                is_special: event.category == EventCategory::Special,
            })
        }
        EventType::Hit => {
            let (batter_name, num_bases, scores, spicy_status) = run_parser(&event, parse_hit)?;
            if let Some((&batter_id, scorer_ids)) = event.player_tags.split_first() {
                let scorer_ids = if spicy_status != ParsedSpicyStatus::None {
                    scorer_ids.split_last()
                        .ok_or_else(|| {
                            FeedParseError::WrongNumberOfTags {
                                event_type: event.r#type,
                                tag_type: "player",
                                expected_num: scores.scorers.len() + 2, // i think
                                actual_num: scorer_ids.len(),
                            }
                        })?
                        .1
                } else {
                    scorer_ids
                };

                let (score_children, spicy_status) = extract_spicy_event(children, spicy_status)?;
                let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, score_children, event.r#type, 1)?;

                make_fed_event(event, FedEventData::Hit {
                    game: GameEvent::try_from_event(event, unscatter)?,
                    batter_name: batter_name.to_string(),
                    batter_id,
                    num_bases,
                    scores,
                    stopped_inhabiting,
                    spicy_status,
                    is_special: event.category == EventCategory::Special,
                })
            } else {
                Err(FeedParseError::MissingTags { event_type: event.r#type, tag_type: "player" })
            }
        }
        EventType::GameEnd => {
            let ((winning_team_name, winning_team_score), (losing_team_name, losing_team_score)) = run_parser(&event, parse_game_end)?;
            let winner_id = event.metadata.other.as_object()
                .and_then(|map| map.get("winner"))
                .and_then(|obj| obj.as_str())
                .and_then(|uuid_str| Uuid::try_parse(uuid_str).ok())
                .ok_or_else(|| FeedParseError::MissingMetadata {
                    event_type: event.r#type,
                    field: "winner",
                })?;
            make_fed_event(event, FedEventData::GameEnd {
                game: GameEvent::try_from_event_extra_teams(event, unscatter)?,
                winner_id,
                winning_team_name: winning_team_name.to_string(),
                winning_team_score,
                losing_team_name: losing_team_name.to_string(),
                losing_team_score,
            })
        }
        EventType::BatterUp => {
            let (batter_name, inhabited, team_name, wielding_item, is_repeating) = run_parser(&event, parse_batter_up)?;

            // I missed `team_name: "Millennials, wielding An Actual Airplane"` once and I don't
            // want something like that to happen again
            assert!(is_known_team_nickname(team_name));

            make_fed_event(event, FedEventData::BatterUp {
                game: GameEvent::try_from_event(event, unscatter)?,
                batter_name: batter_name.to_string(),
                team_name: team_name.to_string(),
                wielding_item: wielding_item.map(|s| s.to_string()),
                inhabiting: inhabited.map(|inhabited| {
                    let (child, ) = children.iter().collect_tuple()
                        .ok_or_else(|| {
                            FeedParseError::MissingChild {
                                event_type: event.r#type,
                                expected_num_children: 1,
                            }
                        })?;

                    // These live on the parent
                    let (inhabiting_player_id, inhabited_player_id) = get_two_player_ids(event)?;

                    Ok(Inhabiting {
                        sub_event: SubEvent::from_event(child),
                        inhabited_player_name: inhabited.to_string(),
                        inhabiting_player_id,
                        inhabited_player_id,
                        inhabiting_player_team_id: if child.team_tags.is_empty() {
                            None
                        } else {
                            Some(get_one_team_id(child)?)
                        },
                    })
                }).transpose()?,
                is_repeating,
            })
        }
        EventType::Strike => {
            let (strike_type, balls, strikes) = run_parser(&event, parse_strike)?;
            let game = GameEvent::try_from_event(event, unscatter)?;
            make_fed_event(event, match strike_type {
                StrikeType::Swinging => FedEventData::StrikeSwinging { game, balls, strikes },
                StrikeType::Looking => FedEventData::StrikeLooking { game, balls, strikes },
                StrikeType::Flinching => FedEventData::StrikeFlinching { game, balls, strikes },
            })
        }
        EventType::Ball => {
            let (balls, strikes) = run_parser(&event, parse_ball)?;
            make_fed_event(event, FedEventData::Ball {
                game: GameEvent::try_from_event(event, unscatter)?,
                balls,
                strikes,
            })
        }
        EventType::FoulBall => {
            // Eventually this will need very foul support, but I'll get to that when it comes up
            let (balls, strikes) = run_parser(&event, parse_foul_ball)?;
            make_fed_event(event, FedEventData::FoulBall {
                game: GameEvent::try_from_event(event, unscatter)?,
                balls,
                strikes,
            })
        }
        EventType::ShamingRun => { todo!() }
        EventType::HomeFieldAdvantage => { todo!() }
        EventType::HitByPitch => { todo!() }
        EventType::BatterSkipped => {
            let (player_name, reason) = run_parser(&event, parse_batter_skipped)?;
            make_fed_event(event, FedEventData::BatterSkipped {
                game: GameEvent::try_from_event(event, unscatter)?,
                batter_name: player_name.to_string(),
                reason: match reason {
                    ParsedBatterSkippedReason::Shelled => { BatterSkippedReason::Shelled }
                    ParsedBatterSkippedReason::Elsewhere => {
                        BatterSkippedReason::Elsewhere(get_one_player_id(event)?)
                    }
                },
            })
        }
        EventType::Party => {
            let player_name = run_parser(&event, parse_party)?;
            let sub_event = get_one_sub_event(event)?;
            make_fed_event(event, FedEventData::Party {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(sub_event)?,
                player_id: get_one_player_id(sub_event)?,
                player_name: player_name.to_string(),
                sub_event: SubEvent::from_event(sub_event),
                rating_before: get_float_metadata(sub_event, "before")?,
                rating_after: get_float_metadata(sub_event, "after")?,
            })
        }
        EventType::StrikeZapped => {
            parse_fixed_description(event, "The Electricity zaps a strike away!",
                                    FedEventData::StrikeZapped {
                                        game: GameEvent::try_from_event(event, unscatter)?,
                                    })
        }
        EventType::WeatherChange => { todo!() }
        EventType::MildPitch => {
            let (pitcher_name, pitch_type, runners_advance, scores) = run_parser(&event, parse_mild_pitch)?;
            let (&pitcher_id, rest_player_ids) = event.player_tags.split_first()
                .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                    event_type: event.r#type,
                    tag_type: "player",
                    expected_num: 1,
                    actual_num: event.player_tags.len(),
                })?;

            match pitch_type {
                MildPitchType::Ball((balls, strikes)) => {
                    let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_player_ids, &children, event.r#type, 1)?;

                    make_fed_event(event, FedEventData::MildPitch {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        pitcher_id,
                        pitcher_name: pitcher_name.to_string(),
                        balls,
                        strikes,
                        runners_advance,
                        scores,
                        stopped_inhabiting,
                    })
                }
                MildPitchType::Walk(batter_name) => {
                    let (&batter_id, rest_player_ids) = rest_player_ids.split_first()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 2,
                            actual_num: event.player_tags.len(),
                        })?;
                    let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_player_ids, &children, event.r#type, 2)?;

                    // I don't believe this should be possible
                    assert!(!runners_advance, "Runners \"advanced on the pathetic play\" on a mild pitch that was also a walk");
                    make_fed_event(event, FedEventData::MildPitchWalk {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        pitcher_id,
                        pitcher_name: pitcher_name.to_string(),
                        batter_id,
                        batter_name: batter_name.to_string(),
                        scores,
                        stopped_inhabiting,
                    })
                }
            }
        }
        EventType::InningEnd => {
            let (inning_num, lost_triple_threat_names) = run_parser(&event, parse_inning_end)?;

            make_fed_event(event, FedEventData::InningEnd {
                game: GameEvent::try_from_event(event, unscatter)?,
                inning_num,
                lost_triple_threat: zip_mod_change_events(lost_triple_threat_names, children)?,
            })
        }
        EventType::BigDeal => {
            let metadata_err = || FeedParseError::MissingMetadata {
                event_type: event.r#type,
                field: "being",
            };
            let being_id = event.metadata
                .other
                .as_object()
                .ok_or_else(metadata_err)?
                .get("being")
                .ok_or_else(metadata_err)?
                .as_i64()
                .ok_or_else(metadata_err)?;

            make_fed_event(event, FedEventData::BeingSpeech {
                being: Being::try_from(being_id as i32)
                    .map_err(|_| FeedParseError::UnknownBeing(being_id))?,
                message: event.description.clone(),
            })
        }
        EventType::BlackHole => {
            let (scoring_team, victim_team) = run_parser(&event, parse_black_hole)?;
            assert!(is_known_team_nickname(scoring_team));
            assert!(is_known_team_nickname(victim_team));
            make_fed_event(event, FedEventData::BlackHole {
                game: GameEvent::try_from_event(event, unscatter)?,
                scoring_team_nickname: scoring_team.to_string(),
                victim_team_nickname: victim_team.to_string(),
            })
        }
        EventType::Sun2 => {
            let scoring_team = run_parser(&event, parse_sun2)?;
            assert!(is_known_team_nickname(scoring_team));
            make_fed_event(event, FedEventData::Sun2 {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_nickname: scoring_team.to_string(),
            })
        }
        EventType::BirdsCircle => {
            parse_fixed_description(event, "The Birds circle ... but they don't find what they're looking for.", FedEventData::BirdsCircle {
                game: GameEvent::try_from_event(event, unscatter)?,
            })
        }
        EventType::AmbushedByCrows => {
            let (pitcher_name, batter_name) = run_parser(&event, parse_friend_of_crows)?;
            let (pitcher, batter_id) = if let Some(name) = pitcher_name {
                let (pitcher_uuid, batter_uuid) = get_two_player_ids(event)?;
                (Some(PlayerInfo { player_id: pitcher_uuid, player_name: name.to_string() }), batter_uuid)
            } else {
                (None, get_one_player_id(event)?)
            };

            make_fed_event(event, FedEventData::AmbushedByCrows {
                game: GameEvent::try_from_event(event, unscatter)?,
                batter_id,
                batter_name: batter_name.to_string(),
                pitcher,
            })
        }
        EventType::BirdsUnshell => {
            let player_name = run_parser(&event, parse_birds_unshell)?;

            let (pecked_free, superallergy) = get_two_sub_events(event)?;
            let team_id = get_one_team_id(pecked_free)?;
            assert_eq!(team_id, get_one_team_id(superallergy)?);
            let player_id = get_one_player_id(pecked_free)?;
            assert_eq!(player_id, get_one_player_id(superallergy)?);

            make_fed_event(event, FedEventData::BirdsUnshell {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id,
                player_id,
                player_name: player_name.to_string(),
                pecked_free_event: SubEvent::from_event(pecked_free),
                superallergy_event: SubEvent::from_event(superallergy),
            })
        }
        EventType::BecomeTripleThreat => {
            let names = run_parser(&event, parse_become_triple_threat)?;

            let pitchers = zip_eq(children, names)
                .map(|(event, pitcher_name)| {
                    Ok(ModChangeSubEventWithNamedPlayer {
                        sub_event: SubEvent::from_event(event),
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: pitcher_name.to_string(),
                    })
                })
                .collect::<Result<_, _>>()?;

            make_fed_event(event, FedEventData::BecomeTripleThreat {
                game: GameEvent::try_from_event(event, unscatter)?,
                pitchers,
            })
        }
        EventType::GainFreeRefill => {
            let (player_name, roast, ingredient1, ingredient2) = run_parser(&event, parse_gain_free_refill)?;
            let sub_event = get_one_sub_event(event)?;
            let player_id = get_one_player_id(event)?;
            // The player ID should match in the sub event
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            make_fed_event(event, FedEventData::GainFreeRefill {
                game: GameEvent::try_from_event(event, unscatter)?,
                player_id,
                player_name: player_name.to_string(),
                roast: roast.to_string(),
                ingredient1: ingredient1.to_string(),
                ingredient2: ingredient2.to_string(),
                sub_event: SubEvent::from_event(sub_event),
                team_id: get_one_team_id(sub_event)?,
            })
        }
        EventType::CoffeeBean => {
            let (player_name, roast, notes, wired, gained) = run_parser(&event, parse_coffee_bean)?;
            let sub_event = get_one_sub_event(event)?;
            let player_id = get_one_player_id(event)?;
            let prev_mod = if sub_event.r#type == EventType::ModChange {
                let mod_str = get_str_metadata(sub_event, "to")?;
                // Check that the added mod matches what was parsed
                assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
                Some(get_str_metadata(sub_event, "from")?)
            } else {
                let mod_str = get_str_metadata(sub_event, "mod")?;
                // Check that the added mod matches what was parsed
                assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
                None
            };
            // The player ID should match in the sub event
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            make_fed_event(event, FedEventData::CoffeeBean {
                game: GameEvent::try_from_event(event, unscatter)?,
                player_id,
                player_name: player_name.to_string(),
                roast: roast.to_string(),
                notes: notes.to_string(),
                which_mod: if wired { CoffeeBeanMod::Wired } else { CoffeeBeanMod::Tired },
                has_mod: gained,
                sub_event: SubEvent::from_event(sub_event),
                team_id: get_one_team_id(sub_event)?,
                previous: prev_mod.map(|s| s.try_into()
                    .map_err(|_| FeedParseError::UnexpectedMetadataValue {
                        event_type: sub_event.r#type,
                        field: "from",
                        value: s.to_string(),
                    })
                ).transpose()?,
            })
        }
        EventType::FeedbackBlocked => {
            let (resisted_name, tangled_name) = run_parser(&event, parse_feedback_blocked)?;
            let (resisted_id, tangled_id) = get_two_player_ids(event)?;
            let sub_event = get_one_sub_event(event)?;

            make_fed_event(event, FedEventData::FeedbackBlocked {
                game: GameEvent::try_from_event(event, unscatter)?,
                resisted_id,
                resisted_name: resisted_name.to_string(),
                tangled_id,
                tangled_team_id: get_one_team_id(sub_event)?,
                tangled_name: tangled_name.to_string(),
                tangled_rating_before: get_float_metadata(sub_event, "before")?,
                tangled_rating_after: get_float_metadata(sub_event, "after")?,
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::FeedbackSwap => {
            let (player1_name, player2_name, position) = run_parser(&event, parse_feedback)?;
            let sub_event = get_one_sub_event(event)?;

            macro_rules! get_player_data {
                ($event:ident, $prefix:literal, $expected_name:ident) => {
                    {
                        let team_nickname = get_str_metadata($event, concat!($prefix, "TeamName"))?.to_string();
                        assert!(is_known_team_nickname(&team_nickname));
                        let player_name = get_str_metadata($event, concat!($prefix, "PlayerName"))?.to_string();
                        assert_eq!(player_name, $expected_name);
                        FeedbackPlayerData {
                            team_id: get_uuid_metadata($event, concat!($prefix, "TeamId"))?,
                            team_nickname,
                            player_id: get_uuid_metadata($event, concat!($prefix, "PlayerId"))?,
                            player_name,
                            location: get_int_metadata($event, concat!($prefix, "Location"))?,
                        }
                    }
                };
            }

            make_fed_event(event, FedEventData::Feedback {
                game: GameEvent::try_from_event(event, unscatter)?,
                players: (
                    get_player_data!(sub_event, "a", player1_name),
                    get_player_data!(sub_event, "b", player2_name),
                ),
                position_type: position,
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::SuperallergicReaction => { todo!() }
        EventType::AllergicReaction => {
            let player_name = run_parser(&event, parse_allergic_reaction)?;
            let player_id = get_one_player_id(event)?;
            let sub_event = get_one_sub_event(event)?;
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            make_fed_event(event, FedEventData::AllergicReaction {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(sub_event)?,
                player_id,
                player_name: player_name.to_string(),
                sub_event: SubEvent::from_event(sub_event),
                rating_before: get_float_metadata(sub_event, "before")?,
                rating_after: get_float_metadata(sub_event, "after")?,
            })
        }
        EventType::ReverbBestowsReverberating => {
            let player_name = run_parser(&event, parse_bestow_reverberating)?;
            let player_id = get_one_player_id(event)?;
            let sub_event = get_one_sub_event(event)?;
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            make_fed_event(event, FedEventData::BestowReverberating {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(sub_event)?,
                player_id,
                player_name: player_name.to_string(),
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::ReverbRosterShuffle => {
            let (team_nickname, reverb_type, gravity_player_names) = run_parser(&event, parse_roster_shuffle)?;

            let gravity_players = zip_eq(gravity_player_names, &event.player_tags)
                .map(|(player_name, &player_id)| PlayerInfo { player_id, player_name: player_name.to_string() })
                .collect();

            match reverb_type {
                ParsedReverbType::Rotation => {
                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::Reverb {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: get_one_team_id(sub_event)?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Rotation(SubEvent::from_event(sub_event)),
                        gravity_players,
                    })
                }
                ParsedReverbType::Lineup => {
                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::Reverb {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: get_one_team_id(sub_event)?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Lineup(SubEvent::from_event(sub_event)),
                        gravity_players,
                    })
                }
                ParsedReverbType::Full => {
                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::Reverb {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: get_one_team_id(sub_event)?,
                        team_nickname: team_nickname.to_string(),
                        reverb_type: ReverbType::Full(SubEvent::from_event(sub_event)),
                        gravity_players,
                    })
                }
                ParsedReverbType::SeveralPlayers => {
                    todo!()
                }
            }
        }
        EventType::Blooddrain => {
            let (sipper_name, sipped_name, sipped_category) = run_parser(&event, parse_blooddrain)?;
            let (sipper_id, sipped_id) = get_two_player_ids(event)?;

            let (sipped_event, sipper_event) = get_two_sub_events(event)?;

            make_fed_event(event, FedEventData::Blooddrain {
                game: GameEvent::try_from_event(event, unscatter)?,
                is_siphon: false,
                sipper: PlayerStatChange {
                    team_id: get_one_team_id(sipper_event)?,
                    player_id: sipper_id,
                    player_name: sipper_name.to_string(),
                    rating_before: get_float_metadata(sipper_event, "before")?,
                    rating_after: get_float_metadata(sipper_event, "after")?,
                    sub_event: SubEvent::from_event(sipper_event),
                },
                sipped: PlayerStatChange {
                    team_id: get_one_team_id(sipped_event)?,
                    player_id: sipped_id,
                    player_name: sipped_name.to_string(),
                    rating_before: get_float_metadata(sipped_event, "before")?,
                    rating_after: get_float_metadata(sipped_event, "after")?,
                    sub_event: SubEvent::from_event(sipped_event),
                },
                sipped_category,
            })
        }
        EventType::BlooddrainSiphon => {
            let (sipper_name, sipped_name, sipped_category, action) = run_parser(&event, parse_blooddrain_siphon)?;
            let (sipper_id, sipped_id) = get_two_player_ids(event)?;

            match action {
                None => {
                    let (sipped_event, sipper_event) = get_two_sub_events(event)?;

                    make_fed_event(event, FedEventData::Blooddrain {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        is_siphon: true,
                        sipper: PlayerStatChange {
                            team_id: get_one_team_id(sipper_event)?,
                            player_id: sipper_id,
                            player_name: sipper_name.to_string(),
                            rating_before: get_float_metadata(sipper_event, "before")?,
                            rating_after: get_float_metadata(sipper_event, "after")?,
                            sub_event: SubEvent::from_event(sipper_event),
                        },
                        sipped: PlayerStatChange {
                            team_id: get_one_team_id(sipped_event)?,
                            player_id: sipped_id,
                            player_name: sipped_name.to_string(),
                            rating_before: get_float_metadata(sipped_event, "before")?,
                            rating_after: get_float_metadata(sipped_event, "after")?,
                            sub_event: SubEvent::from_event(sipped_event),
                        },
                        sipped_category,
                    })
                }
                Some(action) => {
                    let stat_decrease_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::SpecialBlooddrain {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        sipper_id,
                        sipped_team_id: get_one_team_id(stat_decrease_event)?,
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
                        sipped_event: SubEvent::from_event(stat_decrease_event),
                        rating_before: get_float_metadata(stat_decrease_event, "before")?,
                        rating_after: get_float_metadata(stat_decrease_event, "after")?,
                    })
                }
            }
        }
        EventType::BlooddrainBlocked => { todo!() }
        EventType::Incineration => {
            let (victim_name, replacement_name) = run_parser(&event, parse_incineration)?;
            let (incin_child, enter_hall_child, hatch_child, replace_child) =
                children.iter().collect_tuple()
                    .ok_or_else(|| {
                        FeedParseError::MissingChild {
                            event_type: event.r#type,
                            expected_num_children: 4,
                        }
                    })?;

            let team_nickname = get_str_metadata(replace_child, "teamName")?;
            assert!(is_known_team_nickname(team_nickname));
            make_fed_event(event, FedEventData::Incineration {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(incin_child)?,
                team_nickname: team_nickname.to_string(),
                victim_id: get_one_player_id(incin_child)?,
                victim_name: victim_name.to_string(),
                replacement_id: get_one_player_id(hatch_child)?,
                replacement_name: replacement_name.to_string(),
                location: get_int_metadata(replace_child, "location")?
                    .try_into()
                    .map_err(|_| FeedParseError::MissingMetadata {
                        event_type: event.r#type,
                        field: "location",
                    })?,
                sub_events: (
                    SubEvent::from_event(incin_child),
                    SubEvent::from_event(enter_hall_child),
                    SubEvent::from_event(hatch_child),
                    SubEvent::from_event(replace_child),
                ),
            })
        }
        EventType::IncinerationBlocked => {
            // For now I only support magmatic, that may have to change
            let (player_name, blocked_reason) = run_parser(&event, parse_incineration_blocked)?;
            match blocked_reason {
                IncinerationBlockedReason::Magmatic => {
                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::BecameMagmatic {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        player_id: get_one_player_id(event)?,
                        player_name: player_name.to_string(),
                        team_id: get_one_team_id(sub_event)?,
                        mod_add_event: SubEvent::from_event(sub_event),
                    })
                }
                IncinerationBlockedReason::Fireproof => {
                    make_fed_event(event, FedEventData::FireproofIncineration {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        player_id: get_one_player_id(event)?,
                        player_name: player_name.to_string(),
                    })
                }
            }
        }
        EventType::FlagPlanted => {
            let (team_nickname, park_name, prefab_name, is_first) = run_parser(&event, parse_flag_planted)?;

            make_fed_event(event, FedEventData::FlagPlanted {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                ballpark_name: park_name.to_string(),
                prefab_name: prefab_name.to_string(),
                renovation_id: get_str_metadata(event, "renoId")?.to_string(),
                votes: get_int_metadata(event, "votes")?,
                is_first,
            })
        }
        EventType::RenovationBuilt => {
            // It may be valuable to parse which reno is built, but there isn't one unified syntax
            // so I'm not going to put in the work now. Contributions welcome.
            make_fed_event(event, FedEventData::RenovationBuilt {
                team_id: get_one_team_id(event)?,
                description: event.description.clone(),
                renovation_id: get_str_metadata(event, "renoId")?.to_string(),
                renovation_title: get_str_metadata(event, "title")?.to_string(),
                votes: get_int_metadata(event, "votes")?,
            })
        }
        EventType::LightSwitchToggled => { todo!() }
        EventType::DecreePassed => {
            let decree_title = run_parser(&event, parse_decree_passed)?;

            make_fed_event(event, FedEventData::DecreePassed {
                decree_title: decree_title.to_string(),
                metadata: event.metadata.clone(),
            })
        }
        EventType::BlessingOrGiftWon => {
            let blessing_title = run_parser(&event, parse_blessing_won)?;

            make_fed_event(event, FedEventData::BlessingWon {
                team_tags: event.team_tags.clone(),
                blessing_title: blessing_title.to_string(),
                metadata: event.metadata.clone(),
            })
        }
        EventType::WillRecieved => {
            let will_title = run_parser(&event, parse_will_received)?;

            make_fed_event(event, FedEventData::WillReceived {
                team_id: get_one_team_id(event)?,
                will_title: will_title.to_string(),
                metadata: event.metadata.clone(),
            })
        }
        EventType::FloodingSwept => {
            let (swept_elsewhere_names, flippered_home_names) = run_parser(&event, parse_flooding_swept)?;

            make_fed_event(event, FedEventData::FloodingSwept {
                game: GameEvent::try_from_event(event, unscatter)?,
                swept_elsewhere: zip_mod_change_events(swept_elsewhere_names, children)?,
                slingshot_home: itertools::zip_eq(flippered_home_names, &event.player_tags)
                    .map(|(player_name, &player_id)| PlayerInfo {
                        player_id,
                        player_name: player_name.to_string(),
                    })
                    .collect(),
            })
        }
        EventType::SalmonSwim => { todo!() }
        EventType::PolarityShift => { todo!() }
        EventType::EnterSecretBase => { todo!() }
        EventType::ExitSecretBase => { todo!() }
        EventType::ConsumersAttack => { todo!() }
        EventType::EchoChamber => { todo!() }
        EventType::GrindRail => { todo!() }
        EventType::TunnelsUsed => { todo!() }
        EventType::PeanutMister => {
            let player_name = run_parser(event, parse_peanut_mister)?;
            make_fed_event(event, FedEventData::PeanutMister {
                game: GameEvent::try_from_event(event, unscatter)?,
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
            })
        }
        EventType::PeanutFlavorText => {
            make_fed_event(event, FedEventData::PeanutFlavorText {
                game: GameEvent::try_from_event(event, unscatter)?,
                message: event.description.clone(),
            })
        }
        EventType::TasteTheInfinite => {
            let (sheller_name, shellee_name) = run_parser(event, parse_taste_the_infinite)?;
            let (sheller_id, shellee_id) = get_two_player_ids(event)?;

            let sub_event = get_one_sub_event(event)?;
            make_fed_event(event, FedEventData::TasteTheInfinite {
                game: GameEvent::try_from_event(event, unscatter)?,
                sheller_id,
                sheller_name: sheller_name.to_string(),
                shellee_team_id: get_one_team_id(sub_event)?,
                shellee_id,
                shellee_name: shellee_name.to_string(),
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::EventHorizonActivation => { todo!() }
        EventType::EventHorizonAwaits => { todo!() }
        EventType::SolarPanelsAwait => { todo!() }
        EventType::SolarPanelsActivation => { todo!() }
        EventType::TarotReading => {
            make_fed_event(event, FedEventData::TarotReading {
                description: event.description.clone(),
                metadata: event.metadata.other.clone(),
                player_tags: event.player_tags.clone(),
                team_tags: event.team_tags.clone(),
            })
        }
        EventType::EmergencyAlert => {
            make_fed_event(event, FedEventData::EmergencyAlert {
                message: event.description.clone(),
                team_tags: event.team_tags.clone(),
            })
        }
        EventType::ReturnFromElsewhere => {
            let (player_name, after_days) = run_parser(event, parse_return_from_elsewhere)?;

            let (return_sub_event, scattered) = if children.len() == 2 {
                let (scattered_sub_event, return_sub_event) = get_two_sub_events(event)?;
                let scattered_name = run_parser(scattered_sub_event, parse_terminated(" was Scattered..."))?;

                let scattered = Scattered {
                    scattered_name: scattered_name.to_string(),
                    sub_event: SubEvent::from_event(scattered_sub_event),
                };
                (return_sub_event, Some(scattered))
            } else {
                (get_one_sub_event(event)?, None)
            };

            make_fed_event(event, FedEventData::ReturnFromElsewhere {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(return_sub_event)?,
                player_id: get_one_player_id(return_sub_event)?,
                player_name: player_name.to_string(),
                sub_event: SubEvent::from_event(return_sub_event),
                number_of_days: after_days,
                scattered,
            })
        }
        EventType::OverUnder => {
            let (player_name, on) = run_parser(event, parse_under_over_over_under("Over Under"))?;

            let sub_event = get_one_sub_event(event)?;
            make_fed_event(event, FedEventData::OverUnder {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(sub_event)?,
                player_id: get_one_player_id(sub_event)?,
                player_name: player_name.to_string(),
                on,
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::UnderOver => {
            let (player_name, on) = run_parser(event, parse_under_over_over_under("Under Over"))?;

            let sub_event = get_one_sub_event(event)?;
            make_fed_event(event, FedEventData::UnderOver {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(sub_event)?,
                player_id: get_one_player_id(sub_event)?,
                player_name: player_name.to_string(),
                on,
                sub_event: SubEvent::from_event(sub_event),
            })
        }
        EventType::Undersea => {
            let team_name = run_parser(event, parse_undersea)?;
            assert!(is_known_team_name(team_name));

            let mod_add_event = get_one_sub_event(event)?;

            make_fed_event(event, FedEventData::Undersea {
                game: GameEvent::try_from_event(event, unscatter)?,
                team_id: get_one_team_id(mod_add_event)?,
                team_name: team_name.to_string(),
                sub_event: SubEvent::from_event(mod_add_event),
            })
        }
        EventType::Homebody => { todo!() }
        EventType::Superyummy => {
            let (player_name, peanuts_present) = run_parser(event, parse_superyummy)?;

            let mod_add_event = get_one_sub_event(event)?;

            make_fed_event(event, FedEventData::SuperyummyGameStart {
                game: GameEvent::try_from_event(event, unscatter)?,
                player_name: player_name.to_string(),
                peanuts_present,
                is_first_proc: mod_add_event.r#type == EventType::AddedModFromOtherMod,
                sub_event: SubEvent::from_event(mod_add_event),
                player_id: get_one_player_id(mod_add_event)?,
                team_id: get_one_team_id(mod_add_event)?,
            })
        }
        EventType::Perk => {
            let player_names = run_parser(event, parse_perk_up)?;

            make_fed_event(event, FedEventData::PerkUp {
                game: GameEvent::try_from_event(event, unscatter)?,
                players: children.iter()
                    .zip(player_names)
                    .map(|(mod_add_event, player_name)| {
                        assert_eq!(format!("{player_name} Perks up."), mod_add_event.description);
                        Ok(ModChangeSubEventWithNamedPlayer {
                            player_name: player_name.to_string(),
                            sub_event: SubEvent::from_event(mod_add_event),
                            player_id: get_one_player_id(mod_add_event)?,
                            team_id: get_one_team_id(mod_add_event)?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            })
        }
        EventType::Earlbird => {
            match run_parser(event, parse_earlbird)? {
                EarlbirdsChange::Added(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::EarlbirdsAdded {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: get_one_team_id(sub_event)?,
                        team_nickname: team_nickname.to_string(),
                        sub_event: SubEvent::from_event(sub_event),
                    })
                }
                EarlbirdsChange::Removed => {
                    let sub_event = get_one_sub_event(event)?;
                    make_fed_event(event, FedEventData::EarlbirdsRemoved {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: get_one_team_id(sub_event)?,
                        sub_event: SubEvent::from_event(sub_event),
                    })
                }
            }
        }
        EventType::LateToTheParty => {
            match run_parser(event, parse_late_to_the_party)? {
                LateToThePartyChange::Added(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    let sub_event = get_one_or_zero_sub_events(event)?;
                    make_fed_event(event, FedEventData::LateToThePartyAdded {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_id: sub_event.map(|e| get_one_team_id(e)).transpose()?,
                        team_nickname: team_nickname.to_string(),
                        sub_event: sub_event.map(SubEvent::from_event),
                    })
                }
                LateToThePartyChange::Removed(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));

                    make_fed_event(event, FedEventData::LateToThePartyRemoved {
                        game: GameEvent::try_from_event(event, unscatter)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
            }
        }
        EventType::ShameDonor => { todo!() }
        EventType::AddedMod => {
            match run_parser(&event, parse_added_mod)? {
                ParsedAddedMod::OverUnder(name) => {
                    make_fed_event(event, FedEventData::AddedOverUnder {
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: name.to_string(),
                    })
                }
                ParsedAddedMod::UnderOver(name) => {
                    make_fed_event(event, FedEventData::AddedUnderOver {
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: name.to_string(),
                    })
                }
                ParsedAddedMod::EnteredPartyTime(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));
                    make_fed_event(event, FedEventData::TeamEnteredPartyTime {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
                ParsedAddedMod::SinkingShip(team_nickname) => {
                    assert!(is_known_team_nickname_uppercase(team_nickname));
                    make_fed_event(event, FedEventData::TeamGainedSinkingShip {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
                ParsedAddedMod::BaseDealing(team_nickname) => {
                    assert!(is_known_team_nickname_uppercase(team_nickname));
                    make_fed_event(event, FedEventData::TeamGainedBaseDealing {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
                ParsedAddedMod::MVP(player_name) => {
                    make_fed_event(event, FedEventData::PlayerNamedMvp {
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: player_name.to_string(),
                        r#mod: get_str_metadata(event, "mod")?.to_string(),
                    })
                }
            }
        }
        EventType::RemovedMod => {
            match run_parser(&event, parse_removed_mod)? {
                ParsedRemovedMod::TeamRemovedFromPartyTimeForPostseason(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));
                    make_fed_event(event, FedEventData::TeamLeftPartyTimeForPostseason {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
                ParsedRemovedMod::TeamUsedFreeWill(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));
                    make_fed_event(event, FedEventData::TeamUsedFreeWill {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                    })
                }
                ParsedRemovedMod::PlayerLostMod((player_name, mod_name)) => {
                    make_fed_event(event, FedEventData::PlayerLostMod {
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: player_name.to_string(),
                        r#mod: get_str_metadata(event, "mod")?.to_string(),
                        mod_name: mod_name.to_string(),
                    })
                }
            }
        }
        EventType::ModExpires => {
            if event.player_tags.is_empty() {
                let (team_nickname, mod_duration) = run_parser(&event, parse_team_mod_expires)?;
                assert!(is_known_team_nickname(team_nickname));
                let mods = get_str_vec_metadata(event, "mods")?;
                make_fed_event(event, FedEventData::TeamModExpires {
                    team_id: get_one_team_id(event)?,
                    team_nickname: team_nickname.to_string(),
                    mods: mods.into_iter().map(String::from).collect(),
                    mod_duration,
                })
            } else {
                let (player_name, mod_duration) = run_parser(&event, parse_player_mod_expires)?;
                let mods = get_str_vec_metadata(event, "mods")?;
                make_fed_event(event, FedEventData::PlayerModExpires {
                    team_id: get_one_team_id(event)?,
                    player_id: get_one_player_id(event)?,
                    player_name: player_name.to_string(),
                    mods: mods.into_iter().map(String::from).collect(),
                    mod_duration,
                })
            }
        }
        EventType::PlayerAddedToTeam => {
            // For now this only parses postseason births, that may need to expand in future
            let team_nickname = run_parser(&event, parse_player_added_to_team)?;

            make_fed_event(event, FedEventData::PostseasonBirth {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                player_id: get_one_player_id(event)?,
                player_name: get_str_metadata(event, "playerName")?.to_string(),
                location: get_int_metadata(event, "location")?
                    .try_into()
                    .map_err(|_| FeedParseError::MissingMetadata {
                        event_type: event.r#type,
                        field: "location",
                    })?,
            })
        }
        EventType::PlayerReplacedByNecromancy => { todo!() }
        EventType::PlayerReplacesReturned => {
            let team_nickname = run_parser(&event, parse_player_replaces_returned)?;

            make_fed_event(event, FedEventData::ReplaceReturnedPlayerFromShadows {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                promoted_player_id: get_uuid_metadata(event, "promotePlayerId")?,
                promoted_player_name: get_str_metadata(event, "promotePlayerName")?.to_string(),
                promoted_location: get_int_metadata(event, "promoteLocation")?
                    .try_into()
                    .map_err(|_| FeedParseError::MissingMetadata {
                        event_type: event.r#type,
                        field: "promoteLocation",
                    })?,
                removed_player_id: get_uuid_metadata(event, "removePlayerId")?,
                removed_player_name: get_str_metadata(event, "removePlayerName")?.to_string(),
                removed_location: get_int_metadata(event, "removeLocation")?
                    .try_into()
                    .map_err(|_| FeedParseError::MissingMetadata {
                        event_type: event.r#type,
                        field: "removeLocation",
                    })?,
            })
        }
        EventType::PlayerRemovedFromTeam => { todo!() }
        EventType::PlayerTraded => { todo!() }
        EventType::PlayerSwap => { todo!() }
        EventType::PlayerMoved => { todo!() }
        EventType::PlayerBornFromIncineration => { todo!() }
        EventType::PlayerStatIncrease => {
            match run_parser(&event, parse_player_stat_increase)? {
                ParsedPlayerStatIncrease::PlayerBoosted(player_name) => {
                    make_fed_event(event, FedEventData::PlayerBoosted {
                        team_id: get_one_team_id(event)?,
                        player_id: get_one_player_id(event)?,
                        player_name: player_name.to_string(),
                        rating_before: get_float_metadata(event, "before")?,
                        rating_after: get_float_metadata(event, "after")?,
                    })
                }
                ParsedPlayerStatIncrease::BottomDwellers(team_nickname) => {
                    assert!(is_known_team_nickname(team_nickname));
                    make_fed_event(event, FedEventData::BottomDwellers {
                        team_id: get_one_team_id(event)?,
                        team_nickname: team_nickname.to_string(),
                        rating_before: get_float_metadata(event, "before")?,
                        rating_after: get_float_metadata(event, "after")?,
                    })
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

            let player_name = run_parser(&event, parse_terminated(" entered the Hall of Flame."))?;

            make_fed_event(event, FedEventData::PlayerCalledBackToHall {
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
            })
        }
        EventType::ExitHallOfFlame => { todo!() }
        EventType::PlayerGainedItem => { todo!() }
        EventType::PlayerLostItem => { todo!() }
        EventType::ReverbFullShuffle => { todo!() }
        EventType::ReverbLineupShuffle => { todo!() }
        EventType::ReverbRotationShuffle => { todo!() }
        EventType::PlayerHatched => {
            // For now this only has the breach events, it will need to be updated for s24
            let player_name = run_parser(&event, parse_player_hatched)?;

            make_fed_event(event, FedEventData::PlayerHatched {
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
            })
        }
        EventType::PlayerEvolves => { todo!() }
        EventType::TeamDivisionMove => {
            // For now this only has the breach events, it will need to be updated for s24
            let (team_nickname, division_name) = run_parser(&event, parse_team_division_move)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(team_nickname, get_str_metadata(event, "teamName")?);
            assert_eq!(division_name, get_str_metadata(event, "divisionName")?);
            let team_id = get_one_team_id(event)?;
            assert_eq!(team_id, get_uuid_metadata(event, "teamId")?);

            make_fed_event(event, FedEventData::TeamJoinedILB {
                team_id,
                team_nickname: team_nickname.to_string(),
                division_id: get_uuid_metadata(event, "divisionId")?,
                division_name: division_name.to_string(),
            })
        }
        EventType::PlayerDivisionMove => {
            let player_name = run_parser(&event, parse_player_division_move)?;

            make_fed_event(event, FedEventData::PlayerJoinedILB {
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
            })
        }
        EventType::TeamWonInternetSeries => {
            let (team_nickname, season_num) = run_parser(&event, parse_team_won_internet_series)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(season_num, event.season + 1);

            make_fed_event(event, FedEventData::TeamWonInternetSeries {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                championships: get_int_metadata(event, "championships")?,
            })
        }
        EventType::EarnedPostseasonSlot => {
            let (team_nickname, season_num) = run_parser(&event, parse_earned_postseason_slot)?;
            assert!(is_known_team_nickname(team_nickname));
            assert_eq!(season_num, event.season + 1);

            make_fed_event(event, FedEventData::EarnedPostseasonSlot {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
            })
        }
        EventType::FinalStandings => {
            let (team_nickname, place, division_name) = run_parser(&event, parse_final_standings)?;
            assert!(is_known_team_nickname(team_nickname));

            make_fed_event(event, FedEventData::FinalStandings {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                place,
                division_name: division_name.to_string(),
            })
        }
        EventType::ModChange => { todo!() }
        EventType::PlayerAlternated => { todo!() }
        EventType::AddedModFromOtherMod => { todo!() }
        EventType::ChangedModFromOtherMod => { todo!() }
        EventType::NecromancyOrPlunderNarration => { todo!() }
        EventType::PlayerPermittedToStay => {
            let player_name = run_parser(&event, parse_terminated(" has been permitted to stay."))?;

            make_fed_event(event, FedEventData::PlayerPermittedToStay {
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
            })
        }
        EventType::DecreeNarration => { todo!() }
        EventType::WillResults => { todo!() }
        EventType::TeamWasShamed => {
            let (shaming_team, shamed_team) = run_parser(&event, parse_team_was_shamed)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            make_fed_event(event, FedEventData::TeamWasShamed {
                shamed_team_id: get_one_team_id(event)?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: get_int_metadata(event, "totalShames")?,
                total_shamings: get_int_metadata(event, "totalShamings")?,
            })
        }
        EventType::TeamDidShame => {
            let (shaming_team, shamed_team) = run_parser(&event, parse_team_did_shame)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            make_fed_event(event, FedEventData::TeamDidShame {
                shaming_team_id: get_one_team_id(event)?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: get_int_metadata(event, "totalShames")?,
                total_shamings: get_int_metadata(event, "totalShamings")?,
            })
        }
        EventType::Investigation => {
            make_fed_event(event, FedEventData::Investigation {
                player_id: get_one_player_id(event)?,
                message: event.description.clone(),
            })
        }
        EventType::Announcement => { todo!() }
        EventType::RunsScored => { todo!() }
        EventType::WinCollectedRegular => { todo!() }
        EventType::WinCollectedPostseason => { todo!() }
        EventType::GameOver => { todo!() }
        EventType::StormWarning => { todo!() }
        EventType::Snowflakes => { todo!() }
        EventType::Sun2SetWin => {
            let team_name = run_parser(&event, parse_sun2_set_win)?;
            assert!(is_known_team_nickname(team_name));
            make_fed_event(event, FedEventData::Sun2SetWin {
                team_id: get_one_team_id(event)?,
                team_nickname: team_name.to_string(),
            })
        }
        EventType::BlackHoleSwallowedWin => {
            let team_name = run_parser(&event, parse_black_hole_swallowed_win)?;
            assert!(is_known_team_nickname(team_name));
            make_fed_event(event, FedEventData::BlackHoleSwallowedWin {
                team_id: get_one_team_id(event)?,
                team_nickname: team_name.to_string(),
            })
        }
        EventType::RemovedModFromOtherMod => { todo!() }
        EventType::PostseasonAdvance => {
            let (team_nickname, round_num, season_num) = run_parser(&event, parse_postseason_advance)?;
            assert!(is_known_team_nickname(team_nickname));
            make_fed_event(event, FedEventData::PostseasonAdvance {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                round: round_num,
                season: season_num,
            })
        }
        EventType::HighPressure => { todo!() }
        EventType::LineupSorted => {
            // This happened as a top-level event exactly once (and really it should have been a
            // child of the lovers' getting Base Dealing)
            parse_fixed_description(event, "The Lovers' lineup has been optimized.",
                                    FedEventData::LineupSorted {
                                        team_id: get_one_team_id(event)?,
                                        team_nickname: "Lovers".to_string(),
                                    })
        }
        EventType::PostseasonEliminated => {
            let (team_nickname, season_num) = run_parser(&event, parse_postseason_eliminated)?;
            assert!(is_known_team_nickname(team_nickname));
            make_fed_event(event, FedEventData::PostseasonEliminated {
                team_id: get_one_team_id(event)?,
                team_nickname: team_nickname.to_string(),
                season: season_num,
            })
        }
    }
}

fn zip_mod_change_events(names: Vec<&str>, children: &[EventuallyEvent]) -> Result<Vec<ModChangeSubEventWithNamedPlayer>, FeedParseError> {
    names.iter().zip_eq(children)
        .map(|(name, sub_event)| Ok(ModChangeSubEventWithNamedPlayer {
            sub_event: SubEvent::from_event(sub_event),
            team_id: get_one_team_id(sub_event)?,
            player_id: get_one_player_id(sub_event)?,
            player_name: name.to_string(),
        }))
        .collect::<Result<_, _>>()
}

fn get_one_player_id_advanced(event: &EventuallyEvent, has_extra_id: bool) -> Result<Uuid, FeedParseError> {
    if has_extra_id {
        let (&id1, &id2) = event.player_tags.iter().collect_tuple()
            .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                event_type: event.r#type,
                tag_type: "player",
                expected_num: 2,
                actual_num: event.player_tags.len(),
            })?;
        if id1 != id2 {
            Err(FeedParseError::ExpectedEqualTags {
                event_type: event.r#type,
                tag_type: "player",
                tag1: id1,
                tag2: id2,
            })
        } else {
            Ok(id1)
        }
    } else {
        get_one_player_id(event)
    }
}

fn extract_spicy_event(children: &[EventuallyEvent], spicy_status: ParsedSpicyStatus) -> Result<(&[EventuallyEvent], SpicyStatus), FeedParseError> {
    Ok(match spicy_status {
        ParsedSpicyStatus::None => { (children, SpicyStatus::None) }
        ParsedSpicyStatus::HeatingUp => { (children, SpicyStatus::HeatingUp) }
        ParsedSpicyStatus::RedHot => {
            // TODO Is the spicy event always the last? first? neither?
            if let Some((spicy_event, children)) = children.split_last() {
                if spicy_event.r#type == EventType::AddedMod {
                    (children, SpicyStatus::RedHot(Some(ModChangeSubEvent {
                        sub_event: SubEvent::from_event(spicy_event),
                        team_id: get_one_team_id(spicy_event)?,
                    })))
                } else {
                    (&children, SpicyStatus::RedHot(None))
                }
            } else {
                (&children, SpicyStatus::RedHot(None))
            }
        }
    })
}

fn extract_cooled_off_event<'e, 't>(event: &'e EventuallyEvent, children: &'e [EventuallyEvent], cooled_off: bool, player_tags: &'t [Uuid]) -> Result<(&'e [EventuallyEvent], Option<ModChangeSubEventWithPlayer>, &'t [Uuid]), FeedParseError> {
    Ok(match cooled_off {
        false => { (children, None, player_tags) }
        true => {
            // TODO Is the spicy event always the last? first? neither?
            if let Some((cooled_off_event, children)) = children.split_last() {
                // TODO Make this assert into a propagated error
                assert_eq!(cooled_off_event.r#type, EventType::RemovedMod);
                let (&player_id, remaining_tags) = player_tags.split_last()
                    .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                        event_type: event.r#type,
                        tag_type: "player",
                        expected_num: 1, // at least
                        actual_num: 0,
                    })?;

                (children, Some(ModChangeSubEventWithPlayer {
                    sub_event: SubEvent::from_event(cooled_off_event),
                    team_id: get_one_team_id(cooled_off_event)?,
                    player_id,
                }), remaining_tags)
            } else {
                Err(FeedParseError::MissingChild {
                    event_type: event.r#type,
                    expected_num_children: 1,  // at least one
                })?
            }
        }
    })
}

fn merge_scores_with_ids(
    scores: ParsedScores,
    scorer_ids: &[Uuid],
    children: &[EventuallyEvent],
    event_type: EventType,
    extra_player_tags: usize,
) -> Result<(ScoreInfo, Option<StoppedInhabiting>), FeedParseError> {
    let mut children = children.iter();

    if scorer_ids.len() != scores.scorers.len() {
        return Err(FeedParseError::WrongNumberOfTags {
            event_type,
            tag_type: "player",
            expected_num: scores.scorers.len() + extra_player_tags,
            actual_num: scorer_ids.len() + extra_player_tags,
        });
    }

    let scoring_players = scores.scorers.into_iter().zip(scorer_ids)
        .map(|(score, &scorer_id)| Ok(ScoringPlayer {
            player_id: scorer_id,
            player_name: score.to_string(),
        }))
        .collect::<Result<_, _>>()?;


    let free_refills = scores.refillers.into_iter()
        .map(|name| {
            if let Some(event) = children.next() {
                let expected_description = format!("{} used their Free Refill.", name);
                if event.description == expected_description {
                    Ok(FreeRefill {
                        sub_event: SubEvent::from_event(event),
                        player_name: name.to_string(),
                        player_id: get_one_player_id(event)?,
                        team_id: get_one_team_id(event)?,
                        sub_play: get_sub_play(event)?,
                    })
                } else {
                    Err(FeedParseError::UnexpectedDescription {
                        event_type,
                        description: event.description.clone(),
                        expected: expected_description,
                    })
                }
            } else {
                Err(FeedParseError::MissingChild {
                    event_type,
                    expected_num_children: -1,
                })
            }
        })
        .collect::<Result<_, _>>()?;

    let result = ScoreInfo {
        scoring_players,
        free_refills,
    };

    if let Some(extra_child) = children.next() {
        if extra_child.r#type == EventType::RemovedMod && extra_child.metadata.other.as_object()
            .and_then(|o| o.get("mod"))
            .and_then(|m| m.as_str())
            .map(|m| m == "INHABITING")
            .unwrap_or(false) {
            let name = run_parser(extra_child, parse_stopped_inhabiting)?;
            Ok((result, Some(StoppedInhabiting {
                sub_event: SubEvent::from_event(extra_child),
                inhabiting_player_name: name.to_string(),
                inhabiting_player_id: get_one_player_id(extra_child)?,
                inhabiting_player_team_id: if extra_child.team_tags.is_empty() {
                    None
                } else {
                    Some(get_one_team_id(extra_child)?)
                },
            })))
        } else {
            Err(FeedParseError::MissingChild {
                event_type,
                expected_num_children: 0,
            })
        }
    } else {
        Ok((result, None))
    }
}

fn make_free_refill(event_type: EventType, children: &mut Iter<EventuallyEvent>, refiller_name: &str) -> Result<FreeRefill, FeedParseError> {
    let child = children.next()
        .ok_or_else(|| {
            FeedParseError::MissingChild {
                event_type,
                expected_num_children: -1, // Unknown at this point in the computation
            }
        })?;

    let (&team_id, ) = child.team_tags.iter().collect_tuple()
        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
            event_type,
            tag_type: "team",
            expected_num: 1,
            actual_num: child.team_tags.len(),
        })?;

    let (&player_id, ) = child.player_tags.iter().collect_tuple()
        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
            event_type,
            tag_type: "player",
            expected_num: 1,
            actual_num: child.player_tags.len(),
        })?;

    Ok(FreeRefill {
        sub_event: SubEvent::from_event(child),
        player_name: refiller_name.to_string(),
        player_id,
        team_id,
        sub_play: get_sub_play(child)?,
    })
}

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

fn is_known_team_nickname_uppercase(name: &str) -> bool {
    KNOWN_TEAM_NICKNAMES.iter().any(|known| known.to_ascii_uppercase() == name)
}

type ParserError<'a> = nom::error::VerboseError<&'a str>;
type ParserResult<'a, Out> = IResult<&'a str, Out, ParserError<'a>>;

fn run_parser<'a, F, Out>(event: &'a EventuallyEvent, parser: F) -> Result<Out, FeedParseError>
    where F: Fn(&'a str) -> ParserResult<'a, Out> {
    let (_, output) = terminated(parser, eof)(&event.description)
        .finish()
        .map_err(|e| FeedParseError::DescriptionParseError {
            event_type: event.r#type,
            err: convert_error(&event.description as &str, e),
        })?;

    Ok(output)
}

fn parse_fixed_description(event: &EventuallyEvent, expected_description: &'static str, data: FedEventData) -> Result<FedEvent, FeedParseError> {
    if event.description == expected_description {
        make_fed_event(event, data)
    } else {
        Err(FeedParseError::UnexpectedDescription {
            event_type: event.r#type,
            description: event.description.clone(),
            expected: expected_description.to_string(),
        })
    }
}

fn make_fed_event(feed_event: &EventuallyEvent, data: FedEventData) -> Result<FedEvent, FeedParseError> {
    Ok(FedEvent {
        id: feed_event.id,
        created: feed_event.created,
        sim: feed_event.sim.clone(),
        tournament: feed_event.tournament,
        season: feed_event.season,
        day: feed_event.day,
        phase: feed_event.phase.try_into().map_err(|_| FeedParseError::UnknownPhase {
            phase: feed_event.phase,
            event_type: feed_event.r#type,
        })?,
        nuts: feed_event.nuts,
        data,
    })
}

fn parse_terminated(tag_content: &str) -> impl Fn(&str) -> ParserResult<&str> + '_ {
    move |input| {
        let (input, parsed_value) = if tag_content == "." {
            alt((
                // The Kaj Statter Jr. rule
                verify(recognize(terminated(take_until1(".."), tag("."))), |s: &str| !s.contains('\n')),
                verify(take_until1(tag_content), |s: &str| !s.contains('\n')),
            ))(input)
        } else {
            verify(take_until1(tag_content), |s: &str| !s.contains('\n'))(input)
        }?;
        let (input, _) = tag(tag_content)(input)?;

        Ok((input, parsed_value))
    }
}

// This is for use in place of parse_terminated when the only remaining text in the string is ".",
// and so you can't use parse_terminated because that would improperly cut off names with periods
// like "Kaj Statter Jr."
fn parse_until_period_eof(input: &str) -> ParserResult<&str> {
    let (input, replacement_name_with_dot) = is_not("\n")(input)?;
    let replacement_name = replacement_name_with_dot.strip_suffix(".")
        .ok_or_else(|| {
            // I can't figure out how to make an error myself so I'm just gonna unwrap a fail
            fail::<_, (), _>(replacement_name_with_dot).unwrap_err()
        })?;

    Ok((input, replacement_name))
}

fn parse_half_inning(input: &str) -> ParserResult<(bool, i32, &str)> {
    let (input, top_of_inning) = alt((
        tag("Top").map(|_| true),
        tag("Bottom").map(|_| false),
    ))(input)?;

    let (input, _) = tag(" of ")(input)?;
    let (input, inning) = parse_whole_number(input)?;

    let (input, _) = tag(", ")(input)?;
    let (input, team_name) = parse_terminated(" batting.")(input)?;

    Ok((input, (top_of_inning, inning, team_name)))
}

fn parse_whole_number(input: &str) -> ParserResult<i32> {
    map_res(digit1, str::parse)(input)
}

fn parse_batter_up(input: &str) -> ParserResult<(&str, Option<&str>, &str, Option<&str>, bool)> {
    let (input, repeating) = opt(parse_terminated("is Repeating!\n"))(input)?;
    let (input, (batter_name, inhabiting_name)) = alt((
        // NOTE order matters here. inhabiting must be first
        parse_batter_up_inhabiting,
        parse_terminated(" batting for the ").map(|n| (n, None)),
    ))(input)?;
    // This is going to fail if a team ever has a period or comma in it
    let (input, team_name) = take_till1(|c| c == ',' || c == '.')(input)?;
    let (input, wielding_item) = alt((
        // No legacy item
        tag(".").map(|_| None),
        // Legacy item
        parse_wielding_item.map(|s| Some(s))
    ))(input)?;

    Ok((input, (batter_name, inhabiting_name, team_name, wielding_item, repeating.is_some())))
}

fn parse_batter_up_inhabiting(input: &str) -> ParserResult<(&str, Option<&str>)> {
    let (input, batter_name) = parse_terminated(" is Inhabiting ")(input)?;
    let (input, inhabiting_name) = parse_terminated("!\n")(input)?;
    let (input, _) = tag(batter_name)(input)?;
    let (input, _) = tag(" batting for the ")(input)?;

    Ok((input, (batter_name, Some(inhabiting_name))))
}

fn parse_wielding_item(input: &str) -> ParserResult<&str> {
    let (input, _) = tag(", wielding ")(input)?;
    // can't use parse_terminated because the terminator would be "." and "the Iffey Jr." exists
    if let Some((idx, end)) = input.rmatch_indices('.').next() {
        let (input, item_name) = (end, &input[0..idx]);
        let (input, _) = tag(".")(input)?;
        Ok((input, item_name))
    } else {
        fail(input)
    }
}

fn parse_ball(input: &str) -> ParserResult<(i32, i32)> {
    let (input, _) = tag("Ball. ")(input)?;
    let (input, count) = parse_count(input)?;

    Ok((input, count))
}

fn parse_foul_ball(input: &str) -> ParserResult<(i32, i32)> {
    let (input, _) = tag("Foul Ball. ")(input)?;
    let (input, count) = parse_count(input)?;

    Ok((input, count))
}

pub enum StrikeType {
    Swinging,
    Looking,
    Flinching,
}

fn parse_strike(input: &str) -> ParserResult<(StrikeType, i32, i32)> {
    let (input, _) = tag("Strike, ")(input)?;
    let (input, strike_type) = alt((
        tag("swinging. ").map(|_| StrikeType::Swinging),
        tag("looking. ").map(|_| StrikeType::Looking),
        tag("flinching. ").map(|_| StrikeType::Flinching),
    ))(input)?;
    let (input, (balls, strikes)) = parse_count(input)?;

    Ok((input, (strike_type, balls, strikes)))
}

fn parse_count(input: &str) -> ParserResult<(i32, i32)> {
    // this should handle double-digit counts because i know how blaseball is
    let (input, balls) = parse_whole_number(input)?;
    let (input, _) = tag("-")(input)?;
    let (input, strikes) = parse_whole_number(input)?;

    Ok((input, (balls, strikes)))
}

fn parse_flyout(input: &str) -> ParserResult<(&str, &str, ParsedScores, bool)> {
    let (input, batter_name) = parse_terminated(" hit a flyout to ")(input)?;
    let (input, fielder_name) = parse_terminated(".")(input)?;

    let (input, scores) = parse_scores(" tags up and scores!")(input)?;

    let (input, cooled_off) = parse_cooled_off(batter_name)(input)?;

    Ok((input, (batter_name, fielder_name, scores, cooled_off)))
}

enum ParsedGroundOut<'a> {
    Simple {
        batter_name: &'a str,
        fielder_name: &'a str,
    },
    FieldersChoice {
        runner_out_name: &'a str,
        batter_name: &'a str,
        base: i32,
    },
    DoublePlay {
        batter_name: &'a str,
    },
}

fn parse_ground_out(input: &str) -> ParserResult<(ParsedGroundOut, ParsedScores, bool)> {
    alt((parse_simple_ground_out, parse_fielders_choice, parse_double_play))(input)
}

fn parse_simple_ground_out(input: &str) -> ParserResult<(ParsedGroundOut, ParsedScores, bool)> {
    let (input, batter_name) = parse_terminated(" hit a ground out to ")(input)?;
    let (input, fielder_name) = parse_terminated(".")(input)?;

    let (input, scores) = parse_scores(" advances on the sacrifice.")(input)?;

    let (input, cooled_off) = parse_cooled_off(batter_name)(input)?;

    Ok((input, (ParsedGroundOut::Simple { batter_name, fielder_name }, scores, cooled_off)))
}

fn parse_fielders_choice(input: &str) -> ParserResult<(ParsedGroundOut, ParsedScores, bool)> {
    let (input, runner_out_name) = parse_terminated(" out at ")(input)?;
    let (input, base) = parse_named_base(input)?;
    let (input, _) = tag(" base.")(input)?;

    // Scores and free refills are split by fielder's choice text
    let (input, scorers) = many0(parse_score(" scores!"))(input)?;

    let (input, _) = tag("\n")(input)?;
    let (input, batter_name) = parse_terminated(" reaches on fielder's choice.")(input)?;

    let (input, refillers) = many0(parse_free_refill)(input)?;

    let (input, cooled_off) = parse_cooled_off(batter_name)(input)?;

    let scores = ParsedScores { scorers, refillers };

    Ok((input, (ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base }, scores, cooled_off)))
}

fn parse_double_play(input: &str) -> ParserResult<(ParsedGroundOut, ParsedScores, bool)> {
    let (input, batter_name) = parse_terminated(" hit into a double play!")(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    let (input, cooled_off) = parse_cooled_off(batter_name)(input)?;

    Ok((input, (ParsedGroundOut::DoublePlay { batter_name }, scores, cooled_off)))
}

fn parse_hit(input: &str) -> ParserResult<(&str, i32, ParsedScores, ParsedSpicyStatus)> {
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_bases) = alt((
        tag("Single!").map(|_| 1),
        tag("Double!").map(|_| 2),
        tag("Triple!").map(|_| 3),
        tag("Quadruple!").map(|_| 4),
    ))(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    let (input, spicy_status) = parse_spicy_status(batter_name)(input)?;

    Ok((input, (batter_name, num_bases, scores, spicy_status)))
}

#[derive(PartialEq)]
enum ParsedSpicyStatus {
    None,
    HeatingUp,
    RedHot,
}

fn parse_spicy_status(batter_name: &str) -> impl FnMut(&str) -> ParserResult<ParsedSpicyStatus> + '_ {
    move |input: &str| {
        let (input, heating_up) = opt(alt((
            terminated(terminated(char('\n'), tag(batter_name)), tag(" is Heating Up!")).map(|_| ParsedSpicyStatus::HeatingUp),
            terminated(terminated(char('\n'), tag(batter_name)), tag(" is Red Hot!")).map(|_| ParsedSpicyStatus::RedHot),
        )))(input)?;
        Ok((input, heating_up.unwrap_or(ParsedSpicyStatus::None)))
    }
}

fn parse_cooled_off(batter_name: &str) -> impl FnMut(&str) -> ParserResult<bool> + '_ {
    move |input: &str| {
        let (input, cooled_off) = opt(
            terminated(terminated(char('\n'), tag(batter_name)), tag(" cooled off.")),
        )(input)?;
        Ok((input, cooled_off.is_some()))
    }
}

struct ParsedScores<'a> {
    scorers: Vec<&'a str>,
    refillers: Vec<&'a str>,
}

impl ParsedScores<'_> {
    fn empty() -> Self {
        ParsedScores {
            scorers: Vec::new(),
            refillers: Vec::new(),
        }
    }
}

fn parse_free_refill(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("\n")(input)?;
    let (input, name) = parse_terminated(" used their Free Refill.\n")(input)?;
    let (input, _) = tag(name)(input)?;
    let (input, _) = tag(" Refills the In!")(input)?;

    Ok((input, name))
}

fn parse_scores<'a>(score_label: &'static str) -> impl FnMut(&'a str) -> ParserResult<ParsedScores<'a>> {
    move |input| {
        let (input, scorers) = many0(parse_score(score_label))(input)?;
        let (input, refillers) = many0(parse_free_refill)(input)?;

        Ok((input, ParsedScores {
            scorers,
            refillers,
        }))
    }
}

fn parse_score(score_label: &'static str) -> impl Fn(&str) -> ParserResult<&str> {
    move |input| {
        let (input, _) = tag("\n")(input)?;
        let (input, name) = parse_terminated(score_label)(input)?;

        Ok((input, name))
    }
}

fn parse_hr(input: &str) -> ParserResult<(bool, &str, i32, Vec<&str>, ParsedSpicyStatus)> {
    let (input, magmatic_player) = opt(parse_terminated(" is Magmatic!\n"))(input)?;
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_runs) = alt((
        tag("solo home run!").map(|_| 1),
        tag("2-run home run!").map(|_| 2),
        tag("3-run home run!").map(|_| 3),
        tag("grand slam!").map(|_| 4), // dunno what happens with a pentaslam...
    ))(input)?;

    let (input, free_refillers) = many0(parse_free_refill)(input)?;

    if let Some(name) = magmatic_player {
        assert_eq!(name, batter_name);
    }

    let (input, spicy_status) = parse_spicy_status(batter_name)(input)?;

    Ok((input, (magmatic_player.is_some(), batter_name, num_runs, free_refillers, spicy_status)))
}

fn parse_stolen_base(input: &str) -> ParserResult<(&str, i32, bool, bool, Option<&str>)> {
    let (input, (runner_name, is_successful)) = alt((
        parse_terminated(" steals ").map(|n| (n, true)),
        parse_terminated(" gets caught stealing ").map(|n| (n, false)),
    ))(input)?;

    let (input, num_runs) = parse_named_base(input)?;

    // Decide whether to be excited
    let (input, _) = tag(if is_successful { " base!" } else { " base." })(input)?;

    let (input, blaserunning) = opt(preceded(tag("\n"), preceded(tag(runner_name), tag(" scores with Blaserunning!"))))(input)?;
    let (input, free_refill) = opt(parse_free_refill)(input)?;

    Ok((input, (runner_name, num_runs, is_successful, blaserunning.is_some(), free_refill)))
}

fn parse_named_base(input: &str) -> ParserResult<i32> {
    alt((
        tag("first").map(|_| 1),
        tag("second").map(|_| 2),
        tag("third").map(|_| 3),
        tag("fourth").map(|_| 4),
        tag("fifth").map(|_| 5),
    ))(input)
}

enum ParsedStrikeout<'a> {
    Swinging(&'a str),
    Looking(&'a str),

    Charm {
        charmer_name: &'a str,
        charmed_name: &'a str,
        num_swings: i32,
    },
}

fn parse_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    alt((
        parse_normal_strikeout,
        parse_charm_strikeout
    ))(input)
}

fn parse_normal_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    let (input, batter_name) = parse_terminated(" strikes out ")(input)?;
    let (input, is_swinging) = alt((
        tag("swinging.").map(|_| true),
        tag("looking.").map(|_| false),
    ))(input)?;

    Ok((input, if is_swinging { ParsedStrikeout::Swinging(batter_name) } else { ParsedStrikeout::Looking(batter_name) }))
}

fn parse_charm_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    let (input, charmer_name) = parse_terminated(" charmed ")(input)?;
    let (input, charmed_name) = parse_terminated("!\n")(input)?;
    let (input, charmed_name2) = parse_terminated(" swings ")(input)?;
    let (input, num_swings) = parse_whole_number(input)?;
    let (input, _) = tag(" times to strike out willingly!")(input)?;

    // I believe these should always be equal
    assert_eq!(charmed_name, charmed_name2);

    Ok((input, ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings }))
}

enum ParsedWalk<'s> {
    Ordinary((&'s str, ParsedScores<'s>, Option<i32>)),
    Charm((&'s str, &'s str, ParsedScores<'s>)),
}

fn parse_walk(input: &str) -> ParserResult<ParsedWalk> {
    alt((
        parse_ordinary_walk.map(|res| ParsedWalk::Ordinary(res)),
        parse_charm_walk.map(|res| ParsedWalk::Charm(res)),
    ))(input)
}

fn parse_base_instincts(input: &str) -> ParserResult<i32> {
    let (input, _) = tag("\nBase Instincts take them directly to ")(input)?;
    let (input, which) = alt((
        tag("second").map(|_| 2),
        tag("third").map(|_| 3),
        tag("fourth").map(|_| 4), // when fifth base is present
    ))(input)?;
    let (input, _) = tag(" base!")(input)?;

    Ok((input, which))
}

fn parse_ordinary_walk(input: &str) -> ParserResult<(&str, ParsedScores, Option<i32>)> {
    let (input, batter_name) = parse_terminated(" draws a walk.")(input)?;

    let (input, base_instincts) = opt(parse_base_instincts)(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    Ok((input, (batter_name, scores, base_instincts)))
}

fn parse_charm_walk(input: &str) -> ParserResult<(&str, &str, ParsedScores)> {
    // This will need to be updated if anyone charms in a run
    let (input, batter_name) = parse_terminated(" charms ")(input)?;
    let (input, pitcher_name) = parse_terminated("!\n")(input)?;
    let (input, _) = tag(batter_name)(input)?;
    let (input, _) = tag(" walks to first base.")(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    Ok((input, (batter_name, pitcher_name, scores)))
}

fn parse_inning_end(input: &str) -> ParserResult<(i32, Vec<&str>)> {
    let (input, _) = tag("Inning ")(input)?;
    let (input, inning_num) = parse_whole_number(input)?;
    let (input, _) = tag(" is now an Outing.")(input)?;
    let (input, lost_triple_threat) = many0(preceded(tag("\n"), parse_terminated(" is no longer a Triple Threat.")))(input)?;

    Ok((input, (inning_num, lost_triple_threat)))
}

fn parse_stopped_inhabiting(input: &str) -> ParserResult<&str> {
    parse_terminated(" stopped Inhabiting.")(input)
}

fn parse_game_end(input: &str) -> ParserResult<((&str, f32), (&str, f32))> {
    // This is a bit tricky because it's a string of arbitrary words (a team name) followed by an
    // arbitrary number (score)
    let (input, winning_team_name) = take_till(AsChar::is_dec_digit)(input)?;
    let (input, winning_team_score) = float(input)?;
    let (input, _) = tag(", ")(input)?;
    let (input, losing_team_name) = take_till(AsChar::is_dec_digit)(input)?;
    let (input, losing_team_score) = float(input)?;

    fn fix_team(name: &str, score: f32) -> (&str, f32) {
        if let Some(n) = name.strip_suffix(" -") {
            (n, -score)
        } else {
            (name.strip_suffix(" ").unwrap(), score)
        }
    }

    let (winning_team_name, winning_team_score) = fix_team(winning_team_name, winning_team_score.into());
    let (losing_team_name, losing_team_score) = fix_team(losing_team_name, losing_team_score.into());

    // Just checking that my assumption is correct. It's <= because of 20.3
    assert!(losing_team_score <= winning_team_score);

    // The parsers for *_team_name should always leave us with a space at the end
    Ok((input, ((winning_team_name, winning_team_score),
                (losing_team_name, losing_team_score))))
}

enum MildPitchType<'a> {
    Ball((i32, i32)),
    Walk(&'a str),
}

fn parse_mild_pitch_ball(input: &str) -> ParserResult<MildPitchType> {
    // Fun fact: Can't reuse the ball parser because it looks for a comma but this has a period
    let (input, _) = tag("Ball, ")(input)?;
    let (input, count) = parse_count(input)?;
    let (input, _) = tag(".")(input)?;

    Ok((input, MildPitchType::Ball(count)))
}

fn parse_mild_pitch(input: &str) -> ParserResult<(&str, MildPitchType, bool, ParsedScores)> {
    let (input, pitcher_name) = parse_terminated(" throws a Mild pitch!\n")(input)?;

    // Fun fact: Can't reuse the ball parser because it looks for a comma but this has a period
    let (input, pitch_type) = alt((
        parse_mild_pitch_ball,
        parse_terminated(" draws a walk.").map(|name| MildPitchType::Walk(name))
    ))(input)?;

    let (input, runners_advance) = opt(tag("\nRunners advance on the pathetic play!"))(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    Ok((input, (pitcher_name, pitch_type, runners_advance.is_some(), scores)))
}

fn parse_coffee_bean(input: &str) -> ParserResult<(&str, &str, &str, bool, bool)> {
    let (input, player_name) = parse_terminated(" is Beaned by a ")(input)?;
    let (input, roast) = parse_terminated(" roast with ")(input)?;
    let (input, notes) = parse_terminated(".\n")(input)?;
    let (input, player_name2) = parse_terminated(" is ")(input)?;
    assert_eq!(player_name, player_name2);
    let (input, (wired, gained)) = alt((
        tag("Wired!").map(|_| (true, true)),
        tag("no longer Wired!").map(|_| (true, false)),
        tag("Tired.").map(|_| (false, true)),
        tag("no longer Tired!").map(|_| (false, false)),
    ))(input)?;

    Ok((input, (player_name2, roast, notes, wired, gained)))
}

fn parse_gain_free_refill(input: &str) -> ParserResult<(&str, &str, &str, &str)> {
    let (input, player_name) = parse_terminated(" is Poured Over with a ")(input)?;
    let (input, roast) = parse_terminated(" roast blending ")(input)?;
    let (input, ingredient1) = parse_terminated(" and ")(input)?;
    let (input, ingredient2) = parse_terminated("!\n")(input)?;
    let (input, _) = tag(player_name)(input)?;
    let (input, _) = tag(" got a Free Refill.")(input)?;

    Ok((input, (player_name, roast, ingredient1, ingredient2)))
}

enum IncinerationBlockedReason {
    Magmatic,
    Fireproof,
}

fn parse_incineration_blocked(input: &str) -> ParserResult<(&str, IncinerationBlockedReason)> {
    let (input, _) = tag("Rogue Umpire tried to incinerate ")(input)?;
    let (input, player_name) = parse_terminated(", but ")(input)?;
    let (input, blocked_reason) = alt((
        pair(tag(player_name), tag(" ate the flame! They became Magmatic!")).map(|_| IncinerationBlockedReason::Magmatic),
        tag("they're Fireproof! The Umpire was incinerated instead!").map(|_| IncinerationBlockedReason::Fireproof),
    ))(input)?;
    Ok((input, (player_name, blocked_reason)))
}

fn parse_player_mod_expires(input: &str) -> ParserResult<(&str, ModDuration)> {
    // This message treats possessives of names ending in s correctly
    let (input, player_name) = alt((
        parse_terminated("'s "),
        parse_terminated("' ")
    ))(input)?;
    let (input, duration) = alt((
        tag("game").map(|_| ModDuration::Game),
        tag("seasonal").map(|_| ModDuration::Seasonal),
    ))(input)?;
    let (input, _) = tag(" mods wore off.")(input)?;
    Ok((input, (player_name, duration)))
}

fn parse_team_mod_expires(input: &str) -> ParserResult<(&str, ModDuration)> {
    let (input, _) = tag("The ")(input)?;
    // This message treats possessives of names ending in s correctly
    let (input, player_name) = alt((
        parse_terminated("'s "),
        parse_terminated("' ")
    ))(input)?;
    let (input, duration) = alt((
        tag("game").map(|_| ModDuration::Game),
        tag("seasonal").map(|_| ModDuration::Seasonal),
    ))(input)?;
    let (input, _) = tag(" mods wore off.")(input)?;
    Ok((input, (player_name, duration)))
}

pub enum ParsedBlooddrainAction<'s> {
    AddBall,
    RemoveBall,
    AddStrike(Option<&'s str>),
    // if there's a strikeout looking, there's a name here
    RemoveStrike,
    AddOut,
    RemoveOut,
}

fn parse_blooddrain_action(drinker_name: &str) -> impl Fn(&str) -> ParserResult<ParsedBlooddrainAction> + '_ {
    move |input: &str| {
        let (input, _) = tag(drinker_name)(input)?;
        let (input, action) = alt((
            // preceded(tag(" increased their "), terminated(parse_category, tag(" ability!"))).map(|ability| BlooddrainAction::IncreaseAbility(ability)),
            tag(" adds a Ball!").map(|_| ParsedBlooddrainAction::AddBall),
            tag(" removes a Ball!").map(|_| ParsedBlooddrainAction::RemoveBall),
            preceded(tag(" adds a Strike!\n"), parse_terminated(" strikes out looking.")).map(|name| ParsedBlooddrainAction::AddStrike(Some(name))),
            tag(" adds a Strike!").map(|_| ParsedBlooddrainAction::AddStrike(None)),
            tag(" removes a Strike!").map(|_| ParsedBlooddrainAction::RemoveStrike),
            tag(" adds a Out!").map(|_| ParsedBlooddrainAction::AddOut),
            tag(" removes a Out!").map(|_| ParsedBlooddrainAction::RemoveOut),
        ))(input)?;

        Ok((input, action))
    }
}

fn parse_blooddrain_ability<'a>(drinker_name: &'a str, category: &'a str) -> impl Fn(&str) -> ParserResult<()> + 'a {
    move |input: &str| {
        let (input, _) = tag(drinker_name)(input)?;
        let (input, _) = tag(" increased their ")(input)?;
        let (input, _) = tag(category)(input)?;
        let (input, _) = tag(" ability!")(input)?;

        Ok((input, ()))
    }
}

fn parse_blooddrain_siphon(input: &str) -> ParserResult<(&str, &str, AttrCategory, Option<ParsedBlooddrainAction>)> {
    let (input, _) = tag("The Blooddrain gurgled!\n")(input)?;
    let (input, drinker_name) = parse_terminated("'s Siphon activates!\n")(input)?;
    let (input, _) = tag(drinker_name)(input)?;
    let (input, _) = tag(" siphoned some of ")(input)?;
    let (input, drunk_name) = parse_terminated("'s ")(input)?;
    let (input, category) = parse_category(input)?;
    let (input, _) = tag(" ability!\n")(input)?;
    let (input, action) = alt((
        parse_blooddrain_action(drinker_name).map(|a| Some(a)),
        parse_blooddrain_ability(drinker_name, &category.to_string()).map(|()| None),
    ))(input)?;

    Ok((input, (drinker_name, drunk_name, category, action)))
}

fn parse_category(input: &str) -> ParserResult<AttrCategory> {
    alt((
        tag("hitting").map(|_| AttrCategory::Batting),
        tag("baserunning").map(|_| AttrCategory::Baserunning),
        tag("pitching").map(|_| AttrCategory::Pitching),
        tag("defensive").map(|_| AttrCategory::Defense),
    ))(input)
}

fn parse_friend_of_crows(input: &str) -> ParserResult<(Option<&str>, &str)> {
    let (input, pitcher_name) = opt(parse_terminated(" calls upon their Friends!\n"))(input)?;
    let (input, _) = tag("A murder of Crows ambush ")(input)?;
    let (input, batter_name) = parse_terminated("!\nThey run to safety, resulting in an out.")(input)?;

    Ok((input, (pitcher_name, batter_name)))
}

fn parse_black_hole_swallowed_win(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The Black Hole swallowed a Win from the ")(input)?;
    let (input, team_name) = parse_terminated("!")(input)?;

    Ok((input, team_name))
}

fn parse_sun2_set_win(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Sun 2 set a Win upon the ")(input)?;
    let (input, team_name) = parse_terminated(".")(input)?;

    Ok((input, team_name))
}

fn parse_sun2(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ")(input)?;
    let (input, scoring_team) = parse_terminated(" collect 10! Sun 2 smiles.\nSun 2 set a Win upon the ")(input)?;
    let (input, _) = tag(scoring_team)(input)?;
    let (input, _) = tag(".")(input)?;

    Ok((input, scoring_team))
}

fn parse_black_hole(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ")(input)?;
    let (input, scoring_team) = parse_terminated(" collect 10!\nThe Black Hole swallows the Runs and a ")(input)?;
    let (input, victim_team) = parse_terminated(" Win.")(input)?;

    Ok((input, (scoring_team, victim_team)))
}

fn parse_team_did_shame(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ")(input)?;
    let (input, shaming_team) = parse_terminated(" shamed the ")(input)?;
    let (input, shamed_team) = parse_terminated(".")(input)?;

    Ok((input, (shaming_team, shamed_team)))
}

fn parse_team_was_shamed(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ")(input)?;
    let (input, shamed_team) = parse_terminated(" were shamed by the ")(input)?;
    let (input, shaming_team) = parse_terminated(".")(input)?;

    Ok((input, (shaming_team, shamed_team)))
}

fn parse_allergic_reaction(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" swallowed a stray peanut and had an allergic reaction!")(input)?;

    Ok((input, player_name))
}

fn parse_feedback(input: &str) -> ParserResult<(&str, &str, ActivePositionType)> {
    let (input, _) = tag("Reality flickers. Things look different ...\n")(input)?;
    let (input, player1_name) = parse_terminated(" and ")(input)?;
    let (input, player2_name) = parse_terminated(" switch teams in the feedback!\n")(input)?;
    let (input, _) = tag(player2_name)(input)?;
    let (input, _) = tag(" is now ")(input)?;
    let (input, position) = alt((
        tag("batting").map(|_| ActivePositionType::Lineup),
        tag("pitching").map(|_| ActivePositionType::Rotation),
    ))(input)?;
    let (input, _) = tag(".")(input)?;

    Ok((input, (player1_name, player2_name, position)))
}

fn parse_perk_up(input: &str) -> ParserResult<Vec<&str>> {
    let (input, names) = separated_list1(tag("\n"), parse_terminated(" Perks up."))(input)?;

    Ok((input, names))
}

fn parse_superyummy(input: &str) -> ParserResult<(&str, bool)> {
    let (input, result) = alt((
        parse_terminated(" loves Peanuts.").map(|n| (n, true)),
        parse_terminated(" misses Peanuts.").map(|n| (n, false)),
    ))(input)?;

    Ok((input, result))
}

fn parse_bestow_reverberating(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Reverberations are at dangerous levels!\n")(input)?;
    let (input, player_name) = parse_terminated(" is now Reverberating wildly!")(input)?;

    Ok((input, player_name))
}

enum ParsedReverbType {
    Rotation,
    Lineup,
    Full,
    SeveralPlayers,
}

fn parse_roster_shuffle(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    alt((parse_roster_shuffle_unsafe, parse_roster_shuffle_dangerous))(input)
}

fn parse_roster_shuffle_unsafe(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    let (input, _) = tag("Reverberations are at unsafe levels!\nThe ")(input)?;
    let (input, (team_name, reverb_type)) = alt((
        parse_terminated(" had their rotation shuffled in the Reverb!").map(|n| (n, ParsedReverbType::Rotation)),
        parse_terminated(" had their lineup shuffled in the Reverb!").map(|n| (n, ParsedReverbType::Lineup)),
        parse_terminated(" had several players shuffled in the Reverb!").map(|n| (n, ParsedReverbType::SeveralPlayers)),
    ))(input)?;

    let (input, gravity_players) = many0(preceded(tag("\n"), parse_terminated("'s Gravity kept them in place!")))(input)?;

    Ok((input, (team_name, reverb_type, gravity_players)))
}

fn parse_roster_shuffle_dangerous(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    let (input, _) = tag("Reverberations are at dangerous levels!\nThe ")(input)?;
    let (input, team_name) = parse_terminated(" were shuffled in the Reverb!")(input)?;

    let (input, gravity_players) = many0(preceded(tag("\n"), parse_terminated("'s Gravity kept them in place!")))(input)?;

    Ok((input, (team_name, ParsedReverbType::Full, gravity_players)))
}

fn parse_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, names) = alt((
        parse_double_become_triple_threat,
        parse_single_become_triple_threat,
    ))(input)?;

    Ok((input, names))
}

fn parse_double_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, pitcher1_name) = parse_terminated(" and ")(input)?;
    let (input, pitcher2_name) = parse_terminated(" chug a Third Wave of Coffee!\nThey are now Triple Threats!")(input)?;

    Ok((input, vec![pitcher1_name, pitcher2_name]))
}

fn parse_single_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, pitcher1_name) = parse_terminated(" chugs a Third Wave of Coffee!\nThey are now a Triple Threat!")(input)?;

    Ok((input, vec![pitcher1_name]))
}

fn parse_under_over_over_under(mod_text: &str) -> impl Fn(&str) -> ParserResult<(&str, bool)> + '_ {
    move |input: &str| {
        // complier told me to do the thing with `x` to make the lifetimes work
        let x = alt((
            parse_terminated(&format!(", {mod_text}, On.")).map(|n| (n, true)),
            parse_terminated(&format!(", {mod_text}, Off.")).map(|n| (n, false)),
        ))(input);
        x
    }
}

fn parse_taste_the_infinite(input: &str) -> ParserResult<(&str, &str)> {
    let (input, sheller_name) = parse_terminated(" tastes the infinite!\n")(input)?;
    let (input, shellee_name) = parse_terminated(" is Shelled!")(input)?;

    Ok((input, (sheller_name, shellee_name)))
}

enum ParsedBatterSkippedReason {
    Shelled,
    Elsewhere,
}

fn parse_batter_skipped(input: &str) -> ParserResult<(&str, ParsedBatterSkippedReason)> {
    let (input, result) = alt((
        parse_terminated(" is Shelled and cannot escape!").map(|n| (n, ParsedBatterSkippedReason::Shelled)),
        parse_terminated(" is Elsewhere..").map(|n| (n, ParsedBatterSkippedReason::Elsewhere)),
    ))(input)?;

    Ok((input, result))
}

fn parse_feedback_blocked(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("Reality begins to flicker ...\nBut ")(input)?;
    let (input, player1_name) = parse_terminated(" resists!\n")(input)?;
    let (input, player2_name) = parse_terminated(" is tangled in the flicker!")(input)?;

    Ok((input, (player1_name, player2_name)))
}

fn parse_flag_planted(input: &str) -> ParserResult<(&str, &str, &str, bool)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" break ground on ")(input)?;
    let (input, park_name) = parse_terminated(", selecting to build the ")(input)?;
    let (input, prefab_name) = parse_terminated(" prefab")(input)?;

    let (input, is_first) = alt((
        tag("!\nTHE FLAG IS PLANTED").map(|_| true),
        tag(".\nAnother flag is planted!").map(|_| false),
    ))(input)?;

    Ok((input, (team_nickname, park_name, prefab_name, is_first)))
}

fn parse_team_division_move(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" have joined the ILB!\nThey will play in the ")(input)?;
    let (input, division_name) = parse_terminated(" division.")(input)?;

    Ok((input, (team_nickname, division_name)))
}

fn parse_player_division_move(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" has joined the ILB.")(input)?;

    Ok((input, player_name))
}

fn parse_flooding_swept(input: &str) -> ParserResult<(Vec<&str>, Vec<&str>)> {
    let (input, _) = tag("A surge of Immateria rushes up from Under!\nBaserunners are swept from play!")(input)?;
    let (input, players_swept_elsewhere) = many0(preceded(tag("\n"), parse_terminated(" is swept Elsewhere!")))(input)?;
    let (input, players_flippered_home) = many0(preceded(tag("\n"), parse_terminated(" uses their Flippers to slingshot home!")))(input)?;

    Ok((input, (players_swept_elsewhere, players_flippered_home)))
}

fn parse_return_from_elsewhere(input: &str) -> ParserResult<(&str, Option<i32>)> {
    let (input, player_name) = parse_terminated(" has returned from Elsewhere after ")(input)?;
    let (input, after_days) = alt((
        tag("one season!").map(|_| None),
        parse_whole_number.map(|n| Some(n)),
    ))(input)?;
    let input = if let Some(after_days) = after_days {
        let (input, _) = if after_days == 1 { tag(" day!") } else { tag(" days!") }(input)?;
        input
    } else {
        input
    };

    Ok((input, (player_name, after_days)))
}

fn parse_incineration(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("Rogue Umpire incinerated ")(input)?;
    let (input, victim_name) = parse_terminated("!\nThey're replaced by ")(input)?;
    let (input, replacement_name) = parse_until_period_eof(input)?;

    Ok((input, (victim_name, replacement_name)))
}

fn parse_pitcher_change(input: &str) -> ParserResult<(&str, &str)> {
    let (input, victim_name) = parse_terminated(" is now pitching for the ")(input)?;
    let (input, team_name) = parse_until_period_eof(input)?;

    Ok((input, (victim_name, team_name)))
}

fn parse_party(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" is Partying!")(input)?;

    Ok((input, player_name))
}

fn parse_player_hatched(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" has been hatched from the field of eggs.")(input)?;

    Ok((input, player_name))
}

fn parse_player_added_to_team(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" earn a Postseason Birth!")(input)?;

    Ok((input, team_nickname))
}

fn parse_final_standings(input: &str) -> ParserResult<(&str, i32, &str)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" finished ")(input)?;
    let (input, place) = parse_whole_number(input)?;
    let (input, _) = match place {
        1 => tag("st")(input)?,
        2 => tag("nd")(input)?,
        3 => tag("rd")(input)?,
        _ => tag("th")(input)?,
    };
    let (input, _) = tag(" in the ")(input)?;
    let (input, division_name) = parse_until_period_eof(input)?;

    Ok((input, (team_nickname, place - 1, division_name)))
}

enum ParsedRemovedMod<'s> {
    TeamRemovedFromPartyTimeForPostseason(&'s str),
    TeamUsedFreeWill(&'s str),
    PlayerLostMod((&'s str, &'s str)),
}

fn parse_removed_mod(input: &str) -> ParserResult<ParsedRemovedMod> {
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" have been removed from Party Time to join the Postseason!"))
            .map(|n| ParsedRemovedMod::TeamRemovedFromPartyTimeForPostseason(n)),
        preceded(tag("The "), parse_terminated(" used their Free Will."))
            .map(|n| ParsedRemovedMod::TeamUsedFreeWill(n)),
        pair(parse_terminated(" lost the "), parse_terminated(" mod."))
            .map(|nm| ParsedRemovedMod::PlayerLostMod(nm))
    ))(input)?;

    Ok((input, result))
}

enum ParsedAddedMod<'a> {
    OverUnder(&'a str),
    UnderOver(&'a str),
    EnteredPartyTime(&'a str),
    SinkingShip(&'a str),
    BaseDealing(&'a str),
    MVP(&'a str),
}

fn parse_added_mod(input: &str) -> ParserResult<ParsedAddedMod> {
    let (input, result) = alt((
        preceded(tag("OVER UNDER, "), is_not("\n")).map(|n| ParsedAddedMod::OverUnder(n)),
        preceded(tag("UNDER OVER, "), is_not("\n")).map(|n| ParsedAddedMod::UnderOver(n)),
        preceded(tag("The "), parse_terminated(" have entered Party Time!")).map(|n| ParsedAddedMod::EnteredPartyTime(n)),
        parse_terminated(" GOING UNDER").map(|n| ParsedAddedMod::SinkingShip(n)),
        parse_terminated(" GETTING OVER").map(|n| ParsedAddedMod::BaseDealing(n)),
        parse_terminated(" is named an MVP.").map(|n| ParsedAddedMod::MVP(n)),
    ))(input)?;

    Ok((input, result))
}

fn parse_postseason_advance(input: &str) -> ParserResult<(&str, Option<i32>, i32)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" advanced to ")(input)?;

    let (input, round_num) = alt((
        preceded(tag("Round "), parse_whole_number).map(|n| Some(n)),
        tag("The Internet Series").map(|_| None),
    ))(input)?;
    let (input, _) = tag(" of the Season ")(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.")(input)?;

    Ok((input, (team_nickname, round_num, season_num)))
}

fn parse_earned_postseason_slot(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" earned a spot in the Season ")(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.")(input)?;

    Ok((input, (team_nickname, season_num)))
}

fn parse_postseason_eliminated(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" have been eliminated from the Season ")(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.")(input)?;

    Ok((input, (team_nickname, season_num)))
}

enum ParsedPlayerStatIncrease<'a> {
    PlayerBoosted(&'a str),
    BottomDwellers(&'a str),
}

fn parse_player_stat_increase(input: &str) -> ParserResult<ParsedPlayerStatIncrease> {
    alt((
        parse_terminated(" was boosted.").map(|name| ParsedPlayerStatIncrease::PlayerBoosted(name)),
        parse_bottom_dweller.map(|name| ParsedPlayerStatIncrease::BottomDwellers(name)),
    ))(input)
}

fn parse_bottom_dweller(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_name) = parse_terminated(" are Bottom Dwellers.")(input)?;

    Ok((input, team_name))
}

fn parse_team_won_internet_series(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" won the Season ")(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Internet Series!")(input)?;

    Ok((input, (team_nickname, season_num)))
}

fn parse_will_received(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Will Received: ")(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, blessing_title) = take_till1(|c| c == '\n')(input)?;

    Ok((input, blessing_title))
}

fn parse_blessing_won(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Blessing Won: ")(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, blessing_title) = take_till1(|c| c == '\n')(input)?;

    Ok((input, blessing_title))
}

enum EarlbirdsChange<'a> {
    Added(&'a str),
    Removed, // This one says [object Object]. lol & lmao
}

fn parse_earlbird(input: &str) -> ParserResult<EarlbirdsChange> {
    let (input, _) = tag("Happy Earlseason!\n")(input)?;
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" are Earlbirds!")).map(|n| EarlbirdsChange::Added(n)),
        tag("Earlbirds wears off for the [object Object].").map(|_| EarlbirdsChange::Removed),
    ))(input)?;

    Ok((input, result))
}

enum LateToThePartyChange<'a> {
    Added(&'a str),
    Removed(&'a str), // This one does not say [object Object]
}

fn parse_late_to_the_party(input: &str) -> ParserResult<LateToThePartyChange> {
    let (input, _) = tag("Late to the Party!\n")(input)?;
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" are Late to the Party!")).map(|n| LateToThePartyChange::Added(n)),
        preceded(tag("Late to the Party wears off for the "), parse_terminated(".")).map(|n| LateToThePartyChange::Removed(n)),
    ))(input)?;

    Ok((input, result))
}

fn parse_decree_passed(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Decree Passed: ")(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, decree_title) = take_till1(|c| c == '\n')(input)?;

    Ok((input, decree_title))
}

fn parse_blooddrain(input: &str) -> ParserResult<(&str, &str, AttrCategory)> {
    let (input, _) = tag("The Blooddrain gurgled!\n")(input)?;
    let (input, drinker_name) = parse_terminated(" siphoned some of ")(input)?;
    let (input, drunk_name) = parse_terminated("'s ")(input)?;
    let (input, category) = parse_category(input)?;
    let (input, _) = tag(" ability!\n")(input)?;
    let (input, _) = parse_blooddrain_ability(drinker_name, &category.to_string())(input)?;

    Ok((input, (drinker_name, drunk_name, category)))
}

fn parse_undersea(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_name) = parse_terminated(" go Undersea. They're now Overperforming!")(input)?;

    Ok((input, team_name))
}

fn parse_peanut_mister(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The Peanut Mister activates!\n")(input)?;
    let (input, player_name) = parse_terminated(" has been cured of their peanut allergy!")(input)?;

    Ok((input, player_name))
}

fn parse_birds_unshell(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The Birds circle...\nThe Birds pecked ")(input)?;
    let (input, player_name) = parse_terminated(" free!")(input)?;

    Ok((input, player_name))
}

fn parse_player_replaces_returned(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ")(input)?;
    let (input, team_nickname) = parse_terminated(" cut a player and promoted another from the shadows.")(input)?;

    Ok((input, team_nickname))
}