#![feature(let_chains)]
mod parse;

pub use parse::stream::{expansion_era_events, EXPANSION_ERA_START, EXPANSION_ERA_END};
pub use parse::event_schema::FedEvent;
pub use parse::parse_feed_event;
pub use parse::feed_event_from_json;
