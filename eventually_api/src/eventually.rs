use std::collections::{HashMap, HashSet};
use futures::{future, Stream, stream, StreamExt};
use itertools::Itertools;
use log::{info, warn};

pub use crate::eventually_schema::{EventuallyEvent, EventuallyEventBuilder, EventuallyResponse};

const PAGE_SIZE: usize = 500;
const BUFFER_PAGES: usize = 5;

pub fn events(start: &'static str) -> impl Stream<Item=EventuallyEvent> {
    eventually_pages(start, BUFFER_PAGES)
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

            for child in &event.metadata.children {
                seen_ids.insert(child.id);
            }

            let id_order: HashMap<_, _> = event.metadata.sibling_ids.iter()
                .flatten()
                .enumerate()
                .map(|(i, uuid)| (uuid, i))
                .collect();

            // ... why did I want this?
            event.metadata.siblings.sort_by_key(|event| id_order.get(&event.id).unwrap());

            // Parents don't always end up being the first item
            let mut parent_event = if let Some(first_sibling) = event.metadata.siblings.first() {
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

            // Parsing becomes much simpler if children are always in subplay order
            sort_children(&mut parent_event.metadata.children);

            // info!("Yielding event {} from {}", parent_event.description, parent_event.created);
            // Double-option because the outer layer is used by `scan` to terminate the iterator
            future::ready(Some(Some(parent_event)))
        })
        .flat_map(|maybe_event| stream::iter(maybe_event.into_iter()))
}

fn sort_children(children: &mut Vec<EventuallyEvent>) {
    children.sort_by_key(|child| child.metadata.sub_play
        .expect("All child events should have a subPlay"));
    // verify
    for (a, b) in children.iter().tuple_windows() {
        assert!(a.metadata.sub_play != b.metadata.sub_play, "All subPlays should be unique");
    }
    for child in children {
        sort_children(&mut child.metadata.children)
    }
}

fn eventually_pages(start: &'static str, buffer_pages: usize) -> impl Stream<Item=Vec<EventuallyEvent>> {
    let cache = sled::open("http_cache/eventually/").unwrap();
    let client = reqwest::Client::new();

    stream::unfold(1, move |page| {
        let cache = cache.clone();
        let client = client.clone();
        async move {
            Some(((page, cache, client), page + 1))
        }
    })
        // `map` doesn't wait for one future to be ready before starting the next, which is the
        // desired behavior in this case
        .map(move |(page, cache, client)| async move {
            let request = client.get("https://api.sibr.dev/eventually/v2/events")
                .query(&[
                    ("limit", PAGE_SIZE),
                    ("offset", page * PAGE_SIZE),
                ])
                .query(&[
                    ("expand_children", "true"),
                    ("expand_siblings", "true"),
                    ("sortby", "{created}"),
                    ("sortorder", "asc"),
                    ("after", start)
                ]);

            let request = request.build().unwrap();

            let cache_key = request.url().to_string();

            let response = match cache.get(&cache_key).unwrap() {
                Some(text) => pot::from_slice(&text).unwrap(),
                None => {
                    info!("Fetching page {} of feed events from network", page);

                    let text = client
                        .execute(request).await
                        .expect("Eventually API call failed")
                        .text().await
                        .expect("Failed to decode Eventually API response");

                    let response: EventuallyResponse = serde_json::from_str(&text).unwrap();

                    cache.insert(&cache_key, pot::to_vec(&response).unwrap()).unwrap();

                    response
                }
            };

            if response.len() > 0 {
                Some(response.0)
            } else {
                None
            }
        })
        .buffered(buffer_pages)
        // Scan is apparently one of the few iterators where you can return None to stop the stream
        // This is necessary because the items are executed in parallel, so each page doesn't know
        // whether the previous page was empty
        .scan((), |_, page_opt| future::ready(page_opt))
}