use std::str::FromStr;
use uuid::Uuid;
use eventually_api::{EventType, EventuallyEvent};
use itertools::Itertools;
use crate::parse::error::FeedParseError;

pub fn get_one_sub_event_from_slice(children: &[EventuallyEvent], event_type: EventType) -> Result<&EventuallyEvent, FeedParseError> {
    let (sub_event, ) = children.iter().collect_tuple()
        .ok_or_else(|| {
            FeedParseError::MissingChild {
                event_type,
                expected_num_children: 1,
            }
        })?;
    Ok(sub_event)
}

pub fn get_one_sub_event(event: &EventuallyEvent) -> Result<&EventuallyEvent, FeedParseError> {
    get_one_sub_event_from_slice(&event.metadata.children, event.r#type)
}

pub fn get_one_or_zero_sub_events(event: &EventuallyEvent) -> Result<Option<&EventuallyEvent>, FeedParseError> {
    if event.metadata.children.is_empty() {
        Ok(None)
    } else {
        get_one_sub_event_from_slice(&event.metadata.children, event.r#type).map(|v| Some(v))
    }
}

pub fn get_two_sub_events(event: &EventuallyEvent) -> Result<(&EventuallyEvent, &EventuallyEvent), FeedParseError> {
    let (a, b) = event.metadata.children.iter().collect_tuple()
        .ok_or_else(|| {
            FeedParseError::MissingChild {
                event_type: event.r#type,
                expected_num_children: 2,
            }
        })?;
    Ok((a, b))
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

pub fn get_uuid_metadata(event: &EventuallyEvent, field: &'static str) -> Result<Uuid, FeedParseError> {
    Uuid::from_str(get_str_metadata(event, field)?)
        .map_err(|_| FeedParseError::MissingMetadata {
            event_type: event.r#type,
            field
        })
}

pub fn get_str_vec_metadata<'a>(event: &'a EventuallyEvent, field: &'static str) -> Result<Vec<&'a str>, FeedParseError> {
    event.metadata.other
        .as_object()
        .and_then(|obj| obj.get(field))
        .and_then(|to| {
            to.as_array()
                .and_then(|arr| arr.iter().map(|v| v.as_str()).collect::<Option<Vec<_>>>())
        })
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

pub fn get_int_metadata(event: &EventuallyEvent, field: &'static str) -> Result<i64, FeedParseError> {
    event.metadata.other
        .as_object()
        .and_then(|obj| obj.get(field))
        .and_then(|to| to.as_i64())
        .ok_or_else(|| {
            FeedParseError::MissingMetadata {
                event_type: event.r#type,
                field,
            }
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

pub fn get_one_or_zero_player_ids(event: &EventuallyEvent) -> Result<Option<Uuid>, FeedParseError> {
    Ok(if event.player_tags.is_empty() {
        None
    } else {
        Some(get_one_id("player", &event.player_tags, event.r#type)?)
    })
}

pub fn get_one_team_id(event: &EventuallyEvent) -> Result<Uuid, FeedParseError> {
    get_one_id("team", &event.team_tags, event.r#type)
}

pub fn get_sub_play(event: &EventuallyEvent) -> Result<i64, FeedParseError> {
    event.metadata.sub_play
        .ok_or_else(|| FeedParseError::MissingMetadata {
            event_type: event.r#type,
            field: "subPlay"
        })
}

fn get_two_ids(tag_type: &'static str, tags: &[Uuid], event_type: EventType) -> Result<(Uuid, Uuid), FeedParseError> {
    tags.iter()
        .cloned()
        .collect_tuple()
        .ok_or_else(||
            FeedParseError::WrongNumberOfTags {
                event_type,
                tag_type,
                expected_num: 2,
                actual_num: tags.len(),
            })
}

pub fn get_two_player_ids(event: &EventuallyEvent) -> Result<(Uuid, Uuid), FeedParseError> {
    get_two_ids("player", &event.player_tags, event.r#type)
}

// pub fn get_two_team_ids(event: &EventuallyEvent) -> Result<(Uuid, Uuid), FeedParseError> {
//     get_two_ids("team", &event.team_tags, event.r#type)
// }
