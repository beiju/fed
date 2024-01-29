use thiserror::Error;
use uuid::Uuid;
use eventually_api::EventType;

#[derive(Error, Debug)]
pub enum FeedParseError {
    #[error(transparent)]
    EventuallyEventJsonParseFailed(#[from] serde_json::Error),

    #[error("Parsing {event_type:?} did not parse end of description: {remaining}")]
    DescriptionNotFullyParsed {
        event_type: EventType,
        remaining: String,
    },

    #[error("Description parse error for {event_type:?} event: {err}")]
    DescriptionParseError {
        event_type: EventType,
        err: String,
    },

    #[error("Expected {tag_type} tag(s) to be non-null for {event_type:?} event")]
    MissingTags {
        event_type: EventType,
        tag_type: &'static str,
    },

    #[error("Expected at least {expected_at_least} {tag_type} tag(s) for {event_type:?} event")]
    NotEnoughTags {
        event_type: EventType,
        tag_type: &'static str,
        expected_at_least: usize,
    },

    #[error("Expected {expected} {tag_type} tag(s) for {event_type:?} event but found more")]
    TooManyTags {
        event_type: EventType,
        tag_type: &'static str,
        expected: usize,
    },

    #[error("Expected equal {tag_type} tag(s) for {event_type:?} event but saw {tag1} and {tag2}")]
    ExpectedEqualTags {
        event_type: EventType,
        tag_type: &'static str,
        tag1: Uuid,
        tag2: Uuid,
    },

    #[error("Expected at least {expected_at_least} children for {event_type:?} event")]
    NotEnoughChildren {
        event_type: EventType,
        expected_at_least: usize,
    },

    #[error("Expected {expected} children {event_type:?} event but found more")]
    TooManyChildren {
        event_type: EventType,
        expected: usize,
    },

    #[error("Unexpected event type {child_event_type:?} as {child_number}th child of {event_type:?} event")]
    UnexpectedChildType {
        event_type: EventType,
        child_event_type: EventType,
        child_number: usize,
    },

    #[error("Expected metadata to be an object for {event_type:?} event")]
    MetadataWasNotAnObject {
        event_type: EventType,
    },

    #[error("Expected metadata field \"{field}\" for {event_type:?} event")]
    MissingMetadata {
        event_type: EventType,
        field: String,
    },

    #[error("Expected metadata field \"{field}\" for {event_type:?} event to have type {ty}")]
    MetadataTypeError {
        event_type: EventType,
        field: String,
        ty: &'static str,
    },

    #[error("Couldn't convert field \"{field}\" for {event_type:?} event to Uuid: {err}")]
    MetadataStrToUuidError {
        event_type: EventType,
        field: &'static str,
        err: uuid::Error,
    },

    #[error("Couldn't convert field \"{field}\" for {event_type:?} event to Enum: {err}")]
    MetadataIntToEnumError {
        event_type: EventType,
        field: String,
        err: String,
    },

    #[error("Unexpected value \"{value}\" in metadata field \"{field}\" for {event_type:?} event")]
    UnexpectedMetadataValue {
        event_type: EventType,
        field: &'static str,
        value: String,
    },

    #[error("Unknown phase {phase} for {event_type:?} event")]
    UnknownPhase {
        phase: i32,
        event_type: EventType
    },

    #[error("Unknown being id {0}")]
    UnknownBeing(i32),

    #[error("Unknown weather {0}")]
    UnknownWeather(i32),

    #[error("Expected location to be one of {expected:?} but it was {actual}")]
    InvalidLocation {
        expected: &'static [i64],
        actual: i64,
    },

    #[error("Expected one of {expected_types:?} event after {after_type:?} event but found {found_type:?}")]
    MissingFollowingEvent {
        expected_types: Vec<EventType>,
        found_type: Option<EventType>,
        after_type: EventType,
    },
}

