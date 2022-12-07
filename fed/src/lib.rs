#![feature(let_chains)]
mod parse;

pub use parse::stream::{expansion_era_events, EXPANSION_ERA_START, EXPANSION_ERA_END};
pub use parse::event_schema::*;
pub use parse::{parse_feed_event, feed_event_from_json};
pub use parse::error::FeedParseError;
