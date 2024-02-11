use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::Value;

const ALWAYS_SORT_FIRST: [i64; 1] = [171];

#[derive(PartialEq, Eq)]
struct SpecialSortingEventType(i64);

impl PartialOrd<Self> for SpecialSortingEventType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SpecialSortingEventType {
    fn cmp(&self, other: &Self) -> Ordering {
        match (ALWAYS_SORT_FIRST.contains(&self.0), ALWAYS_SORT_FIRST.contains(&other.0)) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self.0.cmp(&other.0),
        }
    }
}

fn parse(s: io::Result<String>) -> (String, bool, i64, String, DateTime<Utc>, i64) {
    let s = s.unwrap();
    let value = serde_json::from_str::<Value>(&s).unwrap();
    let created_value = value.as_object().unwrap().get("created").unwrap();
    let created: DateTime<Utc> = serde_json::from_value(created_value.clone()).unwrap();

    let has_parent = value.as_object()
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.as_object())
        .map(|v| v.contains_key("parent"))
        .unwrap_or(false);

    let season = value.as_object().unwrap()
        .get("season").unwrap()
        .as_i64().unwrap();

    let sim = value.as_object().unwrap()
        .get("sim").unwrap()
        .as_str().unwrap()
        .to_string();

    let event_type = value.as_object().unwrap()
        .get("type").unwrap()
        .as_i64().unwrap();

    (s, has_parent, season, sim, created, event_type)
}

fn main() {
    println!("Opening feed dump...");
    let file = File::open("feed_dump_iso.ndjson").unwrap();
    let reader = BufReader::new(file);

    let feed_era_start = chrono::DateTime::parse_from_rfc3339("2021-03-01T05:00:00.000Z").unwrap().with_timezone(&Utc);

    println!("Collecting feed...");
    let mut vec: Vec<_> = reader.lines()
        .map(parse)
        .filter(move |(_, has_parent, _, _, created, _)| {
            created >= &feed_era_start && !*has_parent
        })
        .collect();
    println!("Sorting...");
    vec.sort_by_key(|(_, _, season, sim, _, _)| (sim.clone(), *season));
    let groups = vec.into_iter().group_by(|(_, _, season, sim, _, _)| (sim.clone(), *season));

    println!("Splitting seasons...");
    for ((sim, season), group) in &groups {
        println!("Processing sim {sim} season {season}...");
        let mut vec: Vec<_> = group.map(|(s, _, _, _, created, event_type)| (s, created, event_type)).collect();

        println!("Sorting sim {sim} season {season}...");
        vec.sort_by_key(|(_, created, event_type)| (*created, SpecialSortingEventType(*event_type)));
        println!("Writing sim {sim} season {season}...");
        std::fs::write(format!("feed_dump_filtered/sim-{sim}-season-{season}.ndjson"),
                       vec.iter().map(|(s, _, _)| s).join("\n")).unwrap();
    }
    println!("Done!");
}