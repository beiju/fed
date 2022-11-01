mod parse;

use schemars::schema_for;
use crate::parse::event_schema::FedEvent;

fn main() {
    let schema = schema_for!(FedEvent);

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}