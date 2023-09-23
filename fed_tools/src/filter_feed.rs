use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::Value;

fn parse(s: io::Result<String>) -> (String, bool, i64, String, DateTime<Utc>) {
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

    (s, has_parent, season, sim, created)
}

fn main() {
    let file = File::open("feed_dump_iso.ndjson").unwrap();
    let reader = BufReader::new(file);

    let feed_era_start = chrono::DateTime::parse_from_rfc3339("2021-03-01T05:00:00.000Z").unwrap().with_timezone(&Utc);

    let mut vec: Vec<_> = reader.lines()
        .map(parse)
        .filter(move |(_, has_parent, _, _, created)| {
            created >= &feed_era_start && !*has_parent
        })
        .collect();
    vec.sort_by_key(|(_, _, season, sim, _)| (sim.clone(), *season));
    let groups = vec.into_iter().group_by(|(_, _, season, sim, _)| (sim.clone(), *season));

    for ((sim, season), group) in &groups {
        println!("Processing sim {sim} season {season}");
        let mut vec: Vec<_> = group.map(|(s, _, _, _, created)| (s, created)).collect();

        vec.sort_by_key(|(_, created)| created.clone());
        std::fs::write(format!("feed_dump_filtered/sim-{sim}-season-{season}.ndjson"),
                       vec.iter().map(|(s, _)| s).join("\n")).unwrap();
    }
}