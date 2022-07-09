use nom::IResult;
use fed_api::{EventuallyEvent, EventType};
use crate::error::FeedParseError;
use crate::event_schema::{Being, FedEvent};

pub fn parse_feed_event(feed_event: EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    if feed_event.metadata.siblings.is_empty() {
        parse_single_feed_event(feed_event)
    } else {
        todo!()
    }
}

fn parse_single_feed_event(event: EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    match event.r#type {
        EventType::Undefined => { todo!() }
        EventType::LetsGo => { todo!() }
        EventType::PlayBall => { todo!() }
        EventType::HalfInning => { todo!() }
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

            Ok(FedEvent::BeingSpeech {
                being: Being::from_id(being_id)
                    .ok_or_else(|| FeedParseError::UnknownBeing(being_id))?,
                message: event.description
            })
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