use thiserror::Error;
use fed_api::EventType;

#[derive(Error, Debug)]
pub enum FeedParseError {
    #[error("Expected metadata to exist for {event_type:?} event")]
    NoMetadata {
        event_type: EventType
    },

    #[error("Expected metadata field \"{field}\" for {event_type:?} event")]
    MissingMetadata {
        event_type: EventType,
        field: &'static str,
    },

    #[error("Expected {tag_type} tag(s) for {event_type:?} event")]
    MissingTags {
        event_type: EventType,
        tag_type: &'static str,
    },

    #[error("Unknown being id {0}")]
    UnknownBeing(i64),

    #[error("Unknown weather {0}")]
    UnknownWeather(i64),

    #[error("Unexpected description for {event_type:?} event: {description} (expected {expected})")]
    UnexpectedDescription {
        event_type: EventType,
        description: String,
        expected: String,
    },

    #[error("Description parse error for {event_type:?} event: {err}")]
    DescriptionParseError {
        event_type: EventType,
        err: String,
    },
}