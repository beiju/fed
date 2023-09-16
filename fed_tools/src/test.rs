#![feature(let_chains)]

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, prelude::*};
use par_iter_sync::IntoParallelIteratorSync;
use json_structural_diff::JsonDiff;
use anyhow::{anyhow, Context};
use indicatif::{ProgressDrawTarget, ProgressStyle};
use fed::{FedEvent, InterEventStateSync};
use flate2::read::GzDecoder;
use with_structure::WithStructure;
use enum_flatten::{EnumFlatten, EnumFlattened};
use clap::Parser;

const NUM_EVENTS: u64 = 8299172;

fn check_json_line((i, json_str): (usize, io::Result<String>), state: &InterEventStateSync) -> anyhow::Result<(usize, Option<FedEvent>)> {
    let str = json_str.context("Failed to read line from ndjson file")?;
    if str.contains("\"_eventually_ingest_source\":\"blaseball.com_library\"") {
        return Ok((i, None));
    }
    let feed_event = fed::feed_event_from_json(&str)
        .context(str)
        .context("Failed to parse ndjson entry into EventuallyEvent")?;


    let parsed_event = fed::parse_feed_event(&feed_event, state.inner())
        .with_context(|| format!("Parsing {}: {:?}", feed_event.id, feed_event.description))
        .context("Failed to parse EventuallyEvent into FedEvent")?;

    let prased_event_flat = EnumFlatten::flatten(parsed_event.clone());
    let prased_event_inflat = EnumFlattened::unflatten(prased_event_flat.clone());

    assert!(prased_event_inflat == parsed_event);

    let reconstructed_event = parsed_event.clone().into_feed_event();

    // JsonDiff is expensive. Only run it if the events don't compare equal.
    if feed_event != reconstructed_event {
        let original_event_json = serde_json::to_value(&feed_event)
            .context("Failed to convert original event to serde_json::Value")?;

        let reconstructed_event_json = serde_json::to_value(reconstructed_event)
            .context("Failed to convert reconstructed event to serde_json::Value")?;
        JsonDiff::diff_string(&reconstructed_event_json, &original_event_json, false)
            .map_or_else(|| Ok(()),
                         |str| {
                             let expected = serde_json::to_string_pretty(&original_event_json).unwrap();
                             let actual = serde_json::to_string_pretty(&reconstructed_event_json).unwrap();
                             Err(anyhow!("Received from Feed: {expected}\nParsed into: {parsed_event:#?}\n Produced: {actual}\nDiff: {str}"))
                         })
            .with_context(|| format!("Event not reconstructed exactly: {}", original_event_json.get("description").unwrap().as_str().unwrap()))?;
    }
    Ok((i, Some(parsed_event)))
}

#[derive(Parser)]
struct Args {
    /// Path to save sample outputs, if desired
    #[arg(value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    sample_outputs: Option<std::path::PathBuf>,
}

fn main() -> anyhow::Result<()> {

    run_test(Args::parse())
        .map_err(|err| {
            // Wait until the other threads hopefully clear
            std::thread::sleep(std::time::Duration::from_secs(2));
            err
        })
}

fn run_test(args: Args) -> anyhow::Result<()> {
    println!("Test starting...");

    // If this file doesn't exist, download feed_dump.ndjson from
    // https://faculty.sibr.dev/~allie/feed_dump.ndjson.zstd
    // and run `filter_feed` to make feed_dump.filtered.ndjson
    let file = File::open("feed_dump.filtered.ndjson.gz")?;
    let reader = BufReader::new(GzDecoder::new(file));

    let state = InterEventStateSync::new();

    let iter = reader.lines()
        .enumerate()
        // .map(|args| check_json_line(args));
        .into_par_iter_sync(move |args| Ok::<_, ()>(check_json_line(args, &state)));
        // .flat_map(|args| Ok::<_, ()>(check_json_line(args, &mut state)));

    let mut with_structures = HashSet::<<FedEvent as WithStructure>::Structure>::new();

    let progress = indicatif::ProgressBar::new(NUM_EVENTS);
    progress.set_style(ProgressStyle::with_template("{msg:7} {wide_bar} {human_pos}/{human_len} {elapsed} eta {eta}")?);
    progress.set_draw_target(ProgressDrawTarget::stdout_with_hz(2 /* hz */));
    for item in iter {
        let (i, maybe_value): (usize, Option<FedEvent>) = item?;
        progress.set_position(i as u64);
        let Some(value) = maybe_value else {
            continue
        };

        progress.set_message(format!("s{}d{}", value.season + 1, value.day + 1));

        let Some(ref sample_path) = args.sample_outputs else {
            continue;
        };

        let structure = value.structure();

        if !with_structures.contains(&structure) {
            with_structures.insert(structure);

            std::fs::write(
                sample_path.join(format!("/{}.json", value.id)),
                serde_json::to_string_pretty(&value)?,
            )?;
        }
    }

    progress.finish();

    Ok(())
}