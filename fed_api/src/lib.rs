mod eventually;
mod eventually_schema;

pub use eventually::{events, EventuallyEvent, EventuallyEventBuilder};
pub use eventually_schema::{EventType, EventMetadata, EventMetadataBuilder, Weather};