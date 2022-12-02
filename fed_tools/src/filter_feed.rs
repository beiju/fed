#![feature(backtrace)]

mod parse;

use std::fs::File;
use std::io::{self, prelude::*, BufReader, BufWriter};
use fed_api::{EventuallyEvent};
use par_iter_sync::IntoParallelIteratorSync;
use json_structural_diff::JsonDiff;
use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc, serde::ts_milliseconds};
use itertools::Itertools;
use rocket::serde::Deserialize;
use serde_json::Value;


#[derive(Deserialize)]
struct DeserializeDateTime(#[serde(with = "ts_milliseconds")] DateTime<Utc>);

fn parse(s: io::Result<String>) -> (String, bool, DateTime<Utc>) {
    let s = s.unwrap();
    let value = serde_json::from_str::<Value>(&s).unwrap();
    let created_value = value.as_object().unwrap().get("created").unwrap();
    let created: DeserializeDateTime = serde_json::from_value(created_value.clone()).unwrap();

    let has_parent = value.as_object()
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.as_object())
        .map(|v| v.contains_key("parent"))
        .unwrap_or(false);

    (s, has_parent, created.0)
}

fn main() {
    let mut vec: Vec<_> = {
        let file = File::open("feed_dump.ndjson").unwrap();
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