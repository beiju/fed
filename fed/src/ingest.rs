use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct IngestTaskHolder {
    pub latest_ingest: Arc<Mutex<Option<IngestTask>>>,
}

pub struct IngestTask {

}

impl IngestTask {
    pub fn new() -> IngestTask {
        IngestTask {}
    }
}