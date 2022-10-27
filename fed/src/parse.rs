use std::slice::Iter;
use itertools::Itertools;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_till, take_till1, take_until1};
use nom::{AsChar, Finish, IResult, Parser};
use nom::character::complete::{char, digit1};
use nom::combinator::{eof, fail, map_res, opt, verify};
use nom::error::convert_error;
use nom::multi::{many0, separated_list1};
use nom::number::complete::float;
use nom::sequence::{preceded, terminated};
use uuid::Uuid;
use fed_api::{EventType, EventuallyEvent, Weather};
use crate::error::FeedParseError;
use crate::event_schema::{AttrCategory, Being, BlooddrainAction, CoffeeBeanMod, FedEvent, FedEventData, FeedbackPlayerData, FreeRefill, GameEvent, Inhabiting, ModChangeSubEvent, ModChangeSubEventWithPlayer, ModDuration, PerkPlayers, PlayerStatChange, PositionType, ScoreInfo, ScoringPlayer, SpicyStatus, StoppedInhabiting, SubEvent};
use crate::feed_event_util::{get_one_player_id, get_one_team_id, get_one_sub_event, get_str_metadata, get_float_metadata, get_str_vec_metadata, get_int_metadata, get_two_player_ids, get_one_sub_event_from_slice, get_two_sub_events, get_uuid_metadata};

pub fn parse_feed_event(feed_event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    if feed_event.metadata.siblings.is_empty() {
        parse_single_feed_event(feed_event)
    } else {
        todo!()
    }
}

fn parse_single_feed_event(event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
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
                game: GameEvent::try_from_event(event)?,
                weather: Weather::try_from(weather as i32)
                    .map_err(|_| FeedParseError::UnknownWeather(weather))?,
            })
        }
        EventType::PlayBall => {
            parse_fixed_description(event, "Play ball!", FedEventData::PlayBall {
                game: GameEvent::try_from_event(event)?,
            })
        }
        EventType::HalfInning => {
            let (top_of_inning, inning, team_name) = run_parser(&event, parse_half_inning)?;

            assert!(is_known_team_name(team_name));

            Ok(make_fed_event(event, FedEventData::HalfInningStart {
                game: GameEvent::try_from_event(event)?,
                top_of_inning,
                inning,
                batting_team_name: team_name.to_string(),
            }))
        }
        EventType::PitcherChange => { todo!() }
        EventType::StolenBase => {
            let (runner_name, base_stolen, is_successful, blaserunning, free_refiller) = run_parser(&event, parse_stolen_base)?;
            if is_successful {
                let runner_id = get_one_player_id_advanced(event, blaserunning)?;

                Ok(make_fed_event(event, FedEventData::StolenBase {
                    game: GameEvent::try_from_event(event)?,
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
                        })
                    }).transpose()?,
                }))
            } else {
                Ok(make_fed_event(event, FedEventData::CaughtStealing {
                    game: GameEvent::try_from_event(event)?,
                    runner_name: runner_name.to_string(),
                    base_stolen,
                }))
            }
        }
        EventType::Walk => {
            let parsed_walk = run_parser(&event, parse_walk)?;
            match parsed_walk {
                ParsedWalk::Ordinary((batter_name, scores, base_instincts)) => {
                    let (&batter_id, scorer_ids) = event.player_tags.split_first()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 1 + scores.scorers.len(),
                            actual_num: event.player_tags.len(),
                        })?;

                    let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, &event.metadata.children, event.r#type, 0)?;

                    // I don't think a walk stops inhabiting because you end up on base
                    assert!(stopped_inhabiting.is_none());

                    Ok(make_fed_event(event, FedEventData::Walk {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        scores,
                        base_instincts,
                    }))
                }
                ParsedWalk::Charm((batter_name, pitcher_name)) => {
                    let (batter_id, charmer_id) = get_two_player_ids(event)?;
                    assert_eq!(batter_id, charmer_id);
                    Ok(make_fed_event(event, FedEventData::CharmWalk {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        batter_id,
                        pitcher_name: pitcher_name.to_string(),
                    }))
                }
            }
        }
        EventType::Strikeout => {
            match run_parser(&event, parse_strikeout)? {
                ParsedStrikeout::Swinging(batter_name) => {
                    let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &event.player_tags, &event.metadata.children, event.r#type, 0)?;
                    Ok(make_fed_event(event, FedEventData::StrikeoutSwinging {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                    }))
                }
                ParsedStrikeout::Looking(batter_name) => {
                    let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &event.player_tags, &event.metadata.children, event.r#type, 0)?;
                    Ok(make_fed_event(event, FedEventData::StrikeoutLooking {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        stopped_inhabiting,
                        is_special: event.category == 2,
                    }))
                }
                ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings } => {
                    if let Some((&charmer_id, &charmer_id_2, &charmed_id)) = event.player_tags.iter().collect_tuple() {
                        assert_eq!(charmer_id, charmer_id_2);
                        Ok(make_fed_event(event, FedEventData::CharmStrikeout {
                            game: GameEvent::try_from_event(event)?,
                            charmer_id,
                            charmer_name: charmer_name.to_string(),
                            charmed_id,
                            charmed_name: charmed_name.to_string(),
                            num_swings,
                        }))
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
            let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, cooled_off, &event.player_tags)?;
            let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
            Ok(make_fed_event(event, FedEventData::Flyout {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                fielder_name: fielder_name.to_string(),
                scores,
                stopped_inhabiting,
                cooled_off,
            }))
        }
        EventType::GroundOut => {
            let (parsed_out, scores, cooled_off) = run_parser(&event, parse_ground_out)?;
            let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, cooled_off, &event.player_tags)?;
            let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
            match parsed_out {
                ParsedGroundOut::Simple { batter_name, fielder_name } => {
                    Ok(make_fed_event(event, FedEventData::GroundOut {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        fielder_name: fielder_name.to_string(),
                        scores,
                        stopped_inhabiting,
                        cooled_off,
                        is_special: event.category == 2,
                    }))
                }
                ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base } => {
                    Ok(make_fed_event(event, FedEventData::FieldersChoice {
                        game: GameEvent::try_from_event(event)?,
                        runner_out_name: runner_out_name.to_string(),
                        batter_name: batter_name.to_string(),
                        out_at_base: base,
                        scores,
                        stopped_inhabiting,
                    }))
                }
                ParsedGroundOut::DoublePlay { batter_name } => {
                    Ok(make_fed_event(event, FedEventData::DoublePlay {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        scores,
                        stopped_inhabiting,
                    }))
                }
            }
        }
        EventType::HomeRun => {
            let (is_magmatic, batter_name, num_runs, free_refillers, spicy_status) = run_parser(&event, parse_hr)?;
            let (remaining_children, spicy_status) = extract_spicy_event(event, spicy_status)?;
            let (remaining_children, magmatic_event) = if is_magmatic {
                let expected_num_children = event.metadata.children.len() - remaining_children.len() + 1;
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
                        })))
                    })
                    .unwrap_or(Ok((remaining_children, None)))?
            } else {
                let expected_num_children = event.metadata.children.len() - remaining_children.len() + 1;
                Err(FeedParseError::MissingChild {
                    event_type: event.r#type,
                    expected_num_children: expected_num_children as i32,
                })?
            };

            let batter_id = get_one_player_id_advanced(event, !spicy_status.is_none())?;
            Ok(make_fed_event(event, FedEventData::HomeRun {
                game: GameEvent::try_from_event(event)?,
                magmatic: magmatic_event.map(|e| {
                    Ok((SubEvent::from_event(e), get_one_team_id(e)?))
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
            }))
        }
        EventType::Hit => {
            let (batter_name, num_bases, scores, spicy_status) = run_parser(&event, parse_hit)?;
            if let Some((&batter_id, scorer_ids)) = event.player_tags.split_first() {
                let scorer_ids = if spicy_status != ParsedSpicyStatus::None {
                    scorer_ids.split_last()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: scores.scorers.len() + 2, // i think
                            actual_num: scorer_ids.len(),
                        })?
                        .1
                } else {
                    scorer_ids
                };

                let (score_children, spicy_status) = extract_spicy_event(event, spicy_status)?;
                let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, score_children, event.r#type, 1)?;

                Ok(make_fed_event(event, FedEventData::Hit {
                    game: GameEvent::try_from_event(event)?,
                    batter_name: batter_name.to_string(),
                    batter_id,
                    num_bases,
                    scores,
                    stopped_inhabiting,
                    spicy_status,
                }))
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
            Ok(make_fed_event(event, FedEventData::GameEnd {
                game: GameEvent::try_from_event_extra_teams(event)?,
                winner_id,
                winning_team_name: winning_team_name.to_string(),
                winning_team_score,
                losing_team_name: losing_team_name.to_string(),
                losing_team_score,
            }))
        }
        EventType::BatterUp => {
            let (batter_name, inhabited, team_name, wielding_item, is_repeating) = run_parser(&event, parse_batter_up)?;

            // I missed `team_name: "Millennials, wielding An Actual Airplane"` once and I don't
            // want something like that to happen again
            assert!(is_known_team_nickname(team_name));

            Ok(make_fed_event(event, FedEventData::BatterUp {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                team_name: team_name.to_string(),
                wielding_item: wielding_item.map(|s| s.to_string()),
                inhabiting: inhabited.map(|inhabited| {
                    let (child, ) = event.metadata.children.iter().collect_tuple()
                        .ok_or_else(|| FeedParseError::MissingChild {
                            event_type: event.r#type,
                            expected_num_children: 1,
                        })?;

                    // These live on the parent
                    let (inhabiting_player_id, inhabited_player_id) = get_two_player_ids(event)?;

                    Ok(Inhabiting {
                        sub_event: SubEvent::from_event(child),
                        inhabited_player_name: inhabited.to_string(),
                        inhabiting_player_id,
                        inhabited_player_id,
                    })
                }).transpose()?,
                is_repeating,
            }))
        }
        EventType::Strike => {
            let (strike_type, balls, strikes) = run_parser(&event, parse_strike)?;
            let game = GameEvent::try_from_event(event)?;
            Ok(make_fed_event(event, match strike_type {
                StrikeType::Swinging => FedEventData::StrikeSwinging { game, balls, strikes },
                StrikeType::Looking => FedEventData::StrikeLooking { game, balls, strikes },
                StrikeType::Flinching => FedEventData::StrikeFlinching { game, balls, strikes },
            }))
        }
        EventType::Ball => {
            let (balls, strikes) = run_parser(&event, parse_ball)?;
            Ok(make_fed_event(event, FedEventData::Ball {
                game: GameEvent::try_from_event(event)?,
                balls,
                strikes,
            }))
        }
        EventType::FoulBall => {
            // Eventually this will need very foul support, but I'll get to that when it comes up
            let (balls, strikes) = run_parser(&event, parse_foul_ball)?;
            Ok(make_fed_event(event, FedEventData::FoulBall {
                game: GameEvent::try_from_event(event)?,
                balls,
                strikes,
            }))
        }
        EventType::ShamingRun => { todo!() }
        EventType::HomeFieldAdvantage => { todo!() }
        EventType::HitByPitch => { todo!() }
        EventType::BatterSkipped => { todo!() }
        EventType::Party => { todo!() }
        EventType::StrikeZapped => {
            parse_fixed_description(event, "The Electricity zaps a strike away!",
                                    FedEventData::StrikeZapped {
                                        game: GameEvent::try_from_event(event)?,
                                    })
        }
        EventType::WeatherChange => { todo!() }
        EventType::MildPitch => {
            let (pitcher_name, pitch_type, runners_advance) = run_parser(&event, parse_mild_pitch)?;
            match pitch_type {
                MildPitchType::Ball((balls, strikes)) => {
                    Ok(make_fed_event(event, FedEventData::MildPitch {
                        game: GameEvent::try_from_event(event)?,
                        pitcher_id: get_one_player_id(event)?,
                        pitcher_name: pitcher_name.to_string(),
                        balls,
                        strikes,
                        runners_advance,
                    }))
                }
                MildPitchType::Walk(batter_name) => {
                    let (pitcher_id, batter_id) = get_two_player_ids(event)?;
                    // I don't believe this should be possible
                    assert!(!runners_advance, "Runners \"advanced on the pathetic play\" on a mild pitch that was also a walk");
                    Ok(make_fed_event(event, FedEventData::MildPitchWalk {
                        game: GameEvent::try_from_event(event)?,
                        pitcher_id,
                        pitcher_name: pitcher_name.to_string(),
                        batter_id,
                        batter_name: batter_name.to_string(),
                    }))
                }
            }
        }
        EventType::InningEnd => {
            let inning_num = run_parser(&event, parse_inning_end)?;
            Ok(make_fed_event(event, FedEventData::InningEnd {
                game: GameEvent::try_from_event(event)?,
                inning_num,
            }))
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

            Ok(make_fed_event(event, FedEventData::BeingSpeech {
                being: Being::try_from(being_id as i32)
                    .map_err(|_| FeedParseError::UnknownBeing(being_id))?,
                message: event.description.clone(),
            }))
        }
        EventType::BlackHole => {
            let (scoring_team, victim_team) = run_parser(&event, parse_black_hole)?;
            assert!(is_known_team_nickname(scoring_team));
            assert!(is_known_team_nickname(victim_team));
            Ok(make_fed_event(event, FedEventData::BlackHole {
                game: GameEvent::try_from_event(event)?,
                scoring_team_nickname: scoring_team.to_string(),
                victim_team_nickname: victim_team.to_string(),
            }))
        }
        EventType::Sun2 => {
            let scoring_team= run_parser(&event, parse_sun2)?;
            assert!(is_known_team_nickname(scoring_team));
            Ok(make_fed_event(event, FedEventData::Sun2 {
                game: GameEvent::try_from_event(event)?,
                team_nickname: scoring_team.to_string(),
            }))

        }
        EventType::BirdsCircle => {
            parse_fixed_description(event, "The Birds circle ... but they don't find what they're looking for.", FedEventData::BirdsCircle {
                game: GameEvent::try_from_event(event)?,
            })
        }
        EventType::FriendOfCrows => {
            let (pitcher_name, batter_name) = run_parser(&event, parse_friend_of_crows)?;
            let (pitcher_uuid, batter_uuid) = event.player_tags.iter().cloned().collect_tuple()
                .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                    event_type: event.r#type,
                    tag_type: "player",
                    expected_num: 2,
                    actual_num: event.player_tags.len(),
                })?;

            Ok(make_fed_event(event, FedEventData::FriendOfCrows {
                game: GameEvent::try_from_event(event)?,
                pitcher_id: pitcher_uuid,
                pitcher_name: pitcher_name.to_string(),
                batter_id: batter_uuid,
                batter_name: batter_name.to_string(),
            }))
        }
        EventType::BirdsUnshell => { todo!() }
        EventType::BecomeTripleThreat => { todo!() }
        EventType::GainFreeRefill => {
            let (player_name, roast, ingredient1, ingredient2) = run_parser(&event, parse_gain_free_refill)?;
            let sub_event = get_one_sub_event(event)?;
            let player_id = get_one_player_id(event)?;
            // The player ID should match in the sub event
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            Ok(make_fed_event(event, FedEventData::GainFreeRefill {
                game: GameEvent::try_from_event(event)?,
                player_id,
                player_name: player_name.to_string(),
                roast: roast.to_string(),
                ingredient1: ingredient1.to_string(),
                ingredient2: ingredient2.to_string(),
                sub_event: SubEvent::from_event(sub_event),
                team_id: get_one_team_id(sub_event)?,
            }))
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
            Ok(make_fed_event(event, FedEventData::CoffeeBean {
                game: GameEvent::try_from_event(event)?,
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
            }))
        }
        EventType::FeedbackBlocked => { todo!() }
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

            Ok(make_fed_event(event, FedEventData::Feedback {
                game: GameEvent::try_from_event(event)?,
                players: (
                    get_player_data!(sub_event, "a", player1_name),
                    get_player_data!(sub_event, "b", player2_name),
                ),
                position_type: position,
                sub_event: SubEvent::from_event(sub_event),
            }))
        }
        EventType::SuperallergicReaction => { todo!() }
        EventType::AllergicReaction => {
            let player_name = run_parser(&event, parse_allergic_reaction)?;
            let player_id = get_one_player_id(event)?;
            let sub_event = get_one_sub_event(event)?;
            assert_eq!(player_id, get_one_player_id(sub_event)?);
            Ok(make_fed_event(event, FedEventData::AllergicReaction {
                game: GameEvent::try_from_event(event)?,
                team_id: get_one_team_id(sub_event)?,
                player_id,
                player_name: player_name.to_string(),
                sub_event: SubEvent::from_event(sub_event),
                rating_before: get_float_metadata(sub_event, "before")?,
                rating_after: get_float_metadata(sub_event, "after")?,
            }))
        }
        EventType::ReverbBestowsReverberating => { todo!() }
        EventType::ReverbRosterShuffle => { todo!() }
        EventType::Blooddrain => { todo!() }
        EventType::BlooddrainSiphon => {
            let (sipper_name, sipped_name, sipped_category, action) = run_parser(&event, parse_blooddrain_siphon)?;
            let (sipper_id, sipped_id) = get_two_player_ids(event)?;

            match action {
                None => {
                    let (sipper_event, sipped_event) = get_two_sub_events(event)?;

                    Ok(make_fed_event(event, FedEventData::Blooddrain {
                        game: GameEvent::try_from_event(event)?,
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
                    }))
                }
                Some(action) => {
                    let stat_decrease_event = get_one_sub_event(event)?;
                    Ok(make_fed_event(event, FedEventData::SpecialBlooddrain {
                        game: GameEvent::try_from_event(event)?,
                        sipper_id,
                        sipped_team_id: get_one_team_id(stat_decrease_event)?,
                        sipper_name: sipper_name.to_string(),
                        sipped_id,
                        sipped_name: sipped_name.to_string(),
                        sipped_category,
                        action,
                        sipped_event: SubEvent::from_event(stat_decrease_event),
                        rating_before: get_float_metadata(stat_decrease_event, "before")?,
                        rating_after: get_float_metadata(stat_decrease_event, "after")?,
                    }))
                }
            }
        }
        EventType::BlooddrainBlocked => { todo!() }
        EventType::Incineration => { todo!() }
        EventType::IncinerationBlocked => {
            // For now I only support magmatic, that may have to change
            let player_name = run_parser(&event, parse_incineration_blocked)?;
            let sub_event = get_one_sub_event(event)?;
            Ok(make_fed_event(event, FedEventData::BecameMagmatic {
                game: GameEvent::try_from_event(event)?,
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
                team_id: get_one_team_id(sub_event)?,
                mod_add_event: SubEvent::from_event(sub_event),
            }))
        }
        EventType::FlagPlanted => { todo!() }
        EventType::RenovationBuilt => { todo!() }
        EventType::LightSwitchToggled => { todo!() }
        EventType::DecreePassed => { todo!() }
        EventType::BlessingOrGiftWon => { todo!() }
        EventType::WillRecieved => { todo!() }
        EventType::FloodingSwept => { todo!() }
        EventType::SalmonSwim => { todo!() }
        EventType::PolarityShift => { todo!() }
        EventType::EnterSecretBase => { todo!() }
        EventType::ExitSecretBase => { todo!() }
        EventType::ConsumersAttack => { todo!() }
        EventType::EchoChamber => { todo!() }
        EventType::GrindRail => { todo!() }
        EventType::TunnelsUsed => { todo!() }
        EventType::PeanutMister => { todo!() }
        EventType::PeanutFlavorText => {
            Ok(make_fed_event(event, FedEventData::PeanutFlavorText {
                game: GameEvent::try_from_event(event)?,
                message: event.description.clone(),
            }))
        }
        EventType::TasteTheInfinite => { todo!() }
        EventType::EventHorizonActivation => { todo!() }
        EventType::EventHorizonAwaits => { todo!() }
        EventType::SolarPanelsAwait => { todo!() }
        EventType::SolarPanelsActivation => { todo!() }
        EventType::TarotReading => { todo!() }
        EventType::EmergencyAlert => { todo!() }
        EventType::ReturnFromElsewhere => { todo!() }
        EventType::OverUnder => { todo!() }
        EventType::UnderOver => { todo!() }
        EventType::Undersea => { todo!() }
        EventType::Homebody => { todo!() }
        EventType::Superyummy => {
            let (mod_add_event, ): (&EventuallyEvent, ) = event.metadata.children.iter()
                .collect_tuple()
                .ok_or_else(|| FeedParseError::MissingChild {
                    event_type: event.r#type,
                    expected_num_children: 1,
                })?;

            let mod_name = get_str_metadata(event, "mod")?;

            let which_performing = if mod_name == "OVERPERFORMING" {
                true
            } else if mod_name == "UNDERPERFORMING" {
                false
            } else {
                return Err(FeedParseError::UnexpectedModName {
                    event_type: event.r#type,
                    mod_name: mod_name.to_string(),
                });
            };

            let player_name = if which_performing {
                run_parser(event, parse_terminated(" loves Peanuts."))?
            } else {
                run_parser(event, parse_terminated(" misses Peanuts."))?
            };

            Ok(make_fed_event(event, FedEventData::SuperyummyGameStart {
                game: GameEvent::try_from_event(event)?,
                player_name: player_name.to_string(),
                peanuts: which_performing,
                is_first_proc: mod_add_event.r#type == EventType::AddedModFromOtherMod,
                sub_event: SubEvent::from_event(mod_add_event),
                player_id: get_one_player_id(mod_add_event)?,
                team_id: get_one_team_id(mod_add_event)?,
            }))
        }
        EventType::Perk => {
            let player_names = run_parser(event, parse_perk_up)?;

            Ok(make_fed_event(event, FedEventData::PerkUp {
                game: GameEvent::try_from_event(event)?,
                players: player_names.into_iter().zip_eq(&event.metadata.children)
                    .map(|(player_name, mod_add_event)| {
                        assert_eq!(format!("{player_name} Perks up."), mod_add_event.description);
                        Ok(PerkPlayers {
                            player_name: player_name.to_string(),
                            sub_event: SubEvent::from_event(mod_add_event),
                            player_id: get_one_player_id(mod_add_event)?,
                            team_id: get_one_team_id(mod_add_event)?,
                        })
                    })
                    .collect::<Result<_, _>>()?
            }))
        }
        EventType::Earlbird => { todo!() }
        EventType::LateToTheParty => { todo!() }
        EventType::ShameDonor => { todo!() }
        EventType::AddedMod => { todo!() }
        EventType::RemovedMod => { todo!() }
        EventType::ModExpires => {
            let (player_name, mod_duration) = run_parser(&event, parse_mod_expires)?;
            let mods = get_str_vec_metadata(event, "mods")?;
            Ok(make_fed_event(event, FedEventData::ModExpires {
                team_id: get_one_team_id(event)?,
                player_id: get_one_player_id(event)?,
                player_name: player_name.to_string(),
                mods: mods.into_iter().map(String::from).collect(),
                mod_duration,
            }))
        }
        EventType::PlayerAddedToTeam => { todo!() }
        EventType::PlayerReplacedByNecromancy => { todo!() }
        EventType::PlayerReplacesReturned => { todo!() }
        EventType::PlayerRemovedFromTeam => { todo!() }
        EventType::PlayerTraded => { todo!() }
        EventType::PlayerSwap => { todo!() }
        EventType::PlayerBornFromIncineration => { todo!() }
        EventType::PlayerStatIncrease => { todo!() }
        EventType::PlayerStatDecrease => { todo!() }
        EventType::PlayerStatReroll => { todo!() }
        EventType::PlayerStatDecreaseFromSuperallergic => { todo!() }
        EventType::PlayerMoveFailedForce => { todo!() }
        EventType::EnterHallOfFlame => { todo!() }
        EventType::ExitHallOfFlame => { todo!() }
        EventType::PlayerGainedItem => { todo!() }
        EventType::PlayerLostItem => { todo!() }
        EventType::ReverbFullShuffle => { todo!() }
        EventType::ReverbLineupShuffle => { todo!() }
        EventType::ReverbRotationShuffle => { todo!() }
        EventType::ModChange => { todo!() }
        EventType::AddedModFromOtherMod => { todo!() }
        EventType::ChangedModFromOtherMod => { todo!() }
        EventType::TeamWasShamed => {
            let (shaming_team, shamed_team) = run_parser(&event, parse_team_was_shamed)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            Ok(make_fed_event(event, FedEventData::TeamWasShamed {
                shamed_team_id: get_one_team_id(event)?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: get_int_metadata(event, "totalShames")?,
                total_shamings: get_int_metadata(event, "totalShamings")?,
            }))
        }
        EventType::TeamDidShame => {
            let (shaming_team, shamed_team) = run_parser(&event, parse_team_did_shame)?;
            assert!(is_known_team_nickname(shaming_team));
            assert!(is_known_team_nickname(shamed_team));

            Ok(make_fed_event(event, FedEventData::TeamDidShame {
                shaming_team_id: get_one_team_id(event)?,
                shaming_team_nickname: shaming_team.to_string(),
                shamed_team_nickname: shamed_team.to_string(),
                total_shames: get_int_metadata(event, "totalShames")?,
                total_shamings: get_int_metadata(event, "totalShamings")?,
            }))
        }
        EventType::RunsScored => { todo!() }
        EventType::WinCollectedRegular => { todo!() }
        EventType::WinCollectedPostseason => { todo!() }
        EventType::GameOver => { todo!() }
        EventType::StormWarning => { todo!() }
        EventType::Snowflakes => { todo!() }
        EventType::Sun2SetWin => {
            let team_name = run_parser(&event, parse_sun2_set_win)?;
            assert!(is_known_team_nickname(team_name));
            Ok(make_fed_event(event, FedEventData::Sun2SetWin {
                team_id: get_one_team_id(event)?,
                team_nickname: team_name.to_string(),
            }))
        }
        EventType::BlackHoleSwallowedWin => {
            let team_name = run_parser(&event, parse_black_hole_swallowed_win)?;
            assert!(is_known_team_nickname(team_name));
            Ok(make_fed_event(event, FedEventData::BlackHoleSwallowedWin {
                team_id: get_one_team_id(event)?,
                team_nickname: team_name.to_string(),
            }))
        }
    }
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

fn extract_spicy_event(event: &EventuallyEvent, spicy_status: ParsedSpicyStatus) -> Result<(&[EventuallyEvent], SpicyStatus), FeedParseError> {
    Ok(match spicy_status {
        ParsedSpicyStatus::None => { (event.metadata.children.as_slice(), SpicyStatus::None) }
        ParsedSpicyStatus::HeatingUp => { (event.metadata.children.as_slice(), SpicyStatus::HeatingUp) }
        ParsedSpicyStatus::RedHot => {
            // TODO Is the spicy event always the last? first? neither?
            if let Some((spicy_event, children)) = event.metadata.children.split_last() {
                // TODO Make this assert into a propagated error
                assert_eq!(spicy_event.r#type, EventType::AddedMod);
                (children, SpicyStatus::RedHot(ModChangeSubEvent {
                    sub_event: SubEvent::from_event(spicy_event),
                    team_id: get_one_team_id(spicy_event)?,
                }))
            } else {
                Err(FeedParseError::MissingChild {
                    event_type: event.r#type,
                    expected_num_children: 1,  // at least one
                })?
            }
        }
    })
}

fn extract_cooled_off_event<'e, 't>(event: &'e EventuallyEvent, cooled_off: bool, player_tags: &'t [Uuid]) -> Result<(&'e [EventuallyEvent], Option<ModChangeSubEventWithPlayer>, &'t [Uuid]), FeedParseError> {
    Ok(match cooled_off {
        false => { (event.metadata.children.as_slice(), None, player_tags) }
        true => {
            // TODO Is the spicy event always the last? first? neither?
            if let Some((cooled_off_event, children)) = event.metadata.children.split_last() {
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
        .ok_or_else(|| FeedParseError::MissingChild {
            event_type,
            expected_num_children: -1, // Unknown at this point in the computation
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
    })
}

fn is_known_team_name(name: &str) -> bool {
    vec!["Hawai'i Fridays", "Canada Moist Talkers", "San Francisco Lovers", "Seattle Garages",
         "Breckenridge Jazz Hands", "Hellmouth Sunbeams", "Hades Tigers", "Mexico City Wild Wings",
         "Boston Flowers", "New York Millennials", "Philly Pies", "Miami Dale", "Tokyo Lift",
         "Chicago Firefighters", "Dallas Steaks", "Yellowstone Magic", "Kansas City Breath Mints",
         "Houston Spies", "Charleston Shoe Thieves", "LA Unlimited Tacos",
    ].contains(&name)
}

fn is_known_team_nickname(name: &str) -> bool {
    vec!["Fridays", "Moist Talkers", "Lovers", "Jazz Hands", "Sunbeams", "Tigers", "Wild Wings",
         "Flowers", "Millennials", "Pies", "Garages", "Dale", "Lift", "Firefighters", "Steaks",
         "Magic", "Breath Mints", "Spies", "Shoe Thieves", "Tacos",
    ].contains(&name)
}

type ParserResult<'a, Out> = IResult<&'a str, Out, nom::error::VerboseError<&'a str>>;

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
        Ok(make_fed_event(event, data))
    } else {
        Err(FeedParseError::UnexpectedDescription {
            event_type: event.r#type,
            description: event.description.clone(),
            expected: expected_description.to_string(),
        })
    }
}

fn make_fed_event(feed_event: &EventuallyEvent, data: FedEventData) -> FedEvent {
    FedEvent {
        id: feed_event.id,
        created: feed_event.created,
        sim: feed_event.sim.clone(),
        tournament: feed_event.tournament,
        season: feed_event.season,
        day: feed_event.day,
        phase: feed_event.phase,
        nuts: feed_event.nuts,
        data,
    }
}

fn parse_terminated(tag_content: &'static str) -> impl Fn(&str) -> ParserResult<&str> {
    move |input| {
        let (input, parsed_value) = verify(take_until1(tag_content), |s: &str| !s.contains('\n'))(input)?;
        let (input, _) = tag(tag_content)(input)?;

        Ok((input, parsed_value))
    }
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

    let (input, scores) = parse_scores(" scores!")(input)?;
    let (input, _) = tag("\n")(input)?;

    let (input, batter_name) = parse_terminated(" reaches on fielder's choice.")(input)?;

    let (input, cooled_off) = parse_cooled_off(batter_name)(input)?;

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
    Charm((&'s str, &'s str)),
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

    let (input, scores) = parse_scores(" scores!")(input)?;

    let (input, base_instincts) = opt(parse_base_instincts)(input)?;

    Ok((input, (batter_name, scores, base_instincts)))
}

fn parse_charm_walk(input: &str) -> ParserResult<(&str, &str)> {
    // This will need to be updated if anyone charms in a run
    let (input, batter_name) = parse_terminated(" charms ")(input)?;
    let (input, pitcher_name) = parse_terminated("!\n")(input)?;
    let (input, _) = tag(batter_name)(input)?;
    let (input, _) = tag(" walks to first base.")(input)?;

    Ok((input, (batter_name, pitcher_name)))
}

fn parse_inning_end(input: &str) -> ParserResult<i32> {
    let (input, _) = tag("Inning ")(input)?;
    let (input, inning_num) = parse_whole_number(input)?;
    let (input, _) = tag(" is now an Outing.")(input)?;

    Ok((input, inning_num))
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

    // Just checking that my assumption is correct. It's <= because of 20.3
    assert!(losing_team_score <= winning_team_score);

    // The parsers for *_team_name should always leave us with a space at the end
    Ok((input, ((winning_team_name.strip_suffix(" ").unwrap(), winning_team_score),
                (losing_team_name.strip_suffix(" ").unwrap(), losing_team_score))))
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

fn parse_mild_pitch(input: &str) -> ParserResult<(&str, MildPitchType, bool)> {
    let (input, pitcher_name) = parse_terminated(" throws a Mild pitch!\n")(input)?;

    // TODO: scoring

    // Fun fact: Can't reuse the ball parser because it looks for a comma but this has a period
    let (input, pitch_type) = alt((
        parse_mild_pitch_ball,
        parse_terminated(" draws a walk.").map(|name| MildPitchType::Walk(name))
    ))(input)?;

    let (input, runners_advance) = opt(tag("\nRunners advance on the pathetic play!"))(input)?;

    Ok((input, (pitcher_name, pitch_type, runners_advance.is_some())))
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

fn parse_incineration_blocked(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Rogue Umpire tried to incinerate ")(input)?;
    let (input, player_name) = parse_terminated(", but ")(input)?;
    let (input, _) = tag(player_name)(input)?;
    let (input, _) = tag(" ate the flame! They became Magmatic!")(input)?;
    Ok((input, player_name))
}

fn parse_mod_expires(input: &str) -> ParserResult<(&str, ModDuration)> {
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

fn parse_blooddrain_action(drinker_name: &str) -> impl Fn(&str) -> ParserResult<BlooddrainAction> + '_ {
    move |input: &str| {
        let (input, _) = tag(drinker_name)(input)?;
        let (input, action) = alt((
            // preceded(tag(" increased their "), terminated(parse_category, tag(" ability!"))).map(|ability| BlooddrainAction::IncreaseAbility(ability)),
            tag(" adds a Ball!").map(|_| BlooddrainAction::AddBall),
            tag(" removes a Ball!").map(|_| BlooddrainAction::RemoveBall),
            tag(" adds a Strike!").map(|_| BlooddrainAction::AddStrike),
            tag(" removes a Strike!").map(|_| BlooddrainAction::RemoveStrike),
            tag(" adds a Out!").map(|_| BlooddrainAction::AddOut),
            tag(" removes a Out!").map(|_| BlooddrainAction::RemoveOut),
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

fn parse_blooddrain_siphon(input: &str) -> ParserResult<(&str, &str, AttrCategory, Option<BlooddrainAction>)> {
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
        tag("batting").map(|_| AttrCategory::Batting),
        tag("baserunning").map(|_| AttrCategory::Baserunning),
        tag("pitching").map(|_| AttrCategory::Pitching),
        tag("defensive").map(|_| AttrCategory::Defense),
    ))(input)
}

fn parse_friend_of_crows(input: &str) -> ParserResult<(&str, &str)> {
    let (input, pitcher_name) = parse_terminated(" calls upon their Friends!\nA murder of Crows ambush ")(input)?;
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

fn parse_feedback(input: &str) -> ParserResult<(&str, &str, PositionType)> {
    let (input, _) = tag("Reality flickers. Things look different ...\n")(input)?;
    let (input, player1_name) = parse_terminated(" and ")(input)?;
    let (input, player2_name) = parse_terminated(" switch teams in the feedback!\n")(input)?;
    let (input, _) = tag(player2_name)(input)?;
    let (input, _) = tag(" is now ")(input)?;
    let (input, position) = alt((
        tag("batting").map(|_| PositionType::Batter),
        tag("pitching").map(|_| PositionType::Pitcher),
    ))(input)?;
    let (input, _) = tag(".")(input)?;

    Ok((input, (player1_name, player2_name, position)))
}

fn parse_perk_up(input: &str) -> ParserResult<Vec<&str>> {
    let (input, names) = separated_list1(tag("\n"), parse_terminated(" Perks up."))(input)?;

    Ok((input, names))
}

