use uuid::Uuid;
use fed_api::{EventType, EventuallyEvent};
use itertools::Itertools;
use crate::error::FeedParseError;

pub fn get_one_sub_event(event: &EventuallyEvent) -> Result<&EventuallyEvent, FeedParseError> {
    let (sub_event, ) = event.metadata.children.iter().collect_tuple()
        .ok_or_else(|| FeedParseError::MissingChild {
            event_type: event.r#type,
            expected_num_children: 1,
        })?;
    Ok(sub_event)
}

pub fn get_str_metadata<'a>(event: &'a EventuallyEvent, field: &'static str) -> Result<&'a str, FeedParseError> {
    event.metadata.other
        .as_object()
        .and_then(|obj| obj.get(field))
        .and_then(|to| to.as_str())
        .ok_or_else(|| FeedParseError::MissingMetadata {
            event_type: event.r#type,
            field,
        })
}

pub fn get_float_metadata(event: &EventuallyEvent, field: &'static str) -> Result<f64, FeedParseError> {
    event.metadata.other
        .as_object()
        .and_then(|obj| obj.get(field))
        .and_then(|to| to.as_f64())
        .ok_or_else(|| FeedParseError::MissingMetadata {
            event_type: event.r#type,
            field,
        })
}

fn get_one_id(tag_type: &'static str, tags: &[Uuid], event_type: EventType) -> Result<Uuid, FeedParseError> {
    if let Some((first, rest)) = tags.split_first() {
        if rest.is_empty() {
            Ok(*first)
        } else {
            Err(FeedParseError::WrongNumberOfTags {
                event_type,
                tag_type,
                expected_num: 1,
                actual_num: tags.len(),
            })
        }
    } else {
        Err(FeedParseError::MissingTags { event_type, tag_type })
    }
}

pub fn get_one_player_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    get_one_id("player", &event.player_tags, event.r#type)
}

pub fn get_one_team_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    get_one_id("team", &event.team_tags, event.r#type)
}
