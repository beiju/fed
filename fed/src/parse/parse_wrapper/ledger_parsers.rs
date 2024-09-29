use nom::{Finish, Parser};
use nom::bytes::complete::tag;
use nom::combinator::opt;
use nom::error::convert_error;
use eventually_api::EventType;
use with_structure::WithStructure;
use crate::{FeedParseError, HomeRunLedger, Ledger, LedgerRun, LedgerRunModifier, LedgerV2, RunSource, SimpleLedgerV2};
use crate::parse::parse_wrapper::EventParseWrapper;
use crate::parse::parsers::*;

pub trait ParseableLedger {
    type Ledger: LedgerV2;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError>;
}

impl<RunSourceT: WithStructure + RunSource> ParseableLedger for SimpleLedgerV2<RunSourceT> {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (rest, lines) = parse_score_ledger_v2(RunSourceT::label()).parse(ledger).finish()
            .map_err(|e| {
                FeedParseError::ScoreLedgerParseError {
                    event_type: EventType::RunsScored,
                    err: convert_error(ledger, e),
                    original: ledger.to_string(),
                }
            })?;

        let mut runs = Vec::new();
        let mut active_run = None;

        for line in lines {
            match line {
                ParsedLedgerLineV2::Run => {
                    if let Some(finished_run) = active_run.replace(LedgerRun::default()) {
                        runs.push(finished_run);
                    }
                },
                ParsedLedgerLineV2::Magnified { position, .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::Magnified(position));
                }
                ParsedLedgerLineV2::Underhanded { .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::Underhanded);
                }
                ParsedLedgerLineV2::SunPoint1 { value, .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::SunPoint1(value));
                }
                ParsedLedgerLineV2::Subtractor { .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::Subtractor);
                }
            }
        }

        if let Some(finished_run) = active_run.take() {
            runs.push(finished_run);
        }

        Ok((rest, SimpleLedgerV2::from_runs(runs)))
    }
}

impl ParseableLedger for HomeRunLedger {
    type Ledger = Self;

    fn parse(ledger: &str) -> Result<(&str, Self::Ledger), FeedParseError> {
        let (ledger, home_run) = SimpleLedgerV2::parse(ledger)?;

        let (ledger, alley_oop) = if ledger.starts_with("\nSlam Dunk") {
            let (rest, oop) = SimpleLedgerV2::parse(&ledger[1..])?;
            (rest, Some(oop))
        } else {
            (ledger, None)
        };

        Ok((ledger, Self {
            home_run,
            alley_oop,
        }))
    }
}