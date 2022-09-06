use itertools::Itertools;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_till1, take_until1};
use nom::{Finish, IResult, Parser};
use nom::character::complete::digit1;
use nom::error::convert_error;
use nom::multi::many1;
use uuid::Uuid;
use fed_api::{EventuallyEvent, EventType, Weather};
use crate::error::FeedParseError;
use crate::event_schema::{Being, FedEvent, FedEventData, GameEvent, SubEvent};

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
        EventType::StolenBase => { todo!() }
        EventType::Walk => { todo!() }
        EventType::Strikeout => { todo!() }
        EventType::FlyOut => {
            let (batter_name, fielder_name) = run_parser(&event, parse_flyout)?;
            Ok(make_fed_event(event, FedEventData::Flyout {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                fielder_name: fielder_name.to_string(),
            }))
        }
        EventType::GroundOut => { todo!() }
        EventType::HomeRun => {
            let (batter_name, num_runs) = run_parser(&event, parse_hr)?;
            Ok(make_fed_event(event, FedEventData::HomeRun {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                batter_id: get_one_player_id(event)?,
                num_runs,
            }))
        }
        EventType::Hit => {
            let (batter_name, num_bases) = run_parser(&event, parse_hit)?;
            Ok(make_fed_event(event, FedEventData::Hit {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                batter_id: get_one_player_id(event)?,
                num_bases,
            }))
        }
        EventType::GameEnd => { todo!() }
        EventType::BatterUp => {
            let (batter_name, team_name, wielding_item) = run_parser(&event, parse_batter_up)?;

            // I missed `team_name: "Millennials, wielding An Actual Airplane"` once and I don't
            // want something like that to happen again
            assert!(is_known_team_nickname(team_name));

            Ok(make_fed_event(event, FedEventData::BatterUp {
                game: GameEvent::try_from_event(event)?,
                batter_name: batter_name.to_string(),
                team_name: team_name.to_string(),
                wielding_item: wielding_item.map(|s| s.to_string()),
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
        EventType::StrikeZapped => { todo!() }
        EventType::WeatherChange => { todo!() }
        EventType::MildPitch => { todo!() }
        EventType::InningEnd => { todo!() }
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
        EventType::PeanutFlavorText => { todo!() }
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
                    num_children: 1,
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

fn get_one_player_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    event.player_tags.iter()
        .exactly_one()
        .map(|u| *u)
        .map_err(|_| FeedParseError::MissingTags {
            event_type: EventType::Undefined,
            tag_type: "player",
        })
}

fn get_one_team_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    event.team_tags.iter()
        .exactly_one()
        .map(|u| *u)
        .map_err(|_| FeedParseError::MissingTags {
            event_type: EventType::Undefined,
            tag_type: "team",
        })
}

fn is_known_team_name(name: &str) -> bool {
    vec!["Hawai'i Fridays", "Canada Moist Talkers", "San Francisco Lovers",
         "Breckenridge Jazz Hands", "Hellmouth Sunbeams", "Hades Tigers", "Mexico City Wild Wings",
         "Boston Flowers", "New York Millennials", "Philly Pies",
    ].contains(&name)
}

fn is_known_team_nickname(name: &str) -> bool {
    vec!["Fridays", "Moist Talkers", "Lovers", "Jazz Hands", "Sunbeams", "Tigers", "Wild Wings",
         "Flowers", "Millennials", "Pies",
    ].contains(&name)
}

type ParserResult<'a, Out> = IResult<&'a str, Out, nom::error::VerboseError<&'a str>>;

fn run_parser<'a, F, Out>(event: &'a EventuallyEvent, parser: F) -> Result<Out, FeedParseError>
    where F: Fn(&'a str) -> ParserResult<'a, Out> {
    let (_, output) = parser(&event.description)
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

fn parse_batter_up(input: &str) -> ParserResult<(&str, &str, Option<&str>)> {
    let (input, batter_name) = parse_terminated(" batting for the ")(input)?;
    // This is going to fail if a team ever has a period or comma in it
    let (input, team_name) = take_till1(|c| c == ',' || c == '.')(input)?;
    let (input, wielding_item) = alt((
        // No legacy item
        tag(".").map(|_| None),
        // Legacy item
        parse_wielding_item.map(|s| Some(s))
    ))(input)?;

    Ok((input, (batter_name, team_name, wielding_item)))
}

fn parse_wielding_item(input: &str) -> ParserResult<&str> {
    let (input, _) = tag(", wielding ")(input)?;
    take_until1(".")(input)
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

fn parse_flyout(input: &str) -> ParserResult<(&str, &str)> {
    let (input, batter_name) = parse_terminated(" hit a flyout to ")(input)?;
    let (input, fielder_name) = parse_terminated(".")(input)?;

    Ok((input, (batter_name, fielder_name)))
}

fn parse_hit(input: &str) -> ParserResult<(&str, i32)> {
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_bases) = alt((
        tag("Single!").map(|_| 1),
        tag("Double!").map(|_| 2),
        tag("Triple!").map(|_| 3),
        tag("Quadruple!").map(|_| 4),
    ))(input)?;

    Ok((input, (batter_name, num_bases)))
}

fn parse_hr(input: &str) -> ParserResult<(&str, i32)> {
    let (input, batter_name) = parse_terminated(" hits a ")(input)?;
    let (input, num_runs) = alt((
        tag("solo home run!").map(|_| 1),
        tag("two-run home run!").map(|_| 2),
        tag("three-run home run!").map(|_| 3),
        tag("grand slam!").map(|_| 4), // dunno what happens with a pentaslam...
    ))(input)?;

    Ok((input, (batter_name, num_runs)))
}