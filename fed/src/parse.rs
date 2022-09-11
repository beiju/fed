use std::slice::Iter;
use itertools::Itertools;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_till1, take_until1};
use nom::{Finish, IResult, Parser};
use nom::character::complete::digit1;
use nom::combinator::{eof, fail, opt};
use nom::error::convert_error;
use nom::multi::{many0, many1, separated_list0};
use nom::sequence::{preceded, terminated};
use uuid::Uuid;
use fed_api::{EventuallyEvent, EventType, Weather};
use crate::error::FeedParseError;
use crate::event_schema::{Being, FedEvent, FedEventData, FreeRefill, GameEvent, Inhabiting, Score, SubEvent};

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
                let runner_id = if blaserunning {
                    let (&id1, &id2) = event.player_tags.iter().collect_tuple()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 2,
                            actual_num: event.player_tags.len(),
                        })?;
                    assert_eq!(id1, id2);
                    id1
                } else {
                    get_one_player_id(event)?
                };

                Ok(make_fed_event(event, FedEventData::StolenBase {
                    game: GameEvent::try_from_event(event)?,
                    runner_name: runner_name.to_string(),
                    runner_id,
                    base_stolen,
                    blaserunning,
                    free_refill: free_refiller.map(|refiller_name| {
                        let (sub_event, ) = event.metadata.children.iter().collect_tuple()
                            .ok_or_else(|| FeedParseError::MissingChild {
                                event_type: event.r#type,
                                expected_num_children: 1,
                            })?;

                        let (&team_id, ) = sub_event.team_tags.iter().collect_tuple()
                            .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                                event_type: sub_event.r#type,
                                tag_type: "team",
                                expected_num: 1,
                                actual_num: sub_event.team_tags.len(),
                            })?;

                        let (&player_id, ) = sub_event.player_tags.iter().collect_tuple()
                            .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                                event_type: sub_event.r#type,
                                tag_type: "player",
                                expected_num: 1,
                                actual_num: sub_event.player_tags.len(),
                            })?;

                        Ok(FreeRefill {
                            sub_event: SubEvent::from_event(sub_event),
                            player_name: refiller_name.to_string(),
                            player_id,
                            team_id,
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
            let batter_name = run_parser(&event, parse_walk)?;
            Ok(make_fed_event(event, FedEventData::Walk {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                batter_id: get_one_player_id(event)?,
            }))
        }
        EventType::Strikeout => {
            match run_parser(&event, parse_strikeout)? {
                ParsedStrikeout::Swinging(batter_name) => {
                    Ok(make_fed_event(event, FedEventData::StrikeoutSwinging {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                    }))
                }
                ParsedStrikeout::Looking(batter_name) => {
                    Ok(make_fed_event(event, FedEventData::StrikeoutLooking {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
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
            let (batter_name, fielder_name, scores) = run_parser(&event, parse_flyout)?;
            let scores = merge_scores_with_ids(scores, &event.player_tags, &event.metadata.children, event.r#type, 0)?;
            Ok(make_fed_event(event, FedEventData::Flyout {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                fielder_name: fielder_name.to_string(),
                scores,
            }))
        }
        EventType::GroundOut => {
            let (parsed_out, scores) = run_parser(&event, parse_ground_out)?;
            let scores = merge_scores_with_ids(scores, &event.player_tags, &event.metadata.children, event.r#type, 0)?;
            match parsed_out {
                ParsedGroundOut::Simple { batter_name, fielder_name } => {
                    Ok(make_fed_event(event, FedEventData::GroundOut {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                        fielder_name: fielder_name.to_string(),
                        scores,
                    }))
                }
                ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base } => {
                    Ok(make_fed_event(event, FedEventData::FieldersChoice {
                        game: GameEvent::try_from_event(event)?,
                        runner_out_name: runner_out_name.to_string(),
                        batter_name: batter_name.to_string(),
                        out_at_base: base,
                    }))
                }
                ParsedGroundOut::DoublePlay { batter_name } => {
                    Ok(make_fed_event(event, FedEventData::DoublePlay {
                        game: GameEvent::try_from_event(event)?,
                        batter_name: batter_name.to_string(),
                    }))
                }
            }
        }
        EventType::HomeRun => {
            let mut children = event.metadata.children.iter();
            let (batter_name, num_runs, free_refillers) = run_parser(&event, parse_hr)?;
            Ok(make_fed_event(event, FedEventData::HomeRun {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                batter_id: get_one_player_id(event)?,
                num_runs,
                free_refills: free_refillers.into_iter()
                    .map(|refiller_name| {
                        make_free_refill(event.r#type, &mut children, refiller_name)
                    })
                    .collect::<Result<_, _>>()?,
            }))
        }
        EventType::Hit => {
            let (batter_name, num_bases, scores) = run_parser(&event, parse_hit)?;
            if let Some((&batter_id, scorer_ids)) = event.player_tags.split_first() {
                let scores = merge_scores_with_ids(scores, scorer_ids, &event.metadata.children, event.r#type, 1)?;

                Ok(make_fed_event(event, FedEventData::Hit {
                    game: GameEvent::try_from_event(event)?,
                    batter_name: batter_name.to_string(),
                    batter_id,
                    num_bases,
                    scores,
                }))
            } else {
                Err(FeedParseError::MissingTags { event_type: event.r#type, tag_type: "player" })
            }
        }
        EventType::GameEnd => { todo!() }
        EventType::BatterUp => {
            let (batter_name, inhabited, team_name, wielding_item) = run_parser(&event, parse_batter_up)?;

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
                    let (&inhabiting_player_id, &inhabited_player_id) = event.player_tags.iter().collect_tuple()
                        .ok_or_else(|| FeedParseError::WrongNumberOfTags {
                            event_type: event.r#type,
                            tag_type: "player",
                            expected_num: 2,
                            actual_num: event.player_tags.len(),
                        })?;

                    Ok(Inhabiting {
                        sub_event: SubEvent::from_event(child),
                        inhabited_player_name: inhabited.to_string(),
                        inhabiting_player_id,
                        inhabited_player_id,
                    })
                }).transpose()?,
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
            assert_eq!(event.description, "The Electricity zaps a strike away!");
            Ok(make_fed_event(event, FedEventData::StrikeZapped {
                game: GameEvent::try_from_event(event)?,
            }))
        }
        EventType::WeatherChange => { todo!() }
        EventType::MildPitch => { todo!() }
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
        EventType::BlackHole => { todo!() }
        EventType::Sun2 => { todo!() }
        EventType::BirdsCircle => { todo!() }
        EventType::FriendOfCrows => { todo!() }
        EventType::BirdsUnshell => { todo!() }
        EventType::BecomeTripleThreat => { todo!() }
        EventType::GainFreeRefill => { todo!() }
        EventType::CoffeeBean => { todo!() }
        EventType::FeedbackBlocked => { todo!() }
        EventType::FeedbackSwap => { todo!() }
        EventType::SuperallergicReaction => { todo!() }
        EventType::AllergicReaction => { todo!() }
        EventType::ReverbBestowsReverberating => { todo!() }
        EventType::ReverbRosterShuffle => { todo!() }
        EventType::Blooddrain => { todo!() }
        EventType::BlooddrainSiphon => { todo!() }
        EventType::BlooddrainBlocked => { todo!() }
        EventType::Incineration => { todo!() }
        EventType::IncinerationBlocked => { todo!() }
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

            let mod_name = mod_add_event.metadata.other
                .as_object()
                .ok_or_else(|| FeedParseError::NoMetadata { event_type: event.r#type })?
                .get("mod")
                .ok_or_else(|| FeedParseError::MissingMetadata { event_type: event.r#type, field: "mod" })?
                .as_str()
                .ok_or_else(|| FeedParseError::MissingMetadata { event_type: event.r#type, field: "mod" })?;

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
        EventType::Perk => { todo!() }
        EventType::Earlbird => { todo!() }
        EventType::LateToTheParty => { todo!() }
        EventType::ShameDonor => { todo!() }
        EventType::AddedMod => { todo!() }
        EventType::RemovedMod => { todo!() }
        EventType::ModExpires => { todo!() }
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
        EventType::AddedModFromOtherMod => { todo!() }
        EventType::ChangedModFromOtherMod => { todo!() }
        EventType::TeamWasShamed => { todo!() }
        EventType::TeamDidShame => { todo!() }
        EventType::RunsScored => { todo!() }
        EventType::WinCollectedRegular => { todo!() }
        EventType::WinCollectedPostseason => { todo!() }
        EventType::GameOver => { todo!() }
        EventType::StormWarning => { todo!() }
        EventType::Snowflakes => { todo!() }
    }
}

fn merge_scores_with_ids(scores: Vec<ParsedScore>, scorer_ids: &[Uuid], children: &[EventuallyEvent], event_type: EventType, extra_player_tags: usize) -> Result<Vec<Score>, FeedParseError> {
    let mut children = children.iter();
    if scorer_ids.len() == scores.len() {
        let result = scores.into_iter().zip(scorer_ids)
            .map(|(score, &scorer_id)| Ok(Score {
                player_id: scorer_id,
                player_name: score.name.to_string(),
                free_refill: if let Some(refiller_name) = score.free_refiller {
                    Some(make_free_refill(event_type, &mut children, refiller_name)?)
                } else {
                    None
                },
            }))
            .collect::<Result<_, _>>()?;

        if children.next().is_none() {
            Ok(result)
        } else {
            Err(FeedParseError::MissingChild {
                event_type,
                expected_num_children: 0,
            })
        }
    } else {
        Err(FeedParseError::WrongNumberOfTags {
            event_type,
            tag_type: "player",
            expected_num: scores.len() + extra_player_tags,
            actual_num: scorer_ids.len() + extra_player_tags,
        })
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

fn get_one_id(tag_type: &'static str, tags: &[Uuid], event_type: EventType) -> Result<Uuid, FeedParseError> {
    if let Some((first, rest)) = tags.split_first() {
        if rest.is_empty() {
            Ok(*first)
        } else {
            Err(FeedParseError::WrongNumberOfTags {
                event_type,
                tag_type,
                expected_num: 1,
                actual_num: tags.len(),
            })
        }
    } else {
        Err(FeedParseError::MissingTags { event_type, tag_type })
    }
}

fn get_one_player_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    get_one_id("player", &event.player_tags, event.r#type)
}

fn get_one_team_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    get_one_id("team", &event.team_tags, event.r#type)
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
        let (input, parsed_value) = take_until1(tag_content)(input)?;
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
    let (input, num_str) = many1(digit1)(input)?;
    // The parser should ensure num_str always represents a valid number
    Ok((input, num_str.join("").parse().unwrap()))
}

fn parse_batter_up(input: &str) -> ParserResult<(&str, Option<&str>, &str, Option<&str>)> {
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

    Ok((input, (batter_name, inhabiting_name, team_name, wielding_item)))
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

fn parse_flyout(input: &str) -> ParserResult<(&str, &str, Vec<ParsedScore>)> {
    let (input, batter_name) = parse_terminated(" hit a flyout to ")(input)?;
    let (input, fielder_name) = parse_terminated(".")(input)?;

    let (input, scores) = parse_scores(" tags up and scores!")(input)?;

    Ok((input, (batter_name, fielder_name, scores)))
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

fn parse_ground_out(input: &str) -> ParserResult<(ParsedGroundOut, Vec<ParsedScore>)> {
    alt((parse_simple_ground_out, parse_fielders_choice, parse_double_play))(input)
}

fn parse_simple_ground_out(input: &str) -> ParserResult<(ParsedGroundOut, Vec<ParsedScore>)> {
    let (input, batter_name) = parse_terminated(" hit a ground out to ")(input)?;
    let (input, fielder_name) = parse_terminated(".")(input)?;

    let (input, scores) = parse_scores(" advances on the sacrifice.")(input)?;

    Ok((input, (ParsedGroundOut::Simple { batter_name, fielder_name }, scores)))
}

fn parse_fielders_choice(input: &str) -> ParserResult<(ParsedGroundOut, Vec<ParsedScore>)> {
    let (input, runner_out_name) = parse_terminated(" out at ")(input)?;
    let (input, base) = parse_named_base(input)?;
    let (input, _) = tag(" base.\n")(input)?;
    let (input, batter_name) = parse_terminated(" reaches on fielder's choice.")(input)?;

    // TODO scoring on FC
    Ok((input, (ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base }, vec![])))
}

fn parse_double_play(input: &str) -> ParserResult<(ParsedGroundOut, Vec<ParsedScore>)> {
    let (input, batter_name) = parse_terminated(" hit into a double play!")(input)?;

    // TODO scoring on DP
    Ok((input, (ParsedGroundOut::DoublePlay { batter_name }, vec![])))
}

fn parse_hit(input: &str) -> ParserResult<(&str, i32, Vec<ParsedScore>)> {
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_bases) = alt((
        tag("Single!").map(|_| 1),
        tag("Double!").map(|_| 2),
        tag("Triple!").map(|_| 3),
        tag("Quadruple!").map(|_| 4),
    ))(input)?;

    let (input, scores) = parse_scores(" scores!")(input)?;

    Ok((input, (batter_name, num_bases, scores)))
}

struct ParsedScore<'a> {
    name: &'a str,
    free_refiller: Option<&'a str>,
}

fn parse_free_refill(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("\n")(input)?;
    let (input, name) = parse_terminated(" used their Free Refill.\n")(input)?;
    let (input, _) = tag(name)(input)?;
    let (input, _) = tag(" Refills the In!")(input)?;

    Ok((input, name))
}

fn parse_scores<'a>(score_label: &'static str) -> impl FnMut(&'a str) -> ParserResult<Vec<ParsedScore<'a>>> {
    alt((
        eof.map(|_| Vec::new()), // No scores
        preceded(tag("\n"), separated_list0(tag("\n"), parse_score(score_label)))
    ))
}

fn parse_score(score_label: &'static str) -> impl Fn(&str) -> ParserResult<ParsedScore> {
    move |input| {
        let (input, name) = parse_terminated(score_label)(input)?;
        let (input, free_refiller) = opt(parse_free_refill)(input)?;

        if let Some(free_refiller) = free_refiller { assert_eq!(name, free_refiller); }

        Ok((input, ParsedScore { name, free_refiller }))
    }
}

fn parse_hr(input: &str) -> ParserResult<(&str, i32, Vec<&str>)> {
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_runs) = alt((
        tag("solo home run!").map(|_| 1),
        tag("2-run home run!").map(|_| 2),
        tag("3-run home run!").map(|_| 3),
        tag("grand slam!").map(|_| 4), // dunno what happens with a pentaslam...
    ))(input)?;

    let (input, free_refillers) = many0(parse_free_refill)(input)?;

    Ok((input, (batter_name, num_runs, free_refillers)))
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

fn parse_walk(input: &str) -> ParserResult<&str> {
    let (input, batter_name) = parse_terminated(" draws a walk.")(input)?;

    Ok((input, batter_name))
}

fn parse_inning_end(input: &str) -> ParserResult<i32> {
    let (input, _) = tag("Inning ")(input)?;
    let (input, inning_num) = parse_whole_number(input)?;
    let (input, _) = tag(" is now an Outing.")(input)?;

    Ok((input, inning_num))
}