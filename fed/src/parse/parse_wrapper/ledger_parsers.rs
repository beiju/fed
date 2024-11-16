use nom::{Finish, Parser};
use nom::bytes::complete::tag;
use nom::error::{convert_error, VerboseError};
use eventually_api::EventType;
use with_structure::WithStructure;
use crate::{FeedParseError, HeatMagnetLedger, HomeRunLedger, LedgerRun, LedgerRunModifier, LedgerV2, ModerationLedger, RunSource, SimpleLedgerV2, StolenBaseLedger, TripleThreatLedger};
use crate::parse::parsers::*;

pub trait ParseableLedger {
    type Ledger: LedgerV2;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError>;
}

fn parse_modifiers(mut ledger: &str) -> Result<(&str, Vec<LedgerRunModifier>), FeedParseError> {
    let mut modifiers = Vec::new();
    loop {
        let (rest, parsed_modifier) = parse_ledger(parse_ledger_v2_modifier, ledger)?;
        ledger = rest;

        match parsed_modifier {
            None => { break Ok((ledger, modifiers))  }
            Some(ParsedLedgerV2Modifier::Magnified { position, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Magnified(position));
            }
            Some(ParsedLedgerV2Modifier::Underhanded { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Underhanded);
            }
            Some(ParsedLedgerV2Modifier::SunPoint1 { value, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::SunPoint1(value));
            }
            Some(ParsedLedgerV2Modifier::Subtractor { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Subtractor);
            }
            Some(ParsedLedgerV2Modifier::AcidicPitch { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::AcidicPitch);
            }
        }
    }
}

impl<RunSourceT: WithStructure + RunSource> ParseableLedger for SimpleLedgerV2<RunSourceT> {
    type Ledger = Self;

    fn parse(mut ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let mut runs = Vec::new();

        loop {
            // Try to parse the ledger line; if we can, continue. If we can't, break
            let mut parsed_run;
            (ledger, parsed_run) = parse_ledger(parse_ledger_v2_run(RunSourceT::label()), ledger)?;

            if !parsed_run {
                break Ok((ledger, SimpleLedgerV2::from_runs(runs)));
            }

            let mut run = LedgerRun::default();
            (ledger, run.modifiers) = parse_modifiers(ledger)?;

            runs.push(run);
        }
    }
}

impl ParseableLedger for HomeRunLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, home_run) = SimpleLedgerV2::parse(ledger)?;

        let (ledger, big_bucket) = if ledger.starts_with("Big Bucket") {
            let (rest, bucket) = SimpleLedgerV2::parse(&ledger)?;
            (rest, Some(bucket))
        } else {
            (ledger, None)
        };

        let (ledger, alley_oop) = if ledger.starts_with("Slam Dunk") {
            let (rest, oop) = SimpleLedgerV2::parse(&ledger)?;
            (rest, Some(oop))
        } else {
            (ledger, None)
        };

        Ok((ledger, Self {
            home_run,
            big_bucket,
            alley_oop,
        }))
    }
}

fn parse_ledger<'a, O>(mut parser: impl Parser<&'a str, O, VerboseError<&'a str>>, ledger: &'a str) -> Result<(&'a str, O), FeedParseError> {
    parser.parse(ledger)
        .finish()
        .map_err(|e| {
            FeedParseError::ScoreLedgerParseError {
                event_type: EventType::RunsScored,
                err: convert_error(ledger, e),
                original: ledger.to_string(),
            }
        })
}

impl ParseableLedger for ModerationLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, value) = parse_ledger(parse_ledger_moderation, ledger)?;
        Ok((ledger, ModerationLedger::new(value)))
    }
}

impl ParseableLedger for TripleThreatLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, _) = parse_ledger(parse_ledger_triple_threat, ledger)?;
        let (ledger, modifiers) = parse_modifiers(ledger)?;

        Ok((ledger, TripleThreatLedger::new(modifiers)))
    }
}

impl ParseableLedger for HeatMagnetLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, _) = parse_ledger(tag("Heat Magnet: 5 Runs"), ledger)?;

        Ok((ledger, HeatMagnetLedger::new()))
    }
}

impl ParseableLedger for StolenBaseLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        // TODO This doesn't account for modifiers between stolen base and blaserunning
        let (ledger, (steal_home, blaserunning)) = parse_ledger(parse_ledger_stolen_base, ledger)?;

        let (ledger, steal_home) = if steal_home {
            let (ledger, modifiers) = parse_modifiers(ledger)?;
            (ledger, Some(LedgerRun::new(modifiers)))
        } else {
            (ledger, None)
        };

        let (ledger, blaserunning) = if blaserunning {
            let (ledger, modifiers) = parse_modifiers(ledger)?;
            (ledger, Some(LedgerRun::new(modifiers)))
        } else {
            (ledger, None)
        };

        Ok((ledger, Self { steal_home, blaserunning }))
    }
}