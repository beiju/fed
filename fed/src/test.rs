#![feature(backtrace)]

mod parse;

use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use fed_api::{EventuallyEvent};
use par_iter_sync::IntoParallelIteratorSync;
use json_structural_diff::JsonDiff;
use anyhow::{anyhow, Context};

const NUM_EVENTS: u64 = 8299172;

fn check_json_line((i, json_str): (usize, io::Result<String>)) -> anyhow::Result<(usize, Option<(String, String)>)> {
    let str = json_str.context("Failed to read line from ndjson file")?;
    let feed_event: EventuallyEvent = serde_json::from_str(&str)
        .context(str)
        .context("Failed to parse ndjson entry into EventuallyEvent")?;
    let original_event_json = serde_json::to_value(&feed_event)
        .context("Failed to convert original event to serde_json::Value")?;

    let print = format!("Parsing {}: {:?}", feed_event.id, feed_event.description);
    // First print it pretty, then unwrap it
    let parsed_event = parse::parse_feed_event(&feed_event)
        .context("Failed to parse EventuallyEvent into FedEvent")?;
    // println!("Got event: {:?}", parsed_event);
    let season = parsed_event.season + 1;
    let day = parsed_event.day + 1;
    let reconstructed_event = parsed_event.into_feed_event();

    let reconstructed_event_json = serde_json::to_value(reconstructed_event)
        .context("Failed to convert reconstructed event to serde_json::Value")?;
    JsonDiff::diff_string(&reconstructed_event_json, &original_event_json, false)
        .map_or_else(|| Ok(()),
                     |str| Err(anyhow!("{str}")))?;

    Ok((i, Some(print, format!("s{season}d{day}"))))
}

fn main() -> anyhow::Result<()> {
    // If this file doesn't exist, download feed_dump.ndjson from
    // https://faculty.sibr.dev/~allie/feed_dump.ndjson.zstd
    // and run `filter_feed` to make feed_dump.filtered.ndjson
    let file = File::open("feed_dump.filtered.ndjson")?;
    let reader = BufReader::new(file);

    let iter = reader.lines()
        .enumerate()
        .into_par_iter_sync(|args| Ok::<_, ()>(check_json_line(args)));

    let progress = indicatif::ProgressBar::new(NUM_EVENTS);
    for item in iter {
        let (i, result) = item?;
        if let Some((printout, status)) = result {
            progress.println(printout);
            progress.set_message(status);
        }
        progress.set_position(i as u64);
    }

    progress.finish();

    Ok(())
}