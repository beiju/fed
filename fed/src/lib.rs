#![feature(let_chains)]
mod parse;

pub use parse::stream::expansion_era_events;
pub use parse::event_schema::FedEvent;
pub use parse::parse_feed_event;
pub use parse::feed_event_from_json;
