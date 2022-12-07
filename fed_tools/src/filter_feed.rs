use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde_json::Value;

fn parse(s: io::Result<String>) -> (String, bool, DateTime<Utc>) {
    let s = s.unwrap();
    let value = serde_json::from_str::<Value>(&s).unwrap();
    let created_value = value.as_object().unwrap().get("created").unwrap();
    let created: DateTime<Utc> = serde_json::from_value(created_value.clone()).unwrap();

    let has_parent = value.as_object()
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.as_object())
        .map(|v| v.contains_key("parent"))
        .unwrap_or(false);

    (s, has_parent, created)
}

fn main() {
    let mut vec: Vec<_> = {
        let file = File::open("feed_dump_iso.ndjson").unwrap();
        let reader = BufReader::new(file);

        let feed_era_start = chrono::DateTime::parse_from_rfc3339("2021-03-01T05:00:00.000Z").unwrap().with_timezone(&Utc);

        reader.lines()
            .map(parse)
            .filter(move |(s, has_parent, created)| {
                created >= &feed_era_start && !*has_parent
            })
            .map(|(s, _, created)| (s, created))
            .collect()
    };

    vec.sort_by_key(|(s, created)| created.clone());
    std::fs::write("feed_dump.filtered.ndjson", vec.iter().map(|(s, _)| s).join("\n")).unwrap();
}