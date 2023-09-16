#![feature(let_chains)]
mod parse;
mod fed_event;

pub use parse::stream::{expansion_era_events, EXPANSION_ERA_START, EXPANSION_ERA_END};
pub use eventually_api::Weather;
pub use fed_event::*;
pub use parse::{parse_feed_event, feed_event_from_json, InterEventStateSync};
pub use parse::error::FeedParseError;
