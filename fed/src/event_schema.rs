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
}

#[derive(Debug)]
pub enum FedEvent {
    BeingSpeech {
        being: Being,
        message: String,
    }
}