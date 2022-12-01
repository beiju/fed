use std::io::{BufRead, BufReader};
use flate2::read::GzDecoder;

use crate::parse;
use crate::parse::error::FeedParseError;
use crate::parse::event_schema::FedEvent;

const FILE_GZIP: &[u8] = include_bytes!("../../../feed_dump.filtered.ndjson.gz");

pub fn expansion_era_events() -> impl Iterator<Item=Result<FedEvent, FeedParseError>> {
    BufReader::new(GzDecoder::new(FILE_GZIP))
        .lines()
        .map(|line| {
            let line = line.expect("Reading from an included byte string shouldn't fail");
            let feed_event = parse::feed_event_from_json(&line)?;
            parse::parse_feed_event(&feed_event)
        })
}