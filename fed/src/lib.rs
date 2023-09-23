#![feature(let_chains)]
mod parse;
mod fed_event;

pub use parse::stream::{EXPANSION_ERA_START, EXPANSION_ERA_END};
pub use eventually_api::Weather;
pub use fed_event::*;
pub use parse::{parse_next_event, feed_event_from_json, InterEventStateSync};
pub use parse::error::FeedParseError;
