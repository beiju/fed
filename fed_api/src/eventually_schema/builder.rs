use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::{EventCategory, EventMetadata, EventType, EventuallyEvent};

pub struct EventBuilder;

impl EventBuilder {
    pub fn child(sub_event: impl Into<EventBuilderChildCommon>) -> EventBuilderChildCommon {
        sub_event.into()
    }
}

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

impl EventBuilderCommon {
    pub fn update(self, update: EventBuilderUpdate) -> EventBuilderFull {
        EventBuilderFull {
            common: self,
            game: None,
            update,
            children: Vec::new(),
        }
    }

    pub fn for_game(self, game: impl Into<EventBuilderGame>) -> EventBuilderForGame {
        EventBuilderForGame {
            common: self,
            game: game.into(),
        }
    }
}

pub struct EventBuilderChildCommon {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub nuts: i32,
}

impl EventBuilderChildCommon {
    pub fn update(self, update: EventBuilderUpdate) -> EventBuilderChildFull {
        EventBuilderChildFull {
            common: self,
            update,
        }
    }
}

pub struct EventBuilderChildFull {
    pub common: EventBuilderChildCommon,
    pub update: EventBuilderUpdate,
}

impl EventBuilderChildFull {
    pub fn metadata(self, value: serde_json::Value) -> EventBuilderChildFullWithMetadata {
        EventBuilderChildFullWithMetadata {
            common: self.common,
            update: self.update,
            metadata: BuilderMetadata::OnlyOther(value),
        }
    }

    pub fn full_metadata(self, value: EventMetadata) -> EventBuilderChildFullWithMetadata {
        EventBuilderChildFullWithMetadata {
            common: self.common,
            update: self.update,
            metadata: BuilderMetadata::Full(value),
        }
    }

    pub fn no_metadata(self) -> EventBuilderChildFullWithMetadata {
        EventBuilderChildFullWithMetadata {
            common: self.common,
            update: self.update,
            metadata: BuilderMetadata::Full(EventMetadata::default()),
        }
    }
}

pub struct EventBuilderChildFullWithMetadata {
    pub common: EventBuilderChildCommon,
    pub update: EventBuilderUpdate,
    pub metadata: BuilderMetadata,
}

#[derive(Default)]
pub struct EventBuilderUpdate {
    pub r#type: EventType,
    pub category: EventCategory,
    pub description: String,
    pub player_tags: Vec<Uuid>,
    pub team_tags: Vec<Uuid>,
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
            children: Vec::new()
        }
    }
}

pub struct EventBuilderChildFinished {
    pub common: EventBuilderChildCommon,
    pub update: EventBuilderUpdate,
    pub metadata: BuilderMetadata,
}

impl Into<EventBuilderChildFinished> for EventBuilderChildFull {
    fn into(self) -> EventBuilderChildFinished {
        EventBuilderChildFinished {
            common: self.common,
            update: self.update,
            metadata: BuilderMetadata::Full(Default::default()),
        }
    }
}

impl Into<EventBuilderChildFinished> for EventBuilderChildFullWithMetadata {
    fn into(self) -> EventBuilderChildFinished {
        EventBuilderChildFinished {
            common: self.common,
            update: self.update,
            metadata: self.metadata,
        }
    }
}

pub struct EventBuilderFull {
    pub common: EventBuilderCommon,
    pub game: Option<EventBuilderGame>,
    pub update: EventBuilderUpdate,
    pub children: Vec<EventBuilderChildFinished>,
}

impl EventBuilderFull {
    pub fn metadata(self, value: serde_json::Value) -> EventBuilderFullWithMetadata {
        EventBuilderFullWithMetadata {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata: BuilderMetadata::OnlyOther(value),
            children: self.children,
        }
    }

    pub fn full_metadata(self, value: EventMetadata) -> EventBuilderFullWithMetadata {
        EventBuilderFullWithMetadata {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata: BuilderMetadata::Full(value),
            children: self.children,
        }
    }

    pub fn child(self, child: impl Into<EventBuilderChildFinished>) -> Self {
        let mut children = self.children;
        children.push(child.into());
        Self {
            common: self.common,
            game: self.game,
            update: self.update,
            children,
        }
    }

    pub fn children<T: Into<EventBuilderChildFinished>>(self, new_children: impl IntoIterator<Item=T>) -> Self {
        let mut children = self.children;
        children.extend(new_children.into_iter().map(Into::into));
        Self {
            common: self.common,
            game: self.game,
            update: self.update,
            children,
        }
    }

    pub fn build(self) -> EventuallyEvent {
        EventBuilderFullWithMetadata {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata: BuilderMetadata::Full(EventMetadata::default()),
            children: self.children,
        }.build()
    }
}

pub enum BuilderMetadata {
    OnlyOther(serde_json::Value),
    Full(EventMetadata)
}

pub struct EventBuilderFullWithMetadata {
    pub common: EventBuilderCommon,
    pub game: Option<EventBuilderGame>,
    pub update: EventBuilderUpdate,
    pub metadata: BuilderMetadata,
    pub children: Vec<EventBuilderChildFinished>,
}

impl EventBuilderFullWithMetadata {
    pub fn child(self, child: EventBuilderChildFinished) -> Self {
        let mut children = self.children;
        children.push(child);
        Self {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata: self.metadata,
            children,
        }
    }

    pub fn children<T: Into<EventBuilderChildFinished>>(self, new_children: impl IntoIterator<Item=T>) -> Self {
        let mut children = self.children;
        children.extend(new_children.into_iter().map(Into::into));
        Self {
            common: self.common,
            game: self.game,
            update: self.update,
            metadata: self.metadata,
            children,
        }
    }
    pub fn build(self) -> EventuallyEvent {
        let children = self.children.into_iter()
            .map(|_child| {
                todo!()
            })
            .collect();
        EventuallyEvent {
            id: self.common.id,
            created: self.common.created,
            r#type: self.update.r#type,
            category: self.update.category,
            metadata: match self.metadata {
                BuilderMetadata::OnlyOther(other) => {
                    EventMetadata {
                        children,
                        siblings: vec![],
                        ingest_time: None,
                        ingest_source: None,
                        play: self.game.as_ref().map(|g| g.play),
                        sub_play: None,
                        sibling_ids: None,
                        parent: None,
                        other,
                    }
                }
                BuilderMetadata::Full(metadata) => {
                    metadata
                }
            },
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