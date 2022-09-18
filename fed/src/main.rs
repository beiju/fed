mod ingest;
mod event_schema;
mod parse;
mod error;
mod feed_event_util;

#[macro_use] extern crate rocket;

use rocket::fairing::AdHoc;
use crate::ingest::{IngestTask, IngestTaskHolder};

const FEED_ERA_START: &'static str = "2021-03-01T05:00:00.000Z";

#[get("/hello/<name>/<age>")]
fn hello(name: &str, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let _rocket = rocket::build()
        .mount("/hello", routes![hello])
        .manage(IngestTaskHolder::default())
        .attach(AdHoc::on_liftoff("Fed Ingest", |rocket| Box::pin(async {
            let task_holder: &IngestTaskHolder = rocket.state().unwrap();

            let ingest_task = IngestTask::new(FEED_ERA_START);
            let mut task_mut = task_holder.latest_ingest.lock().unwrap();
            *task_mut = Some(ingest_task);
        })))
        .launch()
        .await?;

    Ok(())
}
