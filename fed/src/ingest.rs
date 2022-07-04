use std::sync::{Arc, Mutex};
use rocket::futures::{pin_mut, StreamExt};
use rocket::tokio::task;

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

    while let Some(event) = event_stream.next().await {
        println!("Got event: {:?}", event);
    }
}