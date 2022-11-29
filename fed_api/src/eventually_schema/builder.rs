use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::{EventCategory, EventType, EventMetadataBuilder, EventuallyEvent};

pub struct EventBuilderCommon {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub sim: String,
    pub day: i32,
    pub phase: i32,
    pub season: i32,
    pub tournament: i32,
    pub nuts: i32,
}

pub struct ChildExtra {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub nuts: i32,
}

impl EventBuilderCommon {
    pub fn update(self, update: EventBuilderUpdate) -> EventBuilderFull {
        EventBuilderFull {
            common: self,
            game: None,
            update,
        }
    }

    pub fn for_game(self, game: impl Into<EventBuilderGame>) -> EventBuilderForGame {
        EventBuilderForGame {
            common: self,
            game: game.into(),
        }
    }

    pub fn child(self, extra: impl Into<ChildExtra>) -> Self {
        let extra = extra.into();
        Self {
            id: extra.id,
            created: extra.created,
            nuts: extra.nuts,
            ..self
        }
    }
}

#[derive(Default)]
pub struct EventBuilderUpdate {
    pub r#type: EventType,
    pub category: EventCategory,
    pub description: String,
    pub player_tags: Vec<Uuid>,
    pub team_tags: Vec<Uuid>,
    pub metadata: EventMetadataBuilder,
}

pub struct EventBuilderGame {
    pub game_id: Uuid,
    pub away_team_id: Uuid,
    pub home_team_id: Uuid,
    pub play: i64,
}

pub struct EventBuilderForGame {
    pub common: EventBuilderCommon,
    pub game: EventBuilderGame,
}

impl EventBuilderForGame {
    pub fn update(self, update: EventBuilderUpdate) -> EventBuilderFull {
        EventBuilderFull {
            common: self.common,
            game: Some(self.game),
            update,
        }
    }
}

pub struct EventBuilderFull {
    pub common: EventBuilderCommon,
    pub game: Option<EventBuilderGame>,
    pub update: EventBuilderUpdate,
}

impl EventBuilderFull {
    pub fn build(self) -> EventuallyEvent {
        EventuallyEvent {
            id: self.common.id,
            created: self.common.created,
            r#type: self.update.r#type,
            category: self.update.category,
            metadata: self.update.metadata.build().unwrap(),
            blurb: "".to_string(),
            description: self.update.description,
            player_tags: self.update.player_tags,
            game_tags: self.game.as_ref().map_or_else(|| Vec::new(), |g| vec![g.game_id]),
            team_tags: self.game.as_ref().map_or_else(|| Vec::new(), |g| {
                vec![g.away_team_id, g.home_team_id]
            }),
            sim: self.common.sim,
            day: self.common.day,
            season: self.common.season,
            tournament: self.common.tournament,
            phase: self.common.phase,
            nuts: self.common.nuts
        }
    }
}