// const FILE_GZIP: &[u8] = include_bytes!("../../../feed_dump.filtered.ndjson.gz");
pub const EXPANSION_ERA_START: &'static str = "2021-03-01T05:00:00.000Z";
pub const EXPANSION_ERA_END: &'static str = "2021-08-01T00:00:00.000Z"; // i think

// pub fn expansion_era_events() -> impl Iterator<Item=Result<FedEvent, FeedParseError>> {
//     // This is temporary. Eventually (ha) get it from HTTP
//     // Go up to the parent so it works from blarser's CWD too
//     let f = File::open("../fed/feed_dump.filtered.ndjson.gz")
//         .expect("Couldn't open file");
//
//     let mut state = InterEventState::new();
//
//     BufReader::new(GzDecoder::new(f))
//         .lines()
//         .map(move |line| {
//             let line = line.expect("Failed reading from gzip file");
//             let feed_event = parse::feed_event_from_json(&line)?;
//             parse::parse_feed_event(&feed_event, &mut state)
//         })
// }