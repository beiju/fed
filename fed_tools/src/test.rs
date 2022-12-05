#![feature(let_chains)]

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, prelude::*};
use par_iter_sync::IntoParallelIteratorSync;
use json_structural_diff::JsonDiff;
use anyhow::{anyhow, Context};
use indicatif::{ProgressDrawTarget, ProgressStyle};
use fed::FedEvent;
use flate2::read::GzDecoder;
use seen_structure::HasStructure;

const NUM_EVENTS: u64 = 8299172;

fn check_json_line((i, json_str): (usize, io::Result<String>)) -> anyhow::Result<(usize, FedEvent)> {
    let str = json_str.context("Failed to read line from ndjson file")?;
    let feed_event = fed::feed_event_from_json(&str)
        .context(str)
        .context("Failed to parse ndjson entry into EventuallyEvent")?;

    let parsed_event = fed::parse_feed_event(&feed_event)
        .with_context(|| format!("Parsing {}: {:?}", feed_event.id, feed_event.description))
        .context("Failed to parse EventuallyEvent into FedEvent")?;

    let reconstructed_event = parsed_event.clone().into_feed_event();

    // JsonDiff is expensive. Only run it if the events don't compare equal.
    if feed_event != reconstructed_event {
        let original_event_json = serde_json::to_value(&feed_event)
            .context("Failed to convert original event to serde_json::Value")?;

        let reconstructed_event_json = serde_json::to_value(reconstructed_event)
            .context("Failed to convert reconstructed event to serde_json::Value")?;
        JsonDiff::diff_string(&reconstructed_event_json, &original_event_json, false)
            .map_or_else(|| Ok(()),
                         |str| Err(anyhow!("{str}")))
            .with_context(|| format!("Event not reconstructed exactly: {}", reconstructed_event_json.get("description").unwrap().as_str().unwrap()))?;
    }
    Ok((i, parsed_event))
}

fn main() -> anyhow::Result<()> {
    run_test()
        .map_err(|err| {
            // Wait until the other threads hopefully clear
            std::thread::sleep(std::time::Duration::from_secs(2));
            err
        })
}

fn run_test() -> anyhow::Result<()> {
    println!("Test starting...");

    // If this file doesn't exist, download feed_dump.ndjson from
    // https://faculty.sibr.dev/~allie/feed_dump.ndjson.zstd
    // and run `filter_feed` to make feed_dump.filtered.ndjson
    let file = File::open("feed_dump.filtered.ndjson.gz")?;
    let reader = BufReader::new(GzDecoder::new(file));

    let iter = reader.lines()
        .enumerate()
        .into_par_iter_sync(|args| Ok::<_, ()>(check_json_line(args)));

    let mut seen_structures = HashSet::<<FedEvent as HasStructure>::Structure>::new();

    let progress = indicatif::ProgressBar::new(NUM_EVENTS);
    progress.set_style(ProgressStyle::with_template("{msg:7} {wide_bar} {human_pos}/{human_len} {elapsed} eta {eta}")?);
    progress.set_draw_target(ProgressDrawTarget::stdout_with_hz(2 /* hz */));
    for item in iter {
        let (i, value): (usize, FedEvent) = item?;
        progress.set_message(format!("s{}d{}", value.season + 1, value.day + 1));
        progress.set_position(i as u64);

        let structure = value.structure();

        if !seen_structures.contains(&structure) {
            seen_structures.insert(structure);

            std::fs::write(
                format!("sample_outputs/{}.json", value.id),
                serde_json::to_string_pretty(&value)?,
            )?;
        }
    }

    progress.finish();

    Ok(())
}