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

    #[error("Expected {num_children} children for {event_type:?} event")]
    MissingChild {
        event_type: EventType,
        num_children: i32,
    },

    #[error("Unexpected type {child_type:?} for child of {event_type:?} event")]
    UnexpectedChildType {
        event_type: EventType,
        child_type: EventType,
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

    #[error("Unexpected mod name in {event_type:?} event: {mod_name}")]
    UnexpectedModName {
        event_type: EventType,
        mod_name: String,
    },

    #[error("Description parse error for {event_type:?} event: {err}")]
    DescriptionParseError {
        event_type: EventType,
        err: String,
    },

    #[error("Description parse error for {event_type:?} event: {err}")]
    MissingPlayerId {
        event_type: EventType,
        err: String,
    },
}