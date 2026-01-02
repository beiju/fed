use crate::parse::parsers::*;
use crate::{
    FeedParseError, HeatMagnetLedger, HomeRunLedger, LedgerRun, LedgerRunModifier, LedgerV2,
    ModerationLedger, OverflowLedger, RunSource, SimpleLedgerV2, StolenBaseLedger,
    TripleThreatLedger,
};
use eventually_api::EventType;
use nom::bytes::complete::tag;
use nom::{Finish, Parser};
use nom_language::error::{VerboseError, convert_error};
use with_structure::WithStructure;

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
            None => break Ok((ledger, modifiers)),
            Some(ParsedLedgerV2Modifier::Magnified { position, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Magnified { position });
            }
            Some(ParsedLedgerV2Modifier::Underhanded { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Underhanded);
            }
            Some(ParsedLedgerV2Modifier::SunPoint1 { value, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::SunPoint1 { value });
            }
            Some(ParsedLedgerV2Modifier::Subtractor { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Subtractor);
            }
            Some(ParsedLedgerV2Modifier::AcidicPitch { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::AcidicPitch);
            }
            Some(ParsedLedgerV2Modifier::Wired { player_name, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Wired {
                    player_name: player_name.to_string(),
                });
            }
            Some(ParsedLedgerV2Modifier::Tired { player_name, .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::Tired {
                    player_name: player_name.to_string(),
                });
            }
            Some(ParsedLedgerV2Modifier::NegativePolarity { .. }) => {
                // TODO Verify run numbers are as expected
                modifiers.push(LedgerRunModifier::NegativePolarity);
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
            let parsed_run;
            (ledger, parsed_run) = parse_ledger(parse_ledger_v2_run(RunSourceT::label()), ledger)?;

            if !parsed_run {
                // This is the end of the loop. It doesn't look like it, because it's in the middle,
                // but it is
                let (ledger, sum_sun) = parse_ledger(parse_ledger_sum_sun, ledger)?;
                // TODO either verify the value or provide it to the parser
                let (ledger, maximum_sun) = parse_ledger(parse_ledger_maximum_sun, ledger)?;

                break Ok((
                    ledger,
                    SimpleLedgerV2::new(runs, sum_sun, maximum_sun.is_some()),
                ));
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

        // TODO These can probably be collapsed with something like parse_ledger_run_if (but that
        //   parses a whole ledger) (if these even need a whole ledger? can there be multiple of
        //   these? no, right? there's only one ball? so these should be singular too.
        //   TODO that then
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

        let (ledger, sum_sun) = parse_ledger(parse_ledger_sum_sun, ledger)?;
        let (ledger, equal_sun) = parse_ledger(parse_ledger_equal_sun, ledger)?;

        Ok((
            ledger,
            Self {
                home_run,
                big_bucket,
                alley_oop,
                sum_sun,
                equal_sun,
            },
        ))
    }
}

fn parse_ledger<'a, O>(
    mut parser: impl Parser<&'a str, Output = O, Error = VerboseError<&'a str>>,
    ledger: &'a str,
) -> Result<(&'a str, O), FeedParseError> {
    parser
        .parse(ledger)
        .finish()
        .map_err(|e| FeedParseError::ScoreLedgerParseError {
            event_type: EventType::RunsScored,
            err: convert_error(ledger, e),
            original: ledger.to_string(),
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
        let (ledger, threats) = parse_ledger(parse_ledger_triple_threat, ledger)?;
        let (ledger, modifiers) = parse_modifiers(ledger)?;

        Ok((ledger, TripleThreatLedger::new(threats, modifiers)))
    }
}

impl ParseableLedger for HeatMagnetLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, _) = parse_ledger(tag("Heat Magnet: 5 Runs"), ledger)?;

        Ok((ledger, HeatMagnetLedger::new()))
    }
}

impl ParseableLedger for OverflowLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, num_runs) = parse_ledger(parse_ledger_overflow, ledger)?;
        let (ledger, modifiers) = parse_modifiers(ledger)?;

        Ok((ledger, OverflowLedger::new(num_runs, modifiers)))
    }
}

fn parse_ledger_run_if(
    ledger: &str,
    condition: bool,
) -> Result<(&str, Option<LedgerRun>), FeedParseError> {
    Ok(if condition {
        let (ledger, modifiers) = parse_modifiers(ledger)?;
        (ledger, Some(LedgerRun::new(modifiers)))
    } else {
        (ledger, None)
    })
}

impl ParseableLedger for StolenBaseLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, has_steal_home) = parse_ledger(parse_ledger_steal_home, ledger)?;
        let (ledger, steal_home) = parse_ledger_run_if(ledger, has_steal_home)?;

        let (ledger, has_blaserunning) = parse_ledger(parse_ledger_blaserunning, ledger)?;
        let (ledger, blaserunning) = parse_ledger_run_if(ledger, has_blaserunning)?;

        // Don't need to look for sum sun if neither of the other run types happened, but the code
        // looks prettier if we just always look for it
        let (ledger, sum_sun) = parse_ledger(parse_ledger_sum_sun, ledger)?;

        Ok((
            ledger,
            Self {
                steal_home,
                blaserunning,
                sum_sun,
            },
        ))
    }
}
