use std::sync::{Arc, Mutex};
use rocket::futures::{pin_mut, StreamExt};
use rocket::tokio::task;
use crate::parse;

#[derive(Default)]
pub struct IngestTaskHolder {
    pub latest_ingest: Arc<Mutex<Option<IngestTask>>>,
}

pub struct IngestTask {

}

impl IngestTask {
    pub fn new(start: &'static str) -> IngestTask {
        task::spawn(ingest_main(start));

        IngestTask {}
    }
}

async fn ingest_main(start: &'static str) {
    let event_stream = fed_api::events(start);

    pin_mut!(event_stream);

    while let Some(feed_event) = event_stream.next().await {
        println!("Parsing {}: {:?}", feed_event.id, feed_event.description);
        // First print it pretty, then unwrap it
        let parsed_event = parse::parse_feed_event(&feed_event).map_err(|err| {
            eprintln!("{err}");
            err
        }).unwrap();
        println!("Got event: {:?}", parsed_event);
        let reconstructed_event = parsed_event.into_feed_event();
        assert_json_diff::assert_json_eq!(feed_event, reconstructed_event);
    }
}