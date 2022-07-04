use std::collections::{HashMap, HashSet};
use futures::{future, Stream, stream, StreamExt};
use log::{info, warn};

pub use crate::api::eventually_schema::{EventuallyEvent, EventuallyResponse};

const PAGE_SIZE: usize = 100;

pub fn events(start: &'static str) -> impl Stream<Item=EventuallyEvent> {
    eventually_pages(start)
        .flat_map(|vec| stream::iter(vec.into_iter()))
        .scan(HashSet::new(), |seen_ids, mut event| {
            // If this event was already seen as a sibling of a processed event, skip it
            if seen_ids.remove(&event.id) {
                // info!("Discarding duplicate event {} from {}", event.description, event.created);
                // Double-option because the outer layer is used by `scan` to terminate the iterator
                return future::ready(Some(None));
            }

            // seen_ids shouldn't grow very large, since every uuid that's put into it should come
            // out within a few seconds
            if seen_ids.len() > 50 {
                warn!("seen_ids is larger than expected ({} ids)", seen_ids.len());
            }

            for sibling in &event.metadata.siblings {
                if sibling.id != event.id {
                    seen_ids.insert(sibling.id);
                }
            }

            let id_order: HashMap<_, _> = event.metadata.sibling_ids.iter()
                .flatten()
                .enumerate()
                .map(|(i, uuid)| (uuid, i))
                .collect();

            event.metadata.siblings.sort_by_key(|event| id_order.get(&event.id).unwrap());

            // Parents don't always end up being the first item
            let parent_event = if let Some(first_sibling) = event.metadata.siblings.first() {
                if first_sibling.id != event.id {
                    let mut parent_event = first_sibling.clone();
                    parent_event.metadata.siblings = event.metadata.siblings;
                    parent_event
                } else {
                    event
                }
            } else {
                event
            };

            // info!("Yielding event {} from {}", parent_event.description, parent_event.created);
            // Double-option because the outer layer is used by `scan` to terminate the iterator
            future::ready(Some(Some(parent_event)))
        })
        .flat_map(|maybe_event| stream::iter(maybe_event.into_iter()))
}

struct EventuallyState {
    page: usize,
    stop: bool,
    cache: sled::Db,
    client: reqwest::Client,
}

fn eventually_pages(start: &'static str) -> impl Stream<Item=Vec<EventuallyEvent>> {
    let start_state = EventuallyState {
        page: 0,
        stop: false,
        cache: sled::open("http_cache/eventually/").unwrap(),
        client: reqwest::Client::new(),
    };

    stream::unfold(start_state, move |state| async move {
        if state.stop {
            None
        } else {
            Some(eventually_page(start, state).await)
        }
    })
}

//noinspection SpellCheckingInspection
async fn eventually_page(start: &'static str, state: EventuallyState) -> (Vec<EventuallyEvent>, EventuallyState) {
    let request = state.client.get("https://api.sibr.dev/eventually/v2/events")
        .query(&[
            ("limit", PAGE_SIZE),
            ("offset", state.page * PAGE_SIZE),
        ])
        .query(&[
            ("expand_siblings", "true"),
            ("sortby", "{created}"),
            ("sortorder", "asc"),
            ("after", start)
        ]);

    let request = request.build().unwrap();

    let cache_key = request.url().to_string();

    let response = match state.cache.get(&cache_key).unwrap() {
        Some(text) => bincode::deserialize(&text).unwrap(),
        None => {
            info!("Fetching page {} of feed events from network", state.page);

            let text = state.client
                .execute(request).await
                .expect("Eventually API call failed")
                .text().await
                .expect("Failed to decode Eventually API response");

            state.cache.insert(&cache_key, bincode::serialize(&text).unwrap()).unwrap();

            text
        }
    };

    let response: EventuallyResponse = serde_json::from_str(&response).unwrap();


    let len = response.len();

    (
        response.0,
        EventuallyState {
            page: state.page + 1,
            stop: len < PAGE_SIZE,
            cache: state.cache,
            client: state.client,
        }
    )
}