use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_until1, take_while};
use nom::{Finish, IResult, Parser};
use nom::character::complete::digit1;
use nom::character::is_digit;
use nom::error::convert_error;
use nom::multi::many1;
use fed_api::{EventuallyEvent, EventType, Weather};
use crate::error::FeedParseError;
use crate::event_schema::{Being, FedEvent, FedEventData, GameEvent};

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
            let (_, (top_of_inning, inning, team_name)) = parse_half_inning(&event.description)
                .finish()
                .map_err(|e| FeedParseError::DescriptionParseError {
                    event_type: event.r#type,
                    err: convert_error(&event.description as &str, e),
                })?;

            Ok(make_fed_event(event, FedEventData::HalfInningStart {
                game: GameEvent::try_from_event(event)?,
                top_of_inning,
                inning,
                batting_team_name: team_name.to_string()
            }))
        }
        EventType::PitcherChange => { todo!() }
        EventType::StolenBase => { todo!() }
        EventType::Walk => { todo!() }
        EventType::Strikeout => { todo!() }
        EventType::FlyOut => { todo!() }
        EventType::GroundOut => { todo!() }
        EventType::HomeRun => { todo!() }
        EventType::Hit => { todo!() }
        EventType::GameEnd => { todo!() }
        EventType::BatterUp => { todo!() }
        EventType::Strike => { todo!() }
        EventType::Ball => { todo!() }
        EventType::FoulBall => { todo!() }
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
        EventType::Superyummy => { todo!() }
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

fn parse_half_inning(input: &str) -> IResult<&str, (bool, i32, &str), nom::error::VerboseError<&str>> {
    let (input, top_of_inning) = alt((
        tag("Top").map(|_| true),
        tag("Bottom").map(|_| false),
    ))(input)?;

    let (input, _) = tag(" of ")(input)?;
    // TODO There has to be a better way to match a whole number
    let (input, inning_str) = many1(digit1)(input)?;
    // The parser should ensure inning_str always represents a valid number
    let inning = inning_str.join("").parse().unwrap();

    let (input, _) = tag(", ")(input)?;
    let (input, team_name) = take_until1(" batting.")(input)?;
    let (input, _) = tag(" batting.")(input)?;

    Ok((input, (top_of_inning, inning, team_name)))
}