#![feature(let_chains)]

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, prelude::*};
use par_iter_sync::IntoParallelIteratorAsync;
use json_structural_diff::JsonDiff;
use anyhow::{anyhow, Context};
use indicatif::{MultiProgress, ProgressDrawTarget, ProgressStyle};
use with_structure::WithStructure;
use enum_flatten::{EnumFlatten, EnumFlattened};
use clap::Parser;
use itertools::Itertools;
use eventually_api::EventuallyEvent;

use fed::{FedEvent, InterEventStateSync, parse_next_event};

const SEASONS: [(&'static str, i64, i64); 7] = [
    // sim, season number, number of events in that season
    ("thisidisstaticyo", 11, 330308),
    ("thisidisstaticyo", 12, 407451),
    ("thisidisstaticyo", 13, 391106),
    ("thisidisstaticyo", 14, 388905),
    ("thisidisstaticyo", 15, 377404),
    ("thisidisstaticyo", 16, 377163),
    ("thisidisstaticyo", 17, 365149),

    // ("thisidisstaticyo", 18, 397322),
    // ("thisidisstaticyo", 19, 411748),
    // ("thisidisstaticyo", 20, 401722),
    // ("thisidisstaticyo", 21, 410259),
    // ("thisidisstaticyo", 22, 408058),
    // ("thisidisstaticyo", 23, 354855),
];

fn check_parse(parsed_event: &FedEvent, source_events: &[EventuallyEvent]) -> anyhow::Result<()> {
    let parsed_event_flat = EnumFlatten::flatten(parsed_event.clone());
    let parsed_event_inflat = EnumFlattened::unflatten(parsed_event_flat.clone());

    assert!(&parsed_event_inflat == parsed_event);

    let reconstructed_events = parsed_event.clone().into_feed_events();

    // JsonDiff is expensive. Only run it if the events don't compare equal.
    if source_events != reconstructed_events {
        let original_event_json = serde_json::to_value(&source_events)
            .context("Failed to convert original events to serde_json::Value")?;

        let reconstructed_event_json = serde_json::to_value(reconstructed_events)
            .context("Failed to convert reconstructed events to serde_json::Value")?;
        JsonDiff::diff_string(&reconstructed_event_json, &original_event_json, false)
            .map_or_else(|| Ok(()),
                         |str| {
                             let expected = serde_json::to_string_pretty(&original_event_json).unwrap();
                             let actual = serde_json::to_string_pretty(&reconstructed_event_json).unwrap();
                             Err(anyhow!("Received from Feed: {expected}\nParsed into: {parsed_event:#?}\n Produced: {actual}\nDiff: {str}"))
                         })
            .with_context(|| format!("Event not reconstructed exactly: {}",
                                     original_event_json.get("description").and_then(|v| v.as_str()).unwrap_or("\"\"")))?;
    }
    Ok(())
}

#[derive(Parser, Clone)]
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

    let progress = MultiProgress::new();
    progress.set_move_cursor(true);

    let capture_progress = progress.clone();
    let iter = SEASONS
        .into_par_iter_async(move |(sim, season, count)| {
            Ok(run_test_on_season(sim, season, count, &capture_progress, args.clone()))
        });

    for value in iter {
        value?;
    }

    println!("Done");

    Ok(())
}

fn run_test_on_season(sim: &str, season: i64, total_events: i64, multi_progress: &MultiProgress, args: Args) -> anyhow::Result<()> {
    // If these files doesn't exist, download feed_dump.ndjson from
    // https://faculty.sibr.dev/~allie/feed_dump.ndjson.zstd
    // and run `filter_feed` to make feed_dump.filtered.ndjson
    let file = File::open(format!("feed_dump_filtered/sim-{sim}-season-{season}.ndjson"))?;
    // let reader = BufReader::new(GzDecoder::new(file));
    let reader = BufReader::new(file);

    let state = InterEventStateSync::new();
    let consumed = RefCell::new(Vec::new());

    let mut event_iter = reader.lines()
        .map(|json_str| {
            let str = json_str.context("Failed to read line from ndjson file")?;
            if str.contains("\"_eventually_ingest_source\":\"blaseball.com_library\"") {
                return Ok::<Option<EventuallyEvent>, anyhow::Error>(None);
            }
            let mut feed_event = fed::feed_event_from_json(&str)
                .context(str)
                .context("Failed to parse ndjson entry into EventuallyEvent")?;

            // I don't want to reconstruct these, so I'm None-ing them
            feed_event.metadata.ingest_source = None;
            feed_event.metadata.ingest_time = None;

            consumed.borrow_mut().push(feed_event.clone());

            Ok(Some(feed_event))
        })
        .filter_map(Result::transpose)
        // TODO Is there a better way to do this where I don't have to unwrap?
        .map(Result::unwrap)
        .peekable();

    let mut with_structures = HashSet::<<FedEvent as WithStructure>::Structure>::new();

    let progress = indicatif::ProgressBar::new(total_events as u64);
    progress.set_style(ProgressStyle::with_template("{msg:7} {wide_bar} {human_pos}/{human_len} {elapsed} eta {eta}")?);
    progress.set_draw_target(ProgressDrawTarget::stdout_with_hz(2 /* hz */));
    let progress = multi_progress.add(progress);
    while let Some(parsed_event) = parse_next_event(&mut event_iter, state.inner())
        .with_context(|| format!("Parsing events: \n{}", consumed.borrow().iter()
            .map(|event| format!("  - {}: {}", event.id, event.description)).format("\n")))? {
        check_parse(&parsed_event, &consumed.borrow())?;
        progress.inc(consumed.borrow().len() as u64);
        progress.set_message(format!("s{}d{}", parsed_event.season + 1, parsed_event.day + 1));
        consumed.borrow_mut().clear();

        let Some(ref sample_path) = args.sample_outputs else {
            continue;
        };

        let structure = parsed_event.structure();

        if !with_structures.contains(&structure) {
            with_structures.insert(structure);

            std::fs::write(
                sample_path.join(format!("/{}.json", parsed_event.id)),
                serde_json::to_string_pretty(&parsed_event)?,
            )?;
        }
    }

    progress.finish();

    Ok(())
}