use nom::{Finish, Parser};
use nom::error::convert_error;
use with_structure::WithStructure;
use crate::{FeedParseError, Ledger, LedgerRun, LedgerRunModifier, LedgerV2, RunSource, SimpleLedgerV2};
use crate::parse::parse_wrapper::EventParseWrapper;
use crate::parse::parsers::*;

pub trait ParseableLedger {
    type Ledger: LedgerV2;

    fn parse(event: &mut EventParseWrapper) -> Result<Ledger<Self::Ledger>, FeedParseError>;
}

impl<RunSourceT: WithStructure + RunSource> ParseableLedger for SimpleLedgerV2<RunSourceT> {
    type Ledger = Self;

    fn parse(event: &mut EventParseWrapper) -> Result<Ledger<Self>, FeedParseError> {
        let score_ledger = event.metadata_str("ledger")?;
        let (_, lines) = parse_score_ledger_v2(RunSourceT::label()).parse(score_ledger).finish()
            .map_err(|e| {
                FeedParseError::ScoreLedgerParseError {
                    event_type: event.event_type,
                    err: convert_error(score_ledger, e),
                    original: score_ledger.to_string(),
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
                ParsedLedgerLineV2::Magnified { .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::Magnified);
                }
                ParsedLedgerLineV2::Underhanded { .. } => { // TODO Verify run numbers are as expected
                    let run = active_run.as_mut().unwrap(); // TODO make this a Result
                    run.modifiers.push(LedgerRunModifier::Underhanded);
                }
            }
        }

        if let Some(finished_run) = active_run.take() {
            runs.push(finished_run);
        }

        Ok(Ledger::V2(SimpleLedgerV2::from_runs(runs)))
    }
}