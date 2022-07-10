use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;
use fed_api::{EventType, EventuallyEvent};

#[derive(Debug)]
pub enum Being {
    EmergencyAlert,
    TheShelledOne,
    TheMonitor,
    TheCoin,
    TheReader,
    TheMicrophone,
    Lootcrates,
    Namerifeht,
}

impl Being {
    pub fn from_id(being_id: i64) -> Option<Being> {
        Some(match being_id {
            -1 => Being::EmergencyAlert,
            0 => Being::TheShelledOne,
            1 => Being::TheMonitor,
            2 => Being::TheCoin,
            3 => Being::TheReader,
            4 => Being::TheMicrophone,
            5 => Being::Lootcrates,
            6 => Being::Namerifeht,
            _ => return None
        })
    }

    pub fn id(&self) -> i64 {
        match self {
            Being::EmergencyAlert => -1,
            Being::TheShelledOne => 0,
            Being::TheMonitor => 1,
            Being::TheCoin => 2,
            Being::TheReader => 3,
            Being::TheMicrophone => 4,
            Being::Lootcrates => 5,
            Being::Namerifeht => 6,
        }
    }
}

#[derive(Debug)]
pub enum FedEventData {
    BeingSpeech {
        being: Being,
        message: String,
    },

    LetsGo,

    PlayBall,
}

#[derive(Debug)]
pub struct FedEvent {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: String,
    pub tournament: i32,
    pub season: i32,
    pub day: i32,
    pub phase: i32,
    pub nuts: i32,
    pub data: FedEventData,

}

impl FedEvent {
    pub fn into_feed_event(self) -> EventuallyEvent {
        let mut event = EventuallyEvent {
            id: self.id,
            created: self.created,
            r#type: EventType::Undefined,
            category: 0,
            metadata: Default::default(),
            blurb: "".to_string(),
            description: "".to_string(),
            player_tags: vec![],
            game_tags: vec![],
            team_tags: vec![],
            sim: self.sim,
            day: self.day,
            season: self.season,
            tournament: self.tournament,
            phase: self.phase,
            nuts: self.nuts,

        };

        match self.data {
            FedEventData::BeingSpeech { being, message } => {
                event.r#type = EventType::BigDeal;
                event.category = 4;
                event.description = message;
                event.metadata.other = json!({
                    "being": being.id()
                });
            }
            FedEventData::LetsGo => {
                event.r#type = EventType::LetsGo;
                event.description = "Let's Go!".to_string()
            }
            FedEventData::PlayBall => { todo!() }
        }


        event
    }
}