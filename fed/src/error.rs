use thiserror::Error;
use fed_api::EventType;

#[derive(Error, Debug)]
pub enum FeedParseError {
    #[error("Expected metadata field \"{field}\" for {event_type:?} event")]
    MissingMetadata {
        event_type: EventType,
        field: &'static str
    },

    #[error("Unknown being id {0}")]
    UnknownBeing(i64)
}