use thiserror::Error;
use uuid::Uuid;
use fed_api::EventType;

#[derive(Error, Debug)]
pub enum FeedParseError {
    #[error("Unknown phase {phase} for {event_type:?} event")]
    UnknownPhase {
        phase: i32,
        event_type: EventType
    },

    #[error("Expected metadata field \"{field}\" for {event_type:?} event")]
    MissingMetadata {
        event_type: EventType,
        field: &'static str,
    },

    #[error("Unexpected value \"{value}\" in metadata field \"{field}\" for {event_type:?} event")]
    UnexpectedMetadataValue {
        event_type: EventType,
        field: &'static str,
        value: String,
    },

    #[error("Expected {tag_type} tag(s) for {event_type:?} event")]
    MissingTags {
        event_type: EventType,
        tag_type: &'static str,
    },

    #[error("Expected {expected_num} {tag_type} tag(s) for {event_type:?} event but saw {actual_num}")]
    WrongNumberOfTags {
        event_type: EventType,
        tag_type: &'static str,
        expected_num: usize,
        actual_num: usize,
    },

    #[error("Expected equal {tag_type} tag(s) for {event_type:?} event but saw {tag1} and {tag2}")]
    ExpectedEqualTags {
        event_type: EventType,
        tag_type: &'static str,
        tag1: Uuid,
        tag2: Uuid,
    },

    #[error("Expected {expected_num_children} children for {event_type:?} event but got fewer")]
    MissingChild {
        event_type: EventType,
        expected_num_children: i32,
    },

    #[error("Expected {expected_num_children} children for {event_type:?} event but got more")]
    ExtraChild {
        event_type: EventType,
        expected_num_children: i32,
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