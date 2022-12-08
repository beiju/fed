pub mod error;
pub mod event_schema;
mod feed_event_util;
pub mod builder;
mod parsers;
pub mod stream;
mod parse_wrapper;

use std::str::FromStr;
use itertools::Itertools;
use serde::Deserialize;
use uuid::{Uuid, uuid};
// the second one is a macro
use eventually_api::{EventType, EventuallyEvent, Weather};

use crate::parse::error::FeedParseError;
use crate::parse::event_schema::*;
use crate::parse::parsers::*;
use crate::parse::parse_wrapper::EventParseWrapper;

pub use stream::expansion_era_events;

const KNOWN_TEAM_NICKNAMES: [&'static str; 24] = [
    "Fridays", "Moist Talkers", "Lovers", "Jazz Hands", "Sunbeams", "Tigers", "Wild Wings",
    "Flowers", "Millennials", "Pies", "Garages", "Dale", "Lift", "Firefighters", "Steaks",
    "Magic", "Breath Mints", "Spies", "Shoe Thieves", "Tacos", "Georgias", "Worms", "Crabs",
    "Mechanics",
];

const TAROT_EVENTS: [Uuid; 8] = [
    uuid!("0d96d9ed-8e40-47ca-a543-b27518b276ef"), // Curry gets Over Under
    uuid!("6dd0204e-213b-4798-9fad-e042a232edc6"), // Krod gets Under Over
    uuid!("760ee47b-7698-4216-9612-e67c13ba12ef"), // Fridays get Sinking Ship
    uuid!("17df7d13-41df-4caf-af56-da75577a43e8"), // Lovers get Base Dealing
    uuid!("6a9e3ad7-f6a7-437c-9bd5-22b602a32cc3"), // Quitter gets Receiver
    uuid!("b0457046-0e88-482a-b3b4-aed27c598a5c"), // Moses gets Receiver
    uuid!("77df7273-e3c3-49b1-9ce5-4baec629d75a"), // Mints get Middling
    uuid!("9cd56488-5ee2-436e-9196-37a76593cdaf"), // Flowers get After Party
];

pub fn parse_feed_event(feed_event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    if feed_event.metadata.siblings.is_empty() {
        parse_single_feed_event(feed_event)
    } else {
        todo!()
    }
}

fn parse_single_feed_event(event: &EventuallyEvent) -> Result<FedEvent, FeedParseError> {
    let mut event = EventParseWrapper::new(event)?;
    // This variable exists just for me to look at in the debugger, because the debugger
    // representation of the Uuid type is to low-level to copy-paste
    let _id_string = event.id.to_string();

    // This can happen on the majority of events, so I handle it outside
    let unscatter = event.next_child_if(EventType::LetsGo, |child| {
        child.event_type == EventType::RemovedMod && child.metadata_str("mod").map_or(false, |m| m == "SCATTERED")
    })?.map(|mut child| {
        let player_name = child.next_parse(parse_terminated(" was Unscattered."))?;
        Ok::<_, FeedParseError>(Unscatter {
            sub_event: child.as_sub_event(),
            team_id: child.next_team_id()?,
            player_id: child.next_player_id()?,
            player_name: player_name.to_string(),
        })
    }).transpose()?;

    // Ditto
    let attractor_secret_base = event.next_parse_opt(parse_terminated(" enters the Secret Base...\n"))
        .map(|name| {
            Ok::<_, FeedParseError>(PlayerInfo {
                player_id: event.next_player_id()?,
                player_name: name.to_string(),
            })
        })
        .transpose()?;

    let data = match event.event_type {
        EventType::Undefined => { todo!() }
        EventType::LetsGo => {
            event.next_parse_tag("Let's Go!")?;
            FedEventData::LetsGo {
                game: event.game(unscatter, attractor_secret_base)?,
                weather: Weather::try_from(event.metadata_i64("weather")? as i32)
                    .map_err(|err| FeedParseError::UnknownWeather(err.number))?,
                stadium_id: event.metadata_uuid("stadium").ok(),
            }
        }
        EventType::PlayBall => {
            event.next_parse_tag("Play ball!")?;

            FedEventData::PlayBall {
                game: event.game(unscatter, attractor_secret_base)?,
            }
        }
        EventType::HalfInning => {
            let (top_of_inning, inning, team_name) = event.next_parse(parse_half_inning)?;
            assert!(is_known_team_name(team_name));

            FedEventData::HalfInningStart {
                game: event.game(unscatter, attractor_secret_base)?,
                top_of_inning,
                inning,
                batting_team_name: team_name.to_string(),
            }
        }
        // EventType::PitcherChange => {
        //     let (victim_name, team_name) = event.next_parse(parse_pitcher_change)?;
        //
        //     assert!(is_known_team_nickname(team_name));
        //
        //     make_fed_event(event, FedEventData::PitcherChange {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_nickname: team_name.to_string(),
        //         pitcher_id: get_one_player_id(player_tags, event.r#type)?,
        //         pitcher_name: victim_name.to_string(),
        //     })
        // }
        // EventType::StolenBase => {
        //     let (runner_name, base_stolen, is_successful, blaserunning, free_refiller) = event.next_parse(parse_stolen_base)?;
        //     if is_successful {
        //         let runner_id = get_one_player_id_advanced(player_tags, event.r#type, blaserunning)?;
        //
        //         make_fed_event(event, FedEventData::StolenBase {
        //             game: event.game(unscatter, attractor_secret_base)?,
        //             runner_name: runner_name.to_string(),
        //             runner_id,
        //             base_stolen,
        //             blaserunning,
        //             free_refill: free_refiller.map(|refiller_name| {
        //                 let sub_event = get_one_sub_event(event)?;
        //                 Ok::<_, FeedParseError>(FreeRefill {
        //                     sub_event: sub_event.as_sub_event(),
        //                     player_name: refiller_name.to_string(),
        //                     player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //                     team_id: sub_event.next_team_id()?,
        //                     sub_play: get_sub_play(sub_event)?,
        //                 })
        //             }).transpose()?,
        //             is_special: event.category == EventCategory::Special,
        //         })
        //     } else {
        //         make_fed_event(event, FedEventData::CaughtStealing {
        //             game: event.game(unscatter, attractor_secret_base)?,
        //             runner_name: runner_name.to_string(),
        //             base_stolen,
        //         })
        //     }
        // }
        // EventType::Walk => {
        //     let parsed_walk = event.next_parse(parse_walk)?;
        //     match parsed_walk {
        //         ParsedWalk::Ordinary((batter_name, scores, base_instincts)) => {
        //             let (&batter_id, scorer_ids) = player_tags.split_first()
        //                 .ok_or_else(|| {
        //                     FeedParseError::WrongNumberOfTags {
        //                         event_type: event.r#type,
        //                         tag_type: "player",
        //                         expected_num: 1 + scores.scorers.len(),
        //                         actual_num: player_tags.len(),
        //                     }
        //                 })?;
        //
        //             let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, &children, event.r#type, 0)?;
        //
        //             make_fed_event(event, FedEventData::Walk {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 batter_id,
        //                 scores,
        //                 stopped_inhabiting,
        //                 base_instincts,
        //                 is_special: event.category == EventCategory::Special,
        //             })
        //         }
        //         ParsedWalk::Charm((batter_name, pitcher_name, scores)) => {
        //             let (&batter_id, rest_ids) = player_tags.split_first()
        //                 .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //                     event_type: event.r#type,
        //                     tag_type: "player",
        //                     expected_num: 2,
        //                     actual_num: player_tags.len(),
        //                 })?;
        //             let (&charmer_id, rest_ids) = rest_ids.split_first()
        //                 .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //                     event_type: event.r#type,
        //                     tag_type: "player",
        //                     expected_num: 2,
        //                     actual_num: player_tags.len(),
        //                 })?;
        //             assert_eq!(batter_id, charmer_id);
        //
        //             let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_ids, &children, event.r#type, 0)?;
        //
        //             make_fed_event(event, FedEventData::CharmWalk {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 batter_id,
        //                 pitcher_name: pitcher_name.to_string(),
        //                 scores,
        //                 stopped_inhabiting,
        //             })
        //         }
        //     }
        // }
        // EventType::Strikeout => {
        //     match event.next_parse(parse_strikeout)? {
        //         ParsedStrikeout::Swinging(batter_name) => {
        //             let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &player_tags, &children, event.r#type, 0)?;
        //             make_fed_event(event, FedEventData::StrikeoutSwinging {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 stopped_inhabiting,
        //                 is_special: event.category == EventCategory::Special,
        //             })
        //         }
        //         ParsedStrikeout::Looking(batter_name) => {
        //             let (_, stopped_inhabiting) = merge_scores_with_ids(ParsedScores::empty(), &player_tags, &children, event.r#type, 0)?;
        //             make_fed_event(event, FedEventData::StrikeoutLooking {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 stopped_inhabiting,
        //                 is_special: event.category == EventCategory::Special,
        //             })
        //         }
        //         ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings } => {
        //             if let Some((&charmer_id, &charmer_id_2, &charmed_id)) = player_tags.iter().collect_tuple() {
        //                 assert_eq!(charmer_id, charmer_id_2);
        //                 make_fed_event(event, FedEventData::CharmStrikeout {
        //                     game: event.game(unscatter, attractor_secret_base)?,
        //                     charmer_id,
        //                     charmer_name: charmer_name.to_string(),
        //                     charmed_id,
        //                     charmed_name: charmed_name.to_string(),
        //                     num_swings,
        //                 })
        //             } else {
        //                 Err(FeedParseError::WrongNumberOfTags {
        //                     event_type: EventType::Strikeout,
        //                     tag_type: "players",
        //                     expected_num: 3,
        //                     actual_num: player_tags.len(),
        //                 })
        //             }
        //         }
        //     }
        // }
        // EventType::FlyOut => {
        //     let (batter_name, fielder_name, scores, cooled_off, batter_debt) = event.next_parse(parse_flyout)?;
        //
        //     let (batter_debt, remaining_player_tags, children) = extract_batter_debt(children, player_tags, event.r#type, batter_debt)?;
        //     let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, children, cooled_off, remaining_player_tags)?;
        //     let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
        //     make_fed_event(event, FedEventData::Flyout {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         batter_name: batter_name.to_string(),
        //         fielder_name: fielder_name.to_string(),
        //         scores,
        //         stopped_inhabiting,
        //         cooled_off,
        //         is_special: event.category == EventCategory::Special,
        //         batter_debt,
        //     })
        // }
        // EventType::GroundOut => {
        //     let (parsed_out, scores, cooled_off) = event.next_parse(parse_ground_out)?;
        //
        //     let has_batter_debt = if let ParsedGroundOut::Simple { batter_debt, .. } = parsed_out {
        //         batter_debt
        //     } else {
        //         false
        //     };
        //     let (batter_debt, remaining_player_tags, children) = extract_batter_debt(children, player_tags, event.r#type, has_batter_debt)?;
        //     let (score_children, cooled_off, remaining_player_tags) = extract_cooled_off_event(event, children, cooled_off, remaining_player_tags)?;
        //     let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, remaining_player_tags, score_children, event.r#type, 0)?;
        //     match parsed_out {
        //         ParsedGroundOut::Simple { batter_name, fielder_name, batter_debt: _ } => {
        //             make_fed_event(event, FedEventData::GroundOut {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 fielder_name: fielder_name.to_string(),
        //                 scores,
        //                 stopped_inhabiting,
        //                 cooled_off,
        //                 is_special: event.category == EventCategory::Special,
        //                 batter_debt,
        //             })
        //         }
        //         ParsedGroundOut::FieldersChoice { runner_out_name, batter_name, base } => {
        //             make_fed_event(event, FedEventData::FieldersChoice {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 runner_out_name: runner_out_name.to_string(),
        //                 batter_name: batter_name.to_string(),
        //                 out_at_base: base,
        //                 scores,
        //                 stopped_inhabiting,
        //                 cooled_off,
        //                 is_special: event.category == EventCategory::Special,
        //             })
        //         }
        //         ParsedGroundOut::DoublePlay { batter_name } => {
        //             make_fed_event(event, FedEventData::DoublePlay {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 batter_name: batter_name.to_string(),
        //                 scores,
        //                 stopped_inhabiting,
        //                 cooled_off,
        //             })
        //         }
        //     }
        // }
        // EventType::HomeRun => {
        //     let (is_magmatic, batter_name, num_runs, free_refillers, spicy_status, big_bucket, attract) =
        //         event.next_parse(parse_hr)?;
        //     let mut expected_children = 0;
        //
        //     let (children, spicy_status) = extract_spicy_event(children, spicy_status)?;
        //     if let SpicyStatus::RedHot(_) = spicy_status { expected_children += 1; }
        //     let (children, magmatic_event) = if is_magmatic {
        //         expected_children += 1;
        //         children.split_first()
        //             .map(|(magmatic_event, children)| {
        //                 (children, Some(magmatic_event))
        //             })
        //             .ok_or_else(move || {
        //                 FeedParseError::MissingChild {
        //                     event_type: event.r#type,
        //                     expected_num_children: expected_children as i32,
        //                 }
        //             })?
        //     } else {
        //         (children, None)
        //     };
        //
        //     let (children, stopped_inhabiting) = if children.is_empty() {
        //         (children, None)
        //     } else if let Some((sub_event, remaining)) = children.split_last() {
        //         run_parser(&sub_event.description, sub_event.r#type, parse_terminated(" stopped Inhabiting."))
        //             .map(|name| {
        //                 Ok::<_, FeedParseError>((remaining, Some(StoppedInhabiting {
        //                     sub_event: sub_event.as_sub_event(),
        //                     inhabiting_player_name: name.to_string(),
        //                     inhabiting_player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //                     inhabiting_player_team_id: if sub_event.team_tags.is_empty() {
        //                         None
        //                     } else {
        //                         Some(sub_event.next_team_id()?)
        //                     },
        //                 })))
        //             })
        //             .unwrap_or(Ok((children, None)))?
        //     } else {
        //         let expected_num_children = children.len() - children.len() + 1;
        //         Err(FeedParseError::MissingChild {
        //             event_type: event.r#type,
        //             expected_num_children: expected_num_children as i32,
        //         })?
        //     };
        //
        //     let (&batter_id, player_tags) = player_tags.split_first()
        //         .ok_or_else(move || {
        //             FeedParseError::WrongNumberOfTags {
        //                 event_type: event.r#type,
        //                 tag_type: "player",
        //                 expected_num: 2,
        //                 actual_num: player_tags.len(),
        //             }
        //         })?;
        //
        //     // Guessing at where this goes in the big list of many events
        //     let (children, player_tags, attraction) = if let Some((team_nickname, player_name)) = attract {
        //         expected_children += 1;
        //         let (child, rest_children)  = children.split_first()
        //             .ok_or_else(move || {
        //                 FeedParseError::MissingChild {
        //                     event_type: event.r#type,
        //                     expected_num_children: expected_children,
        //                 }
        //             })?;
        //
        //         let (_, player_tags) = player_tags.split_first()
        //             .ok_or_else(move || {
        //                 FeedParseError::WrongNumberOfTags {
        //                     event_type: event.r#type,
        //                     tag_type: "player",
        //                     expected_num: 2,
        //                     actual_num: player_tags.len(),
        //                 }
        //             })?;
        //
        //         (rest_children, player_tags, Some(Attraction {
        //             team_nickname: get_str_metadata(child, "teamName")?.to_string(),
        //             team_id: get_uuid_metadata(child, "teamId")?,
        //             player_name: get_str_metadata(child, "playerName")?.to_string(),
        //             player_id: get_uuid_metadata(child, "playerId")?,
        //             sub_event: child.as_sub_event(),
        //         }))
        //     } else {
        //         (children, player_tags, None)
        //     };
        //
        //
        //     make_fed_event(event, FedEventData::HomeRun {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         magmatic: magmatic_event.map(|e| {
        //             Ok::<_, FeedParseError>(ModChangeSubEvent {
        //                 sub_event: e.as_sub_event(),
        //                 team_id: e.next_team_id()?,
        //             })
        //         }).transpose()?,
        //         batter_name: batter_name.to_string(),
        //         batter_id,
        //         num_runs,
        //         stopped_inhabiting,
        //         free_refills: free_refillers.into_iter()
        //             .map(|refiller_name| {
        //                 let mut children = children.iter();
        //                 make_free_refill(event.r#type, &mut children, refiller_name)
        //             })
        //             .collect::<Result<_, _>>()?,
        //         spicy_status,
        //         is_special: event.category == EventCategory::Special,
        //         big_bucket,
        //         attraction,
        //     })
        // }
        // EventType::Hit => {
        //     let (batter_name, num_bases, scores, spicy_status) = event.next_parse(parse_hit)?;
        //     if let Some((&batter_id, scorer_ids)) = player_tags.split_first() {
        //         let scorer_ids = if spicy_status != ParsedSpicyStatus::None {
        //             scorer_ids.split_last()
        //                 .ok_or_else(|| {
        //                     FeedParseError::WrongNumberOfTags {
        //                         event_type: event.r#type,
        //                         tag_type: "player",
        //                         expected_num: scores.scorers.len() + 2, // i think
        //                         actual_num: scorer_ids.len(),
        //                     }
        //                 })?
        //                 .1
        //         } else {
        //             scorer_ids
        //         };
        //
        //         let (score_children, spicy_status) = extract_spicy_event(children, spicy_status)?;
        //         let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, score_children, event.r#type, 1)?;
        //
        //         make_fed_event(event, FedEventData::Hit {
        //             game: event.game(unscatter, attractor_secret_base)?,
        //             batter_name: batter_name.to_string(),
        //             batter_id,
        //             num_bases,
        //             scores,
        //             stopped_inhabiting,
        //             spicy_status,
        //             is_special: event.category == EventCategory::Special,
        //         })
        //     } else {
        //         Err(FeedParseError::MissingTags { event_type: event.r#type, tag_type: "player" })
        //     }
        // }
        // EventType::GameEnd => {
        //     let ((winning_team_name, winning_team_score), (losing_team_name, losing_team_score)) = event.next_parse(parse_game_end)?;
        //     let winner_id = event.metadata.other.as_object()
        //         .and_then(|map| map.get("winner"))
        //         .and_then(|obj| obj.as_str())
        //         .and_then(|uuid_str| Uuid::from_str(uuid_str).ok())
        //         .ok_or_else(|| FeedParseError::MissingMetadata {
        //             event_type: event.r#type,
        //             field: "winner",
        //         })?;
        //     make_fed_event(event, FedEventData::GameEnd {
        //         game: GameEvent::try_from_event_extra_teams(event, unscatter, attractor_secret_base)?,
        //         winner_id,
        //         winning_team_name: winning_team_name.to_string(),
        //         winning_team_score,
        //         losing_team_name: losing_team_name.to_string(),
        //         losing_team_score,
        //     })
        // }
        EventType::BatterUp => {
            let (batter_name, inhabited, team_name, wielding_item, is_repeating) =
                event.next_parse(parse_batter_up)?;
        
            // I missed `team_name: "Millennials, wielding An Actual Airplane"` once and I don't
            // want something like that to happen again
            assert!(is_known_team_nickname(team_name));
        
            FedEventData::BatterUp {
                game: event.game(unscatter, attractor_secret_base)?,
                batter_name: batter_name.to_string(),
                team_name: team_name.to_string(),
                wielding_item: wielding_item.map(|s| s.to_string()),
                inhabiting: inhabited.map(|inhabited| {
                    // Haunting doesn't have a sub-event if the player who Haunted already has the
                    // Inhabiting mod
                    let child = event.next_child_if(EventType::AddedMod, |child| {
                                                        child.event_type == EventType::AddedMod && child.metadata_str("mod").map_or(false, |m| m == "INHABITING")
                                                    })?;
        
                    // These live on the parent
                    let inhabiting_player_id = event.next_player_id()?;
                    let inhabited_player_id = event.next_player_id()?;

                    Ok::<_, FeedParseError>(Inhabiting {
                        sub_event: child.map(|c| c.as_sub_event()),
                        inhabited_player_name: inhabited.to_string(),
                        inhabiting_player_id,
                        inhabited_player_id,
                        inhabiting_player_team_id: child.and_then(|mut c| c.next_team_id_opt()),
                    })
                }).transpose()?,
                is_repeating,
            }
        }
        // EventType::Strike => {
        //     let (strike_type, balls, strikes) = event.next_parse(parse_strike)?;
        //     let game = event.game(unscatter, attractor_secret_base)?;
        //     make_fed_event(event, match strike_type {
        //         StrikeType::Swinging => FedEventData::StrikeSwinging { game, balls, strikes },
        //         StrikeType::Looking => FedEventData::StrikeLooking { game, balls, strikes },
        //         StrikeType::Flinching => FedEventData::StrikeFlinching { game, balls, strikes },
        //     })
        // }
        // EventType::Ball => {
        //     let (balls, strikes) = event.next_parse(parse_ball)?;
        //     make_fed_event(event, FedEventData::Ball {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         balls,
        //         strikes,
        //     })
        // }
        // EventType::FoulBall => {
        //     // Eventually this will need very foul support, but I'll get to that when it comes up
        //     let (balls, strikes) = event.next_parse(parse_foul_ball)?;
        //     make_fed_event(event, FedEventData::FoulBall {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         balls,
        //         strikes,
        //     })
        // }
        // EventType::RunsOverflowing => {
        //     let (team_nickname, num_runs, unruns) = event.next_parse(parse_runs_overflowing)?;
        //     make_fed_event(event, FedEventData::RunsOverflowing {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_nickname: team_nickname.to_string(),
        //         num_runs: if unruns { -num_runs } else { num_runs },
        //     })
        // }
        // EventType::HomeFieldAdvantage => { todo!() }
        // EventType::HitByPitch => {
        //     let (pitcher_name, batter_name, scores) = event.next_parse(parse_hit_by_pitch)?;
        //     let (hbp_player_ids, scorer_ids) = player_tags.split_at(2);
        //
        //     let (pitcher_id, batter_id) = get_two_player_ids_from_slice(hbp_player_ids, event.r#type)?;
        //     let (hbp_children, score_children) = children.split_at(1);
        //     let sub_event = get_one_sub_event_from_slice(hbp_children, event.r#type)?;
        //
        //     let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, scorer_ids, &score_children, event.r#type, 0)?;
        //
        //     make_fed_event(event, FedEventData::HitByPitch {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         pitcher_id,
        //         pitcher_name: pitcher_name.to_string(),
        //         batter_team_id: sub_event.next_team_id()?,
        //         batter_id,
        //         batter_name: batter_name.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //         scores,
        //         stopped_inhabiting,
        //     })
        // }
        // EventType::BatterSkipped => {
        //     let (player_name, reason) = event.next_parse(parse_batter_skipped)?;
        //     make_fed_event(event, FedEventData::BatterSkipped {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         batter_name: player_name.to_string(),
        //         reason: match reason {
        //             ParsedBatterSkippedReason::Shelled => { BatterSkippedReason::Shelled }
        //             ParsedBatterSkippedReason::Elsewhere => {
        //                 BatterSkippedReason::Elsewhere(get_one_player_id(player_tags, event.r#type)?)
        //             }
        //         },
        //     })
        // }
        // EventType::Party => {
        //     let player_name = event.next_parse(parse_party)?;
        //     let sub_event = get_one_sub_event(event)?;
        //     make_fed_event(event, FedEventData::Party {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //         player_name: player_name.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //         rating_before: get_float_metadata(sub_event, "before")?,
        //         rating_after: get_float_metadata(sub_event, "after")?,
        //     })
        // }
        // EventType::StrikeZapped => {
        //     parse_fixed_description(event, "The Electricity zaps a strike away!",
        //                             FedEventData::StrikeZapped {
        //                                 game: event.game(unscatter, attractor_secret_base)?,
        //                             })
        // }
        // EventType::WeatherChange => { todo!() }
        // EventType::MildPitch => {
        //     let (pitcher_name, pitch_type, runners_advance, scores) = event.next_parse(parse_mild_pitch)?;
        //     let (&pitcher_id, rest_player_ids) = player_tags.split_first()
        //         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //             event_type: event.r#type,
        //             tag_type: "player",
        //             expected_num: 1,
        //             actual_num: player_tags.len(),
        //         })?;
        //
        //     match pitch_type {
        //         MildPitchType::Ball((balls, strikes)) => {
        //             let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_player_ids, &children, event.r#type, 1)?;
        //
        //             make_fed_event(event, FedEventData::MildPitch {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 pitcher_id,
        //                 pitcher_name: pitcher_name.to_string(),
        //                 balls,
        //                 strikes,
        //                 runners_advance,
        //                 scores,
        //                 stopped_inhabiting,
        //             })
        //         }
        //         MildPitchType::Walk(batter_name) => {
        //             let (&batter_id, rest_player_ids) = rest_player_ids.split_first()
        //                 .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //                     event_type: event.r#type,
        //                     tag_type: "player",
        //                     expected_num: 2,
        //                     actual_num: player_tags.len(),
        //                 })?;
        //             let (scores, stopped_inhabiting) = merge_scores_with_ids(scores, rest_player_ids, &children, event.r#type, 2)?;
        //
        //             // I don't believe this should be possible
        //             assert!(!runners_advance, "Runners \"advanced on the pathetic play\" on a mild pitch that was also a walk");
        //             make_fed_event(event, FedEventData::MildPitchWalk {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 pitcher_id,
        //                 pitcher_name: pitcher_name.to_string(),
        //                 batter_id,
        //                 batter_name: batter_name.to_string(),
        //                 scores,
        //                 stopped_inhabiting,
        //             })
        //         }
        //     }
        // }
        // EventType::InningEnd => {
        //     let (inning_num, lost_triple_threat_names) = event.next_parse(parse_inning_end)?;
        //
        //     make_fed_event(event, FedEventData::InningEnd {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         inning_num,
        //         lost_triple_threat: zip_mod_change_events(lost_triple_threat_names, children)?,
        //     })
        // }
        EventType::BigDeal => {
            FedEventData::BeingSpeech {
                being: Being::try_from(event.metadata_i64("being")? as i32)
                    .map_err(|e| FeedParseError::UnknownBeing(e.number))?,
                message: event.consume_description().to_string(),
            }
        }
        // EventType::BlackHole => {
        //     let (scoring_team, victim_team) = event.next_parse(parse_black_hole)?;
        //     assert!(is_known_team_nickname(scoring_team));
        //     assert!(is_known_team_nickname(victim_team));
        //     make_fed_event(event, FedEventData::BlackHole {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         scoring_team_nickname: scoring_team.to_string(),
        //         victim_team_nickname: victim_team.to_string(),
        //     })
        // }
        // EventType::Sun2 => {
        //     let (scoring_team, rays_player) = event.next_parse(parse_sun2)?;
        //     assert!(is_known_team_nickname(scoring_team));
        //
        //     let caught_some_rays = if let Some(player_name) = rays_player {
        //         let child = get_one_sub_event_from_slice(children, event.r#type)?;
        //         Some(PlayerStatChange {
        //             sub_event: child.as_sub_event(),
        //             team_id: child.next_team_id()?,
        //             player_id: get_one_player_id(&child.player_tags, child.r#type)?,
        //             player_name: player_name.to_string(),
        //             rating_before: get_float_metadata(child, "before")?,
        //             rating_after: get_float_metadata(child, "after")?,
        //         })
        //     } else {
        //         None
        //     };
        //
        //     make_fed_event(event, FedEventData::Sun2 {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_nickname: scoring_team.to_string(),
        //         caught_some_rays,
        //     })
        // }
        // EventType::BirdsCircle => {
        //     parse_fixed_description(event, "The Birds circle ... but they don't find what they're looking for.", FedEventData::BirdsCircle {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //     })
        // }
        // EventType::AmbushedByCrows => {
        //     let (pitcher_name, batter_name) = event.next_parse(parse_friend_of_crows)?;
        //     let (pitcher, batter_id) = if let Some(name) = pitcher_name {
        //         let (pitcher_id, batter_id) = get_two_player_ids(event)?;
        //         (Some(PitcherInfo { pitcher_id, pitcher_name: name.to_string() }), batter_id)
        //     } else {
        //         (None, get_one_player_id(player_tags, event.r#type)?)
        //     };
        //
        //     make_fed_event(event, FedEventData::AmbushedByCrows {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         batter_id,
        //         batter_name: batter_name.to_string(),
        //         friend_of_crows: pitcher,
        //     })
        // }
        // EventType::BirdsUnshell => {
        //     let player_name = event.next_parse(parse_birds_unshell)?;
        //
        //     let (pecked_free, superallergy) = get_two_sub_events(event)?;
        //     let team_id = pecked_free.next_team_id()?;
        //     assert_eq!(team_id, superallergy.next_team_id()?);
        //     let player_id = get_one_player_id(&pecked_free.player_tags, pecked_free.r#type)?;
        //     assert_eq!(player_id, get_one_player_id(&superallergy.player_tags, superallergy.r#type)?);
        //
        //     make_fed_event(event, FedEventData::BirdsUnshell {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id,
        //         player_id,
        //         player_name: player_name.to_string(),
        //         pecked_free_event: pecked_free.as_sub_event(),
        //         superallergy_event: superallergy.as_sub_event(),
        //     })
        // }
        // EventType::BecomeTripleThreat => {
        //     let names = event.next_parse(parse_become_triple_threat)?;
        //
        //     let pitchers = zip_eq(children, names)
        //         .map(|(event, pitcher_name)| {
        //             Ok::<_, FeedParseError>(ModChangeSubEventWithNamedPlayer {
        //                 sub_event: event.as_sub_event(),
        //                 team_id: event.next_team_id()?,
        //                 player_id: get_one_player_id(&event.player_tags, event.r#type)?,
        //                 player_name: pitcher_name.to_string(),
        //             })
        //         })
        //         .collect::<Result<_, _>>()?;
        //
        //     make_fed_event(event, FedEventData::BecomeTripleThreat {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         pitchers,
        //     })
        // }
        // EventType::GainFreeRefill => {
        //     let (player_name, roast, ingredient1, ingredient2) = event.next_parse(parse_gain_free_refill)?;
        //     let sub_event = get_one_sub_event(event)?;
        //     let player_id = get_one_player_id(player_tags, event.r#type)?;
        //     // The player ID should match in the sub event
        //     assert_eq!(player_id, get_one_player_id(&sub_event.player_tags, sub_event.r#type)?);
        //     make_fed_event(event, FedEventData::GainFreeRefill {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id,
        //         player_name: player_name.to_string(),
        //         roast: roast.to_string(),
        //         ingredient1: ingredient1.to_string(),
        //         ingredient2: ingredient2.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //         team_id: sub_event.next_team_id()?,
        //     })
        // }
        // EventType::CoffeeBean => {
        //     let (player_name, roast, notes, wired, gained) = event.next_parse(parse_coffee_bean)?;
        //     let sub_event = get_one_sub_event(event)?;
        //     let player_id = get_one_player_id(player_tags, event.r#type)?;
        //     let prev_mod = if sub_event.r#type == EventType::ModChange {
        //         let mod_str = get_str_metadata(sub_event, "to")?;
        //         // Check that the added mod matches what was parsed
        //         assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
        //         Some(get_str_metadata(sub_event, "from")?)
        //     } else {
        //         let mod_str = get_str_metadata(sub_event, "mod")?;
        //         // Check that the added mod matches what was parsed
        //         assert_eq!(mod_str, if wired { "WIRED" } else { "TIRED" });
        //         None
        //     };
        //     // The player ID should match in the sub event
        //     assert_eq!(player_id, get_one_player_id(&sub_event.player_tags, sub_event.r#type)?);
        //     make_fed_event(event, FedEventData::CoffeeBean {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id,
        //         player_name: player_name.to_string(),
        //         roast: roast.to_string(),
        //         notes: notes.to_string(),
        //         which_mod: if wired { CoffeeBeanMod::Wired } else { CoffeeBeanMod::Tired },
        //         has_mod: gained,
        //         sub_event: sub_event.as_sub_event(),
        //         team_id: sub_event.next_team_id()?,
        //         previous: prev_mod.map(|s| s.try_into()
        //             .map_err(|_| FeedParseError::UnexpectedMetadataValue {
        //                 event_type: sub_event.r#type,
        //                 field: "from",
        //                 value: s.to_string(),
        //             })
        //         ).transpose()?,
        //     })
        // }
        // EventType::FeedbackBlocked => {
        //     let (resisted_name, tangled_name) = event.next_parse(parse_feedback_blocked)?;
        //     let (resisted_id, tangled_id) = get_two_player_ids(event)?;
        //     let sub_event = get_one_sub_event(event)?;
        //
        //     make_fed_event(event, FedEventData::FeedbackBlocked {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         resisted_id,
        //         resisted_name: resisted_name.to_string(),
        //         tangled_id,
        //         tangled_team_id: sub_event.next_team_id()?,
        //         tangled_name: tangled_name.to_string(),
        //         tangled_rating_before: get_float_metadata(sub_event, "before")?,
        //         tangled_rating_after: get_float_metadata(sub_event, "after")?,
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::FeedbackSwap => {
        //     let (player1_name, player2_name, position) = event.next_parse(parse_feedback)?;
        //     let sub_event = get_one_sub_event(event)?;
        //
        //     macro_rules! get_player_data {
        //         ($event:ident, $prefix:literal, $expected_name:ident) => {
        //             {
        //                 let team_nickname = get_str_metadata($event, concat!($prefix, "TeamName"))?.to_string();
        //                 assert!(is_known_team_nickname(&team_nickname));
        //                 let player_name = get_str_metadata($event, concat!($prefix, "PlayerName"))?.to_string();
        //                 assert_eq!(player_name, $expected_name);
        //                 FeedbackPlayerData {
        //                     team_id: get_uuid_metadata($event, concat!($prefix, "TeamId"))?,
        //                     team_nickname,
        //                     player_id: get_uuid_metadata($event, concat!($prefix, "PlayerId"))?,
        //                     player_name,
        //                     location: get_int_metadata($event, concat!($prefix, "Location"))?.try_into()?,
        //                 }
        //             }
        //         };
        //     }
        //
        //     make_fed_event(event, FedEventData::Feedback {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         players: (
        //             get_player_data!(sub_event, "a", player1_name),
        //             get_player_data!(sub_event, "b", player2_name),
        //         ),
        //         position_type: position,
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::SuperallergicReaction => { todo!() }
        // EventType::AllergicReaction => {
        //     let player_name = event.next_parse(parse_allergic_reaction)?;
        //     let player_id = get_one_player_id(player_tags, event.r#type)?;
        //     let sub_event = get_one_sub_event(event)?;
        //     assert_eq!(player_id, get_one_player_id(&sub_event.player_tags, sub_event.r#type)?);
        //     make_fed_event(event, FedEventData::AllergicReaction {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         player_id,
        //         player_name: player_name.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //         rating_before: get_float_metadata(sub_event, "before")?,
        //         rating_after: get_float_metadata(sub_event, "after")?,
        //     })
        // }
        // EventType::ReverbBestowsReverberating => {
        //     let player_name = event.next_parse(parse_bestow_reverberating)?;
        //     let player_id = get_one_player_id(player_tags, event.r#type)?;
        //     let sub_event = get_one_sub_event(event)?;
        //     assert_eq!(player_id, get_one_player_id(&sub_event.player_tags, sub_event.r#type)?);
        //     make_fed_event(event, FedEventData::BestowReverberating {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         player_id,
        //         player_name: player_name.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::ReverbRosterShuffle => {
        //     let (team_nickname, reverb_type, gravity_player_names) = event.next_parse(parse_roster_shuffle)?;
        //
        //     let gravity_players = zip_eq(gravity_player_names, player_tags.iter())
        //         .map(|(player_name, &player_id)| PlayerInfo { player_id, player_name: player_name.to_string() })
        //         .collect();
        //
        //     match reverb_type {
        //         ParsedReverbType::Rotation => {
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::Reverb {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 reverb_type: ReverbType::Rotation(sub_event.as_sub_event()),
        //                 gravity_players,
        //             })
        //         }
        //         ParsedReverbType::Lineup => {
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::Reverb {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 reverb_type: ReverbType::Lineup(sub_event.as_sub_event()),
        //                 gravity_players,
        //             })
        //         }
        //         ParsedReverbType::Full => {
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::Reverb {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 reverb_type: ReverbType::Full(sub_event.as_sub_event()),
        //                 gravity_players,
        //             })
        //         }
        //         ParsedReverbType::SeveralPlayers => {
        //             todo!()
        //         }
        //     }
        // }
        // EventType::Blooddrain => {
        //     let (sipper_name, sipped_name, sipped_category) = event.next_parse(parse_blooddrain)?;
        //     let (sipper_id, sipped_id) = get_two_player_ids(event)?;
        //
        //     let (sipped_event, sipper_event) = get_two_sub_events(event)?;
        //
        //     make_fed_event(event, FedEventData::Blooddrain {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         is_siphon: false,
        //         sipper: PlayerStatChange {
        //             team_id: sipper_event.next_team_id()?,
        //             player_id: sipper_id,
        //             player_name: sipper_name.to_string(),
        //             rating_before: get_float_metadata(sipper_event, "before")?,
        //             rating_after: get_float_metadata(sipper_event, "after")?,
        //             sub_event: sipper_event.as_sub_event(),
        //         },
        //         sipped: PlayerStatChange {
        //             team_id: sipped_event.next_team_id()?,
        //             player_id: sipped_id,
        //             player_name: sipped_name.to_string(),
        //             rating_before: get_float_metadata(sipped_event, "before")?,
        //             rating_after: get_float_metadata(sipped_event, "after")?,
        //             sub_event: sipped_event.as_sub_event(),
        //         },
        //         sipped_category,
        //     })
        // }
        // EventType::BlooddrainSiphon => {
        //     let (sipper_name, sipped_name, sipped_category, action) = event.next_parse(parse_blooddrain_siphon)?;
        //     let (sipper_id, sipped_id) = get_two_player_ids(event)?;
        //
        //     match action {
        //         None => {
        //             let (sipped_event, sipper_event) = get_two_sub_events(event)?;
        //
        //             make_fed_event(event, FedEventData::Blooddrain {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 is_siphon: true,
        //                 sipper: PlayerStatChange {
        //                     team_id: sipper_event.next_team_id()?,
        //                     player_id: sipper_id,
        //                     player_name: sipper_name.to_string(),
        //                     rating_before: get_float_metadata(sipper_event, "before")?,
        //                     rating_after: get_float_metadata(sipper_event, "after")?,
        //                     sub_event: sipper_event.as_sub_event(),
        //                 },
        //                 sipped: PlayerStatChange {
        //                     team_id: sipped_event.next_team_id()?,
        //                     player_id: sipped_id,
        //                     player_name: sipped_name.to_string(),
        //                     rating_before: get_float_metadata(sipped_event, "before")?,
        //                     rating_after: get_float_metadata(sipped_event, "after")?,
        //                     sub_event: sipped_event.as_sub_event(),
        //                 },
        //                 sipped_category,
        //             })
        //         }
        //         Some(action) => {
        //             let stat_decrease_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::SpecialBlooddrain {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 sipper_id,
        //                 sipped_team_id: stat_decrease_event.next_team_id()?,
        //                 sipper_name: sipper_name.to_string(),
        //                 sipped_id,
        //                 sipped_name: sipped_name.to_string(),
        //                 sipped_category,
        //                 action: match action {
        //                     ParsedBlooddrainAction::AddBall => { BlooddrainAction::AddBall }
        //                     ParsedBlooddrainAction::RemoveBall => { BlooddrainAction::RemoveBall }
        //                     ParsedBlooddrainAction::AddStrike(name) => { BlooddrainAction::AddStrike(name.map(|s| s.to_string())) }
        //                     ParsedBlooddrainAction::RemoveStrike => { BlooddrainAction::RemoveStrike }
        //                     ParsedBlooddrainAction::AddOut => { BlooddrainAction::AddOut }
        //                     ParsedBlooddrainAction::RemoveOut => { BlooddrainAction::RemoveOut }
        //                 },
        //                 sipped_event: stat_decrease_event.as_sub_event(),
        //                 rating_before: get_float_metadata(stat_decrease_event, "before")?,
        //                 rating_after: get_float_metadata(stat_decrease_event, "after")?,
        //             })
        //         }
        //     }
        // }
        // EventType::BlooddrainBlocked => { todo!() }
        // EventType::Incineration => {
        //     let (victim_name, replacement_name) = event.next_parse(parse_incineration)?;
        //     let (incin_child, enter_hall_child, hatch_child, replace_child) =
        //         children.iter().collect_tuple()
        //             .ok_or_else(|| {
        //                 FeedParseError::MissingChild {
        //                     event_type: event.r#type,
        //                     expected_num_children: 4,
        //                 }
        //             })?;
        //
        //     let team_nickname = get_str_metadata(replace_child, "teamName")?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     make_fed_event(event, FedEventData::Incineration {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: incin_child.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         victim_id: get_one_player_id(&incin_child.player_tags, incin_child.r#type)?,
        //         victim_name: victim_name.to_string(),
        //         replacement_id: get_one_player_id(&hatch_child.player_tags, hatch_child.r#type)?,
        //         replacement_name: replacement_name.to_string(),
        //         location: get_int_metadata(replace_child, "location")?
        //             .try_into()
        //             .map_err(|_| FeedParseError::MissingMetadata {
        //                 event_type: event.r#type,
        //                 field: "location",
        //             })?,
        //         sub_events: (
        //             incin_child.as_sub_event(),
        //             enter_hall_child.as_sub_event(),
        //             hatch_child.as_sub_event(),
        //             replace_child.as_sub_event(),
        //         ),
        //     })
        // }
        // EventType::IncinerationBlocked => {
        //     // For now I only support magmatic, that may have to change
        //     let (player_name, blocked_reason) = event.next_parse(parse_incineration_blocked)?;
        //     match blocked_reason {
        //         IncinerationBlockedReason::Magmatic => {
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::BecameMagmatic {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //                 team_id: sub_event.next_team_id()?,
        //                 mod_add_event: sub_event.as_sub_event(),
        //             })
        //         }
        //         IncinerationBlockedReason::Fireproof => {
        //             make_fed_event(event, FedEventData::FireproofIncineration {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //             })
        //         }
        //     }
        // }
        // EventType::FlagPlanted => {
        //     let (team_nickname, park_name, prefab_name, is_first) = event.next_parse(parse_flag_planted)?;
        //
        //     make_fed_event(event, FedEventData::FlagPlanted {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         ballpark_name: park_name.to_string(),
        //         prefab_name: prefab_name.to_string(),
        //         renovation_id: get_str_metadata(event, "renoId")?.to_string(),
        //         votes: get_int_metadata(event, "votes")?,
        //         is_first,
        //     })
        // }
        // EventType::RenovationBuilt => {
        //     // Funnily enough, fraudulent renos' make-good events have string values for the
        //     // metadata instead of ints.
        //     let is_fraudulent_reno_fix = event.metadata.other
        //         .as_object()
        //         .and_then(|obj| obj.get("votes"))
        //         .ok_or_else(|| FeedParseError::MissingMetadata {
        //             event_type: event.r#type,
        //             field: "votes",
        //         })?
        //         .is_string();
        //
        //     // It may be valuable to parse which reno is built, but there isn't one unified syntax
        //     // so I'm not going to put in the work now. Contributions welcome.
        //     make_fed_event(event, FedEventData::RenovationBuilt {
        //         team_id: event.next_team_id()?,
        //         description: event.description.clone(),
        //         renovation_id: get_str_metadata(event, "renoId")?.to_string(),
        //         renovation_title: get_str_metadata(event, "title")?.to_string(),
        //         votes: if is_fraudulent_reno_fix {
        //             RenovationVotes::Manual(get_str_metadata(event, "votes")?.to_string())
        //         } else {
        //             RenovationVotes::Normal(get_int_metadata(event, "votes")?)
        //         },
        //     })
        // }
        // EventType::LightSwitchToggled => { todo!() }
        // EventType::DecreePassed => {
        //     let decree_title = event.next_parse(parse_decree_passed)?;
        //
        //     make_fed_event(event, FedEventData::DecreePassed {
        //         decree_title: decree_title.to_string(),
        //         metadata: event.metadata.clone(),
        //     })
        // }
        // EventType::BlessingOrGiftWon => {
        //     let blessing_title = event.next_parse(parse_blessing_won)?;
        //
        //     make_fed_event(event, FedEventData::BlessingWon {
        //         team_tags: event.team_tags.clone(),
        //         blessing_title: blessing_title.to_string(),
        //         metadata: event.metadata.clone(),
        //     })
        // }
        // EventType::WillRecieved => {
        //     let will_title = event.next_parse(parse_will_received)?;
        //
        //     make_fed_event(event, FedEventData::WillReceived {
        //         team_id: event.next_team_id()?,
        //         will_title: will_title.to_string(),
        //         metadata: event.metadata.clone(),
        //     })
        // }
        // EventType::FloodingSwept => {
        //     let (parsed_effects, free_refillers, flood_pumps) = event.next_parse(parse_flooding_swept)?;
        //
        //     let mut children_iter = children.iter();
        //     let mut player_tags_iter = player_tags.iter();
        //
        //     let expected_num_tags = parsed_effects.iter()
        //         .filter(|effect| match effect {
        //             ParsedFloodingEffect::Elsewhere(_) => { false }
        //             ParsedFloodingEffect::Flippers(_) => { true }
        //             ParsedFloodingEffect::Ego(_) => { true }
        //         })
        //         .count();
        //
        //     let effects = parsed_effects.iter()
        //         .map(|effect| Ok::<_, FeedParseError>(match effect {
        //             ParsedFloodingEffect::Elsewhere(player_name) => {
        //                 let sub_event = children_iter.next()
        //                     .ok_or_else(|| FeedParseError::MissingChild {
        //                         event_type: event.r#type,
        //                         expected_num_children: (children.len() + 1) as i32, // At least
        //                     })?;
        //
        //                 FloodingSweptEffect::Elsewhere(ModChangeSubEventWithNamedPlayer {
        //                     sub_event: sub_event.as_sub_event(),
        //                     team_id: sub_event.next_team_id()?,
        //                     player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //                     player_name: player_name.to_string(),
        //                 })
        //             }
        //             ParsedFloodingEffect::Flippers(player_name) => {
        //                 FloodingSweptEffect::Flippers(PlayerInfo {
        //                     player_id: *player_tags_iter.next()
        //                         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //                             event_type: event.r#type,
        //                             tag_type: "player",
        //                             expected_num: expected_num_tags,
        //                             actual_num: player_tags.len(),
        //                         })?,
        //                     player_name: player_name.to_string(),
        //                 })
        //             }
        //             ParsedFloodingEffect::Ego(player_name) => {
        //                 FloodingSweptEffect::Ego(PlayerInfo {
        //                     player_id: *player_tags_iter.next()
        //                         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
        //                             event_type: event.r#type,
        //                             tag_type: "player",
        //                             expected_num: expected_num_tags,
        //                             actual_num: player_tags.len(),
        //                         })?,
        //                     player_name: player_name.to_string(),
        //                 })
        //             }
        //         }))
        //         .collect::<Result<Vec<_>, _>>()?;
        //
        //     let free_refills = free_refillers.into_iter()
        //         .map(|refiller_name| {
        //             make_free_refill(event.r#type, &mut children_iter, refiller_name)
        //         })
        //         .collect::<Result<Vec<_>, _>>()?;
        //
        //     if children_iter.next().is_some() {
        //         Err(FeedParseError::ExtraChild {
        //             event_type: event.r#type,
        //             expected_num_children: effects.len() as i32 - expected_num_tags as i32,
        //         })?
        //     }
        //
        //     if player_tags_iter.next().is_some() {
        //         Err(FeedParseError::WrongNumberOfTags {
        //             event_type: event.r#type,
        //             tag_type: "player",
        //             expected_num: expected_num_tags,
        //             actual_num: player_tags.len(),
        //         })?
        //     }
        //
        //     make_fed_event(event, FedEventData::FloodingSwept {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         effects,
        //         free_refills,
        //         flood_pumps,
        //     })
        // }
        // EventType::SalmonSwim => {
        //     let (inning_num, parsed_runs_lost) = event.next_parse(parse_salmon)?;
        //
        //     make_fed_event(event, FedEventData::SalmonSwim {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         inning_num,
        //         run_losses: match parsed_runs_lost {
        //             ParsedSalmonRunsLost::None => { RunLossesFromSalmon::None }
        //             ParsedSalmonRunsLost::OneTeam(ParsedTeamRunsLost { runs, name }) => {
        //                 RunLossesFromSalmon::OneTeam(TeamRunsLost { runs_lost: runs, team_name: name.to_string() })
        //             }
        //             ParsedSalmonRunsLost::BothTeams((a, b)) => {
        //                 RunLossesFromSalmon::BothTeams((
        //                     TeamRunsLost { runs_lost: a.runs, team_name: a.name.to_string() },
        //                     TeamRunsLost { runs_lost: b.runs, team_name: b.name.to_string() },
        //                 ))
        //             }
        //         },
        //     })
        // }
        // EventType::PolarityShift => { todo!() }
        // EventType::EnterSecretBase => {
        //     let player_name = event.next_parse(parse_terminated(" enters the Secret Base..."))?;
        //
        //     make_fed_event(event, FedEventData::EnterSecretBase {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //     })
        // }
        // EventType::ExitSecretBase => {
        //     let player_name = event.next_parse(parse_terminated(" exits the Secret Base to Second Base!"))?;
        //
        //     make_fed_event(event, FedEventData::ExitSecretBase {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //     })
        // }
        // EventType::ConsumersAttack => {
        //     let (player_name, item_breaks, scattered) = event.next_parse(parse_consumer_attack)?;
        //
        //     let (team_id, effect, sensed_something_fishy) = if item_breaks.is_some() {
        //         let break_child= get_one_sub_event_from_slice(children, event.r#type)?;
        //
        //         let item_breaks = ItemDamage {
        //             item_id: get_uuid_metadata(break_child, "itemId")?,
        //             item_name: get_str_metadata(break_child, "itemName")?.to_string(),
        //             item_mods: vec![],
        //             durability: get_int_metadata(break_child, "itemDurability")?,
        //             player_item_rating_before: get_float_metadata(break_child, "playerItemRatingBefore")?,
        //             player_item_rating_after: get_float_metadata(break_child, "playerItemRatingAfter")?,
        //             player_rating: get_float_metadata(break_child, "playerRating")?,
        //             sub_event: break_child.as_sub_event(),
        //         };
        //
        //         let team_id = break_child.next_team_id()?;
        //         (team_id, ConsumerAttackEffect::DefendedWithItem(item_breaks), None)
        //     } else {
        //         // I'm hoping that detectives only sense something fishy if the attack hit
        //         // TODO: If this is true, move the something fishy inside the effect
        //         let (sub_event, sensed_something_fishy) = if children.len() == 2 {
        //             // Then a detective sensed something fishy
        //             let (chomp_event, fishy_event) = get_two_sub_events(event)?;
        //             let detective_name = run_parser(&fishy_event.description, fishy_event.r#type, parse_terminated(" sensed something fishy."))?;
        //             let detective_activity = DetectiveActivity {
        //                 detective_id: get_one_player_id(&fishy_event.player_tags, fishy_event.r#type)?,
        //                 detective_name: detective_name.to_string(),
        //                 sub_event: fishy_event.as_sub_event(),
        //             };
        //
        //             (chomp_event, Some(detective_activity))
        //         } else {
        //             let sub_event = get_one_sub_event(event)?;
        //             (sub_event, None)
        //         };
        //
        //         let team_id = sub_event.next_team_id()?;
        //         (team_id, ConsumerAttackEffect::Chomp {
        //             rating_before: get_float_metadata(sub_event, "before")?,
        //             rating_after: get_float_metadata(sub_event, "after")?,
        //             sub_event: sub_event.as_sub_event(),
        //         }, sensed_something_fishy)
        //     };
        //
        //     make_fed_event(event, FedEventData::ConsumerAttack {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //         effect,
        //         sensed_something_fishy,
        //         scattered,
        //     })
        // }
        // EventType::EchoChamber => {
        //     let (player_name, which_mod) = event.next_parse(parse_echo_chamber)?;
        //
        //     let child = get_one_sub_event_from_slice(children, event.r#type)?;
        //     make_fed_event(event, FedEventData::EchoChamber {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: child.next_team_id()?,
        //         player_id: get_one_player_id(&child.player_tags, child.r#type)?,
        //         player_name: player_name.to_string(),
        //         which_mod,
        //         sub_event: child.as_sub_event(),
        //     })
        // }
        // EventType::GrindRail => {
        //     let (player_name, first_trick, success) = event.next_parse(parse_grind_rail)?;
        //
        //     fn trick_from_parsed(parsed: ParsedGrindRailTrick) -> GrindRailTrick {
        //         GrindRailTrick {
        //             trick_name: parsed.name.to_string(),
        //             points: parsed.score,
        //         }
        //     }
        //
        //     make_fed_event(event, FedEventData::GrindRail {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //         first_trick: trick_from_parsed(first_trick),
        //         success: match success {
        //             ParsedGrindRailSuccess::Safe(trick) => {
        //                 GrindRailSuccess::Safe(trick_from_parsed(trick))
        //             }
        //             ParsedGrindRailSuccess::TaggedOut(trick) => {
        //                 GrindRailSuccess::TaggedOut(trick_from_parsed(trick))
        //             }
        //             ParsedGrindRailSuccess::Bailed => {
        //                 GrindRailSuccess::Bailed
        //             }
        //         },
        //     })
        // }
        // EventType::TunnelsUsed => { todo!() }
        // EventType::PeanutMister => {
        //     let (player_name, cured_superallergy) = event.next_parse(parse_peanut_mister)?;
        //
        //     let superallergy = if cured_superallergy {
        //         let sub_event = get_one_sub_event_from_slice(children, event.r#type)?;
        //         Some(ModChangeSubEvent {
        //             sub_event: sub_event.as_sub_event(),
        //             team_id: sub_event.next_team_id()?,
        //         })
        //     } else {
        //         None
        //     };
        //
        //     make_fed_event(event, FedEventData::PeanutMister {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //         superallergy,
        //     })
        // }
        // EventType::PeanutFlavorText => {
        //     make_fed_event(event, FedEventData::PeanutFlavorText {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         message: event.description.clone(),
        //     })
        // }
        // EventType::TasteTheInfinite => {
        //     let (sheller_name, shellee_name) = event.next_parse(parse_taste_the_infinite)?;
        //     let (sheller_id, shellee_id) = get_two_player_ids(event)?;
        //
        //     let sub_event = get_one_sub_event(event)?;
        //     make_fed_event(event, FedEventData::TasteTheInfinite {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         sheller_id,
        //         sheller_name: sheller_name.to_string(),
        //         shellee_team_id: sub_event.next_team_id()?,
        //         shellee_id,
        //         shellee_name: shellee_name.to_string(),
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::EventHorizonActivation => { todo!() }
        // EventType::EventHorizonAwaits => { todo!() }
        // EventType::SolarPanelsAwait => {
        //     parse_fixed_description(event, "The Solar Panels are angled toward Sun 2.", FedEventData::SolarPanelsAwait {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //     })
        // }
        // EventType::SolarPanelsActivation => {
        //     let (num_runs, team_nickname) = event.next_parse(parse_solar_panels)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //
        //     make_fed_event(event, FedEventData::SolarPanelsActivate {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         num_runs,
        //         team_nickname: team_nickname.to_string(),
        //     })
        // }
        // EventType::TarotReading => {
        //     make_fed_event(event, FedEventData::TarotReading {
        //         description: event.description.clone(),
        //         metadata: event.metadata.other.clone(),
        //         player_tags: player_tags.into(),
        //         team_tags: event.team_tags.clone(),
        //     })
        // }
        // EventType::EmergencyAlert => {
        //     make_fed_event(event, FedEventData::EmergencyAlert {
        //         message: event.description.clone(),
        //         team_tags: event.team_tags.clone(),
        //     })
        // }
        // EventType::ReturnFromElsewhere => {
        //     match event.next_parse(parse_return_from_elsewhere)? {
        //         ParsedReturnFromElsewhere::Normal((player_name, after_days)) => {
        //             let (return_sub_event, scattered) = if children.len() == 2 {
        //                 let (scattered_sub_event, return_sub_event) = get_two_sub_events(event)?;
        //                 let scattered_name = run_parser(&scattered_sub_event.description, scattered_sub_event.r#type, parse_terminated(" was Scattered..."))?;
        //
        //                 let scattered = Scattered {
        //                     scattered_name: scattered_name.to_string(),
        //                     sub_event: scattered_sub_event.as_sub_event(),
        //                 };
        //                 (return_sub_event, Some(scattered))
        //             } else {
        //                 (get_one_sub_event(event)?, None)
        //             };
        //
        //             make_fed_event(event, FedEventData::ReturnFromElsewhere {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 player_name: player_name.to_string(),
        //                 flavor: ReturnFromElsewhereFlavor::Full {
        //                     team_id: return_sub_event.next_team_id()?,
        //                     player_id: get_one_player_id(&return_sub_event.player_tags, return_sub_event.r#type)?,
        //                     sub_event: return_sub_event.as_sub_event(),
        //                     number_of_days: after_days,
        //                     scattered,
        //                 },
        //             })
        //         }
        //         ParsedReturnFromElsewhere::Short(player_name) => {
        //             if children.is_empty() {
        //                 make_fed_event(event, FedEventData::ReturnFromElsewhere {
        //                     game: event.game(unscatter, attractor_secret_base)?,
        //                     player_name: player_name.to_string(),
        //                     flavor: ReturnFromElsewhereFlavor::False,
        //                 })
        //             } else {
        //                 let return_sub_event = get_one_sub_event_from_slice(children, event.r#type)?;
        //                 make_fed_event(event, FedEventData::ReturnFromElsewhere {
        //                     game: event.game(unscatter, attractor_secret_base)?,
        //                     player_name: player_name.to_string(),
        //                     flavor: ReturnFromElsewhereFlavor::Short {
        //                         team_id: return_sub_event.next_team_id()?,
        //                         player_id: get_one_player_id(&return_sub_event.player_tags, return_sub_event.r#type)?,
        //                         sub_event: return_sub_event.as_sub_event(),
        //                     },
        //                 })
        //             }
        //         }
        //     }
        // }
        // EventType::OverUnder => {
        //     let (player_name, on) = event.next_parse(parse_under_over_over_under("Over Under"))?;
        //
        //     let sub_event = get_one_sub_event(event)?;
        //     make_fed_event(event, FedEventData::OverUnder {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //         player_name: player_name.to_string(),
        //         on,
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::UnderOver => {
        //     let (player_name, on) = event.next_parse(parse_under_over_over_under("Under Over"))?;
        //
        //     let sub_event = get_one_sub_event(event)?;
        //     make_fed_event(event, FedEventData::UnderOver {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
        //         player_name: player_name.to_string(),
        //         on,
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::Undersea => {
        //     let team_name = event.next_parse(parse_undersea)?;
        //     assert!(is_known_team_name(team_name));
        //
        //     let mod_add_event = get_one_sub_event(event)?;
        //
        //     make_fed_event(event, FedEventData::Undersea {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: mod_add_event.next_team_id()?,
        //         team_name: team_name.to_string(),
        //         sub_event: mod_add_event.as_sub_event(),
        //     })
        // }
        // EventType::Homebody => {
        //     let players = event.next_parse(parse_homebody)?;
        //
        //     let homebodies = zip_eq(players, children)
        //         .map(|((player_name, is_overperforming), mod_add_event)| {
        //             Ok::<_, FeedParseError>(TogglePerforming {
        //                 player_id: get_one_player_id(&mod_add_event.player_tags, mod_add_event.r#type)?,
        //                 team_id: mod_add_event.next_team_id()?,
        //                 player_name: player_name.to_string(),
        //                 is_overperforming,
        //                 is_first_proc: mod_add_event.r#type == EventType::AddedModFromOtherMod,
        //                 sub_event: mod_add_event.as_sub_event(),
        //             })
        //         })
        //         .collect::<Result<_, _>>()?;
        //
        //     make_fed_event(event, FedEventData::HomebodyGameStart {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         homebodies,
        //     })
        // }
        EventType::Superyummy => {
            let (player_name, peanuts_present) = event.next_parse(parse_superyummy)?;

            let expected_types = [EventType::AddedModFromOtherMod, EventType::ChangedModFromOtherMod];
            if let Some(mut mod_add_event) = event.next_child_if_any(&expected_types, |child| {
                expected_types.iter().any(|t| t == &child.event_type)
            })? {
                FedEventData::SuperyummyGameStart {
                    game: event.game(unscatter, attractor_secret_base)?,
                    toggle: TogglePerforming {
                        player_name: player_name.to_string(),
                        is_overperforming: peanuts_present,
                        is_first_proc: mod_add_event.event_type == EventType::AddedModFromOtherMod,
                        sub_event: mod_add_event.as_sub_event(),
                        player_id: mod_add_event.next_player_id()?,
                        team_id: mod_add_event.next_team_id()?,
                    },
                }
            } else {
                // Then this must have come from an Echoed Superyummy
                FedEventData::EchoedSuperyummyGameStart {
                    game: event.game(unscatter, attractor_secret_base)?,
                    player_name: player_name.to_string(),
                    peanuts_present,
                }
            }
        }
        // EventType::Perk => {
        //     let player_names = event.next_parse(parse_perk_up)?;
        //
        //     make_fed_event(event, FedEventData::PerkUp {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         players: children.iter()
        //             .zip(player_names)
        //             .map(|(mod_add_event, player_name)| {
        //                 assert_eq!(format!("{player_name} Perks up."), mod_add_event.description);
        //                 Ok::<_, FeedParseError>(ModChangeSubEventWithNamedPlayer {
        //                     player_name: player_name.to_string(),
        //                     sub_event: mod_add_event.as_sub_event(),
        //                     player_id: get_one_player_id(&mod_add_event.player_tags, mod_add_event.r#type)?,
        //                     team_id: mod_add_event.next_team_id()?,
        //                 })
        //             })
        //             .collect::<Result<_, _>>()?,
        //     })
        // }
        // EventType::Earlbird => {
        //     match event.next_parse(parse_earlbird)? {
        //         EarlbirdsChange::Added(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::EarlbirdsAdded {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 sub_event: sub_event.as_sub_event(),
        //             })
        //         }
        //         EarlbirdsChange::Removed => {
        //             let sub_event = get_one_sub_event(event)?;
        //             make_fed_event(event, FedEventData::EarlbirdsRemoved {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.next_team_id()?,
        //                 sub_event: sub_event.as_sub_event(),
        //             })
        //         }
        //     }
        // }
        // EventType::LateToTheParty => {
        //     match event.next_parse(parse_late_to_the_party)? {
        //         LateToThePartyChange::Added(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //
        //             let sub_event = get_one_or_zero_sub_events(event)?;
        //             make_fed_event(event, FedEventData::LateToThePartyAdded {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_id: sub_event.map(|e| e.next_team_id()).transpose()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 sub_event: sub_event.map(SubEvent::from_event),
        //             })
        //         }
        //         LateToThePartyChange::Removed(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //
        //             make_fed_event(event, FedEventData::LateToThePartyRemoved {
        //                 game: event.game(unscatter, attractor_secret_base)?,
        //                 team_nickname: team_nickname.to_string(),
        //             })
        //         }
        //     }
        // }
        // EventType::ShameDonor => { todo!() }
        // EventType::AddedMod => {
        //     if TAROT_EVENTS.iter().any(|uuid| uuid == &event.id) {
        //         // Then it's a tarot event and we can forget parsing. Thankfully
        //         make_fed_event(event, FedEventData::TarotReadingAddedMod {
        //             team_id: event.next_team_id()?,
        //             player_id: get_one_or_zero_player_ids(event)?,
        //             description: event.description.clone(),
        //             r#mod: get_str_metadata(event, "mod")?.to_string(),
        //             mod_duration: get_int_metadata(event, "type")?,
        //         })
        //     } else {
        //         match event.next_parse(parse_added_mod)? {
        //             ParsedAddedMod::EnteredPartyTime(team_nickname) => {
        //                 assert!(is_known_team_nickname(team_nickname));
        //                 make_fed_event(event, FedEventData::TeamEnteredPartyTime {
        //                     team_id: event.next_team_id()?,
        //                     team_nickname: team_nickname.to_string(),
        //                 })
        //             }
        //             ParsedAddedMod::GainFreeWill(team_nickname) => {
        //                 assert!(is_known_team_nickname(team_nickname));
        //                 make_fed_event(event, FedEventData::TeamGainedFreeWill {
        //                     team_id: event.next_team_id()?,
        //                     team_nickname: team_nickname.to_string(),
        //                 })
        //             }
        //             ParsedAddedMod::MVP(player_name) => {
        //                 make_fed_event(event, FedEventData::PlayerNamedMvp {
        //                     team_id: event.next_team_id()?,
        //                     player_id: get_one_player_id(player_tags, event.r#type)?,
        //                     player_name: player_name.to_string(),
        //                     level: 1,
        //                 })
        //             }
        //         }
        //     }
        // }
        // EventType::RemovedMod => {
        //     match event.next_parse(parse_removed_mod)? {
        //         ParsedRemovedMod::TeamRemovedFromPartyTimeForPostseason(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //             make_fed_event(event, FedEventData::TeamLeftPartyTimeForPostseason {
        //                 team_id: event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //             })
        //         }
        //         ParsedRemovedMod::TeamUsedFreeWill(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //             make_fed_event(event, FedEventData::TeamUsedFreeWill {
        //                 team_id: event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //             })
        //         }
        //         ParsedRemovedMod::PlayerLostMod((player_name, mod_name)) => {
        //             make_fed_event(event, FedEventData::PlayerLostMod {
        //                 team_id: event.next_team_id()?,
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //                 r#mod: get_str_metadata(event, "mod")?.to_string(),
        //                 mod_name: mod_name.to_string(),
        //             })
        //         }
        //         ParsedRemovedMod::InvestigationConcluded(stadium_name) => {
        //             make_fed_event(event, FedEventData::InvestigationConcluded {
        //                 team_id: event.next_team_id()?,
        //                 stadium_name: stadium_name.to_string(),
        //             })
        //         }
        //     }
        // }
        // EventType::ModExpires => {
        //     if player_tags.is_empty() {
        //         let (team_nickname, mod_duration) = event.next_parse(parse_team_mod_expires)?;
        //         assert!(is_known_team_nickname(team_nickname));
        //         let mods = get_str_vec_metadata(event, "mods")?;
        //         make_fed_event(event, FedEventData::TeamModExpires {
        //             team_id: event.next_team_id()?,
        //             team_nickname: team_nickname.to_string(),
        //             mods: mods.into_iter().map(String::from).collect(),
        //             mod_duration,
        //         })
        //     } else {
        //         let (player_name, mod_duration) = event.next_parse(parse_player_mod_expires)?;
        //         let mods = get_str_vec_metadata(event, "mods")?;
        //         make_fed_event(event, FedEventData::PlayerModExpires {
        //             team_id: event.next_team_id()?,
        //             player_id: get_one_player_id(player_tags, event.r#type)?,
        //             player_name: player_name.to_string(),
        //             mods: mods.into_iter().map(String::from).collect(),
        //             mod_duration,
        //         })
        //     }
        // }
        // EventType::PlayerAddedToTeam => {
        //     match event.next_parse(parse_player_added_to_team)? {
        //         ParsedPlayerAddedToTeam::PostseasonBirth(team_nickname) => {
        //             make_fed_event(event, FedEventData::PostseasonBirth {
        //                 team_id: event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: get_str_metadata(event, "playerName")?.to_string(),
        //                 location: get_int_metadata(event, "location")?
        //                     .try_into()
        //                     .map_err(|_| FeedParseError::MissingMetadata {
        //                         event_type: event.r#type,
        //                         field: "location",
        //                     })?,
        //             })
        //         }
        //         ParsedPlayerAddedToTeam::Localized { player_name, team_nickname, .. } => {
        //             // TODO Check location from parsing against location from metadata
        //             make_fed_event(event, FedEventData::PlayerLocalized {
        //                 team_id: event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //                 location: get_int_metadata(event, "location")?
        //                     .try_into()
        //                     .map_err(|_| FeedParseError::MissingMetadata {
        //                         event_type: event.r#type,
        //                         field: "location",
        //                     })?,
        //             })
        //         }
        //     }
        // }
        // EventType::PlayerReplacedByNecromancy => { todo!() }
        // EventType::PlayerReplacesReturned => {
        //     let team_nickname = event.next_parse(parse_player_replaces_returned)?;
        //
        //     make_fed_event(event, FedEventData::ReplaceReturnedPlayerFromShadows {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         promoted_player_id: get_uuid_metadata(event, "promotePlayerId")?,
        //         promoted_player_name: get_str_metadata(event, "promotePlayerName")?.to_string(),
        //         promoted_location: get_int_metadata(event, "promoteLocation")?
        //             .try_into()
        //             .map_err(|_| FeedParseError::MissingMetadata {
        //                 event_type: event.r#type,
        //                 field: "promoteLocation",
        //             })?,
        //         removed_player_id: get_uuid_metadata(event, "removePlayerId")?,
        //         removed_player_name: get_str_metadata(event, "removePlayerName")?.to_string(),
        //         removed_location: get_int_metadata(event, "removeLocation")?
        //             .try_into()
        //             .map_err(|_| FeedParseError::MissingMetadata {
        //                 event_type: event.r#type,
        //                 field: "removeLocation",
        //             })?,
        //     })
        // }
        // EventType::PlayerRemovedFromTeam => { todo!() }
        // EventType::PlayerTraded => { todo!() }
        // EventType::PlayerSwap => { todo!() }
        // EventType::PlayerMoved => {
        //     match event.next_parse(parse_player_moved)? {
        //         ParsedPlayerMoved::ReturnFromInvestigation((_player_name, emptyhanded)) => {
        //             make_fed_event(event, FedEventData::ReturnFromInvestigation {
        //                 player_id: get_uuid_metadata(event, "playerId")?,
        //                 player_name: get_str_metadata(event, "playerName")?.to_string(),
        //                 previous_team_id: get_uuid_metadata(event, "sendTeamId")?,
        //                 previous_team_name: get_str_metadata(event, "sendTeamName")?.to_string(),
        //                 new_location: get_int_metadata(event, "receiveLocation")?
        //                     .try_into()
        //                     .map_err(|_| FeedParseError::MissingMetadata {
        //                         event_type: event.r#type,
        //                         field: "receiveLocation",
        //                     })?,
        //                 new_team_id: get_uuid_metadata(event, "receiveTeamId")?,
        //                 new_team_name: get_str_metadata(event, "receiveTeamName")?.to_string(),
        //                 emptyhanded,
        //             })
        //         }
        //         ParsedPlayerMoved::Roamin(_player_name) => {
        //             make_fed_event(event, FedEventData::Roam {
        //                 player_id: get_uuid_metadata(event, "playerId")?,
        //                 player_name: get_str_metadata(event, "playerName")?.to_string(),
        //                 location: get_int_metadata(event, "location")?
        //                     .try_into()
        //                     .map_err(|_| FeedParseError::MissingMetadata {
        //                         event_type: event.r#type,
        //                         field: "receiveLocation",
        //                     })?,
        //                 previous_team_id: get_uuid_metadata(event, "sendTeamId")?,
        //                 previous_team_name: get_str_metadata(event, "sendTeamName")?.to_string(),
        //                 new_team_id: get_uuid_metadata(event, "receiveTeamId")?,
        //                 new_team_name: get_str_metadata(event, "receiveTeamName")?.to_string(),
        //             })
        //         }
        //     }
        // }
        // EventType::PlayerBornFromIncineration => { todo!() }
        // EventType::PlayerStatIncrease => {
        //     match event.next_parse(parse_player_stat_increase)? {
        //         ParsedPlayerStatIncrease::PlayerBoosted(player_name) => {
        //             make_fed_event(event, FedEventData::PlayerBoosted {
        //                 team_id: event.next_team_id()?,
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //                 rating_before: get_float_metadata(event, "before")?,
        //                 rating_after: get_float_metadata(event, "after")?,
        //             })
        //         }
        //         ParsedPlayerStatIncrease::BottomDwellers(team_nickname) => {
        //             assert!(is_known_team_nickname(team_nickname));
        //             make_fed_event(event, FedEventData::BottomDwellers {
        //                 team_id: event.next_team_id()?,
        //                 team_nickname: team_nickname.to_string(),
        //                 rating_before: get_float_metadata(event, "before")?,
        //                 rating_after: get_float_metadata(event, "after")?,
        //             })
        //         }
        //     }
        // }
        // EventType::PlayerStatDecrease => { todo!() }
        // EventType::PlayerStatReroll => { todo!() }
        // EventType::PlayerStatDecreaseFromSuperallergic => { todo!() }
        // EventType::PlayerMoveFailedForce => { todo!() }
        // EventType::EnterHallOfFlame => {
        //     // In Beta, this event type is only top-level for return-to-hall events. That was no
        //     // longer true in Short Circuits.
        //     assert_eq!(event.sim, "thisidisstaticyo");
        //
        //     let player_name = event.next_parse(parse_terminated(" entered the Hall of Flame."))?;
        //
        //     make_fed_event(event, FedEventData::PlayerCalledBackToHall {
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //     })
        // }
        // EventType::ExitHallOfFlame => { todo!() }
        // EventType::PlayerGainedItem => { todo!() }
        // EventType::PlayerLostItem => { todo!() }
        // EventType::ReverbFullShuffle => { todo!() }
        // EventType::ReverbLineupShuffle => { todo!() }
        // EventType::ReverbRotationShuffle => { todo!() }
        // EventType::PlayerHatched => {
        //     // For now this only has the breach events, it will need to be updated for s24
        //     let player_name = event.next_parse(parse_player_hatched)?;
        //
        //     make_fed_event(event, FedEventData::PlayerHatched {
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //     })
        // }
        // EventType::PlayerEvolves => { todo!() }
        // EventType::TeamDivisionMove => {
        //     // For now this only has the breach events, it will need to be updated for s24
        //     let (team_nickname, division_name) = event.next_parse(parse_team_division_move)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     assert_eq!(team_nickname, get_str_metadata(event, "teamName")?);
        //     assert_eq!(division_name, get_str_metadata(event, "divisionName")?);
        //     let team_id = event.next_team_id()?;
        //     assert_eq!(team_id, get_uuid_metadata(event, "teamId")?);
        //
        //     make_fed_event(event, FedEventData::TeamJoinedILB {
        //         team_id,
        //         team_nickname: team_nickname.to_string(),
        //         division_id: get_uuid_metadata(event, "divisionId")?,
        //         division_name: division_name.to_string(),
        //     })
        // }
        // EventType::PlayerDivisionMove => {
        //     match event.next_parse(parse_player_division_move)? {
        //         ParsedPlayerDivisionMove::JoinedIlb(player_name) => {
        //             make_fed_event(event, FedEventData::PlayerJoinedILB {
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //             })
        //         }
        //         ParsedPlayerDivisionMove::PulledThroughRift(player_name) => {
        //             make_fed_event(event, FedEventData::PlayerPulledThroughRift {
        //                 player_id: get_one_player_id(player_tags, event.r#type)?,
        //                 player_name: player_name.to_string(),
        //             })
        //         }
        //     }
        // }
        // EventType::TeamWonInternetSeries => {
        //     let (team_nickname, season_num) = event.next_parse(parse_team_won_internet_series)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     assert_eq!(season_num, event.season + 1);
        //
        //     make_fed_event(event, FedEventData::TeamWonInternetSeries {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         championships: get_int_metadata(event, "championships")?,
        //     })
        // }
        // EventType::EarnedPostseasonSlot => {
        //     let (team_nickname, season_num) = event.next_parse(parse_earned_postseason_slot)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     assert_eq!(season_num, event.season + 1);
        //
        //     make_fed_event(event, FedEventData::EarnedPostseasonSlot {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //     })
        // }
        // EventType::FinalStandings => {
        //     let (team_nickname, place, division_name) = event.next_parse(parse_final_standings)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //
        //     make_fed_event(event, FedEventData::FinalStandings {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         place,
        //         division_name: division_name.to_string(),
        //     })
        // }
        // EventType::ModChange => {
        //     // This is only a top-level event for MVPs
        //     let (player_name, level) = event.next_parse(parse_repeat_mvp)?;
        //
        //     make_fed_event(event, FedEventData::PlayerNamedMvp {
        //         team_id: event.next_team_id()?,
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //         level,
        //     })
        // }
        // EventType::PlayerAlternated => { todo!() }
        // EventType::AddedModFromOtherMod => { todo!() }
        // EventType::ChangedModFromOtherMod => { todo!() }
        // EventType::NecromancyOrPlunderNarration => { todo!() }
        // EventType::PlayerPermittedToStay => {
        //     let player_name = event.next_parse(parse_terminated(" has been permitted to stay."))?;
        //
        //     make_fed_event(event, FedEventData::PlayerPermittedToStay {
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         player_name: player_name.to_string(),
        //     })
        // }
        // EventType::DecreeNarration => { todo!() }
        // EventType::WillResults => { todo!() }
        // EventType::TeamStatAdjustment => { todo!() }
        // EventType::TeamWasShamed => {
        //     let (shaming_team, shamed_team) = event.next_parse(parse_team_was_shamed)?;
        //     assert!(is_known_team_nickname(shaming_team));
        //     assert!(is_known_team_nickname(shamed_team));
        //
        //     make_fed_event(event, FedEventData::TeamWasShamed {
        //         shamed_team_id: event.next_team_id()?,
        //         shaming_team_nickname: shaming_team.to_string(),
        //         shamed_team_nickname: shamed_team.to_string(),
        //         total_shames: get_int_metadata(event, "totalShames")?,
        //         total_shamings: get_int_metadata(event, "totalShamings")?,
        //     })
        // }
        // EventType::TeamDidShame => {
        //     let (shaming_team, shamed_team) = event.next_parse(parse_team_did_shame)?;
        //     assert!(is_known_team_nickname(shaming_team));
        //     assert!(is_known_team_nickname(shamed_team));
        //
        //     make_fed_event(event, FedEventData::TeamDidShame {
        //         shaming_team_id: event.next_team_id()?,
        //         shaming_team_nickname: shaming_team.to_string(),
        //         shamed_team_nickname: shamed_team.to_string(),
        //         total_shames: get_int_metadata(event, "totalShames")?,
        //         total_shamings: get_int_metadata(event, "totalShamings")?,
        //     })
        // }
        // EventType::Echo => {
        //     let (echoer_name, echoee_name) = event.next_parse(parse_echo)?;
        //
        //     // I would prefer to use try_group_by but it doesn't exist and I don't feel like
        //     // writing it
        //     let child_groups = {
        //         let mut child_groups = Vec::new();
        //         let mut remove_mods_event = None;
        //         for child in children {
        //             if child.r#type == EventType::RemovedModsFromAnotherMod {
        //                 if remove_mods_event.is_some() {
        //                     Err(FeedParseError::UnexpectedChildPattern {
        //                         event_type: event.r#type,
        //                         err: format!("Encountered two {:?} events in a row",
        //                                      EventType::RemovedModsFromAnotherMod),
        //                     })?;
        //                 } else {
        //                     remove_mods_event = Some(child);
        //                 }
        //             } else if child.r#type == EventType::AddedModsFromAnotherMod {
        //                 child_groups.push((remove_mods_event.take(), child));
        //             } else {
        //                 Err(FeedParseError::UnexpectedChildType {
        //                     event_type: event.r#type,
        //                     child_event_type: child.r#type,
        //                 })?;
        //             }
        //         };
        //
        //         child_groups
        //     };
        //
        //     let (main_echo_event, sub_echo_events) = child_groups.split_first()
        //         .ok_or_else(|| FeedParseError::MissingChild {
        //             event_type: event.r#type,
        //             expected_num_children: 1, // At least
        //         })?;
        //
        //     let parse_str = format!("'s Echoed an Echo from {echoer_name}!");
        //     let sub_echos = sub_echo_events.iter()
        //         .map(move |event| {
        //             let echoer_name = run_parser(&event.1.description, event.1.r#type, parse_terminated(&parse_str))?;
        //             make_echo(echoer_name, event)
        //         })
        //         .collect::<Result<_, _>>()?;
        //
        //     make_fed_event(event, FedEventData::Echo {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         echoee_name: echoee_name.to_string(),
        //         primary_echo: make_echo(echoer_name, main_echo_event)?,
        //         receiver_echos: sub_echos,
        //     })
        // }
        // EventType::EchoIntoStatic => {
        //     let (echoer_name, echoee_name) = event.next_parse(parse_echo_into_static)?;
        //
        //     let (echoer_removed, echoee_removed, echoer_mod_change, echoee_mod_change) = children.iter()
        //         .collect_tuple()
        //         .ok_or_else(|| FeedParseError::MissingChild {
        //             event_type: event.r#type,
        //             expected_num_children: 4,
        //         })?;
        //
        //     let make_echo_into_static = |name: &str, removed_event: &EventuallyEvent, mod_change_event: &EventuallyEvent| {
        //         let nickname = get_str_metadata(removed_event, "teamName")?;
        //         assert!(is_known_team_nickname(nickname));
        //         Ok::<_, FeedParseError>(EchoIntoStatic {
        //             team_id: get_uuid_metadata(removed_event, "teamId")?,
        //             team_nickname: nickname.to_string(),
        //             player_id: get_uuid_metadata(removed_event, "playerId")?,
        //             player_name: name.to_string(),
        //             removed_from_team_sub_event: removed_event.as_sub_event(),
        //             mod_changed_sub_event: mod_change_event.as_sub_event(),
        //         })
        //     };
        //
        //     make_fed_event(event, FedEventData::EchoIntoStatic {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         echoer: make_echo_into_static(echoer_name, echoer_removed, echoer_mod_change)?,
        //         echoee: make_echo_into_static(echoee_name, echoee_removed, echoee_mod_change)?,
        //     })
        // }
        // EventType::RemovedModsFromAnotherMod => { todo!() }
        // EventType::AddedModsFromAnotherMod => { todo!() }
        // EventType::Psychoacoustics => {
        //     // For some reason the description on the main event is empty and the description is
        //     // only on the child event
        //     let child = get_one_sub_event_from_slice(children, event.r#type)?;
        //     let (stadium_name, mod_name, team_nickname) = run_parser(&child.description, child.r#type, parse_psychoacoustics)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     make_fed_event(event, FedEventData::Psychoacoustics {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         stadium_name: stadium_name.to_string(),
        //         team_id: child.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         mod_name: mod_name.to_string(),
        //         mod_id: get_str_metadata(child, "mod")?.to_string(),
        //         sub_event: child.as_sub_event(),
        //     })
        // }
        // EventType::EchoReciever => {
        //     let (echoer_name, echoee_name) = event.next_parse(parse_echo_receiver)?;
        //
        //     let child = get_one_sub_event_from_slice(children, event.r#type)?;
        //     make_fed_event(event, FedEventData::EchoReceiver {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         echoer_name: echoer_name.to_string(),
        //         echoee_name: echoee_name.to_string(),
        //         echoee_id: get_one_player_id(&child.player_tags, child.r#type)?,
        //         echoee_team_id: child.next_team_id()?,
        //         sub_event: child.as_sub_event(),
        //     })
        // }
        // EventType::InvestigationMessage => {
        //     make_fed_event(event, FedEventData::InvestigationMessage {
        //         player_id: get_one_player_id(player_tags, event.r#type)?,
        //         message: event.description.clone(),
        //     })
        // }
        // EventType::Tidings => {
        //     make_fed_event(event, FedEventData::Tidings {
        //         message: event.description.clone(),
        //         metadata: event.metadata.clone(),
        //         player_tags: player_tags.into(),
        //     })
        // }
        // EventType::Middling => {
        //     let (team_nickname, is_middling) = event.next_parse(parse_middling)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //
        //     let child = get_one_sub_event_from_slice(children, event.r#type)?;
        //     make_fed_event(event, FedEventData::Middling {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_nickname: team_nickname.to_string(),
        //         is_middling,
        //         change_event: ModChangeSubEvent {
        //             sub_event: child.as_sub_event(),
        //             team_id: child.next_team_id()?,
        //         },
        //     })
        // }
        // EventType::PlayerAttributeIncrease => { todo!() }
        // EventType::PlayerAttributeDecrease => { todo!() }
        // EventType::EnterCrimeScene => {
        //     let (_player_name, stadium_nickname) = event.next_parse(parse_enter_crime_scene)?;
        //
        //     let (crime_scene_event, shadows_event) = get_two_sub_events_from_slice(children, event.r#type)?;
        //
        //     make_fed_event(event, FedEventData::EnterCrimeScene {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         player_id: get_uuid_metadata(crime_scene_event, "playerId")?,
        //         player_name: get_str_metadata(crime_scene_event, "playerName")?.to_string(),
        //         previous_team_id: get_uuid_metadata(crime_scene_event, "sendTeamId")?,
        //         previous_team_name: get_str_metadata(crime_scene_event, "sendTeamName")?.to_string(),
        //         previous_location: get_int_metadata(crime_scene_event, "location")?
        //             .try_into()
        //             .map_err(|_| FeedParseError::MissingMetadata {
        //                 event_type: event.r#type,
        //                 field: "location",
        //             })?,
        //         new_team_id: get_uuid_metadata(crime_scene_event, "receiveTeamId")?,
        //         new_team_name: get_str_metadata(crime_scene_event, "receiveTeamName")?.to_string(),
        //         stadium_name: stadium_nickname.to_string(),
        //         rating_before: get_float_metadata(shadows_event, "before")?,
        //         rating_after: get_float_metadata(shadows_event, "after")?,
        //         enter_crime_scene_sub_event: crime_scene_event.as_sub_event(),
        //         enter_shadows_sub_event: shadows_event.as_sub_event(),
        //     })
        // }
        // EventType::ItemBreaks => { todo!() }
        // EventType::Announcement => { todo!() }
        // EventType::RunsScored => { todo!() }
        // EventType::WinCollectedRegular => { todo!() }
        // EventType::WinCollectedPostseason => { todo!() }
        // EventType::GameOver => { todo!() }
        // EventType::StormWarning => { todo!() }
        // EventType::Snowflakes => { todo!() }
        // EventType::Sun2SetWin => {
        //     let team_name = event.next_parse(parse_sun2_set_win)?;
        //     assert!(is_known_team_nickname(team_name));
        //     make_fed_event(event, FedEventData::Sun2SetWin {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_name.to_string(),
        //     })
        // }
        // EventType::BlackHoleSwallowedWin => {
        //     let team_name = event.next_parse(parse_black_hole_swallowed_win)?;
        //     assert!(is_known_team_nickname(team_name));
        //     make_fed_event(event, FedEventData::BlackHoleSwallowedWin {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_name.to_string(),
        //     })
        // }
        // EventType::RemovedModFromOtherMod => { todo!() }
        // EventType::PostseasonAdvance => {
        //     let (team_nickname, round_num, season_num) = event.next_parse(parse_postseason_advance)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     make_fed_event(event, FedEventData::PostseasonAdvance {
        //         team_id: event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         round: round_num,
        //         season: season_num,
        //     })
        // }
        // EventType::GainBloodType => { todo!() }
        // EventType::HighPressure => {
        //     let (team_nickname, is_on) = event.next_parse(parse_high_pressure)?;
        //     assert!(is_known_team_nickname(team_nickname));
        //     let sub_event = get_one_sub_event(event)?;
        //     make_fed_event(event, FedEventData::HighPressure {
        //         game: event.game(unscatter, attractor_secret_base)?,
        //         team_id: sub_event.next_team_id()?,
        //         team_nickname: team_nickname.to_string(),
        //         is_on,
        //         sub_event: sub_event.as_sub_event(),
        //     })
        // }
        // EventType::LineupSorted => {
        //     // This happened as a top-level event exactly once (and really it should have been a
        //     // child of the lovers' getting Base Dealing)
        //     parse_fixed_description(event, "The Lovers' lineup has been optimized.",
        //                             FedEventData::LineupSorted {
        //                                 team_id: event.next_team_id()?,
        //                                 team_nickname: "Lovers".to_string(),
        //                             })
        // }
        // EventType::NutButton => { todo!() }
        EventType::PostseasonEliminated => {
            let (team_nickname, season_num) = event.next_parse(parse_postseason_eliminated)?;
            assert!(is_known_team_nickname(team_nickname));
            FedEventData::PostseasonEliminated {
                team_id: event.next_team_id()?,
                team_nickname: team_nickname.to_string(),
                season: season_num,
            }
        }
        _ => { todo!("Remove me after refactoring") }
    };

    event.to_fed(data)
}

// fn extract_batter_debt<'a>(children: &'a [EventuallyEvent], player_tags: &'a [Uuid], event_type: EventType, batter_debt: bool) -> Result<(Option<BatterDebt>, &'a [Uuid], &'a [EventuallyEvent]), FeedParseError> {
//     if batter_debt {
//         let (observed_tags, other_tags) = player_tags.split_at(2);
//         let (batter_id, fielder_id) = observed_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//                 event_type,
//                 tag_type: "player",
//                 expected_num: 2,
//                 actual_num: player_tags.len(),
//             })?;
//
//         let (sub_event, rest_children) = if children.first().map_or(false, |child| child.r#type == EventType::AddedMod) {
//             let (child, rest_children) = children.split_first()
//                 .expect("If there isn't at least one child we shouldn't be in this branch of the if");
//
//             let sub_event = ModChangeSubEvent {
//                 team_id: child.next_team_id()?,
//                 sub_event: child.as_sub_event(),
//             };
//
//             (Some(sub_event), rest_children)
//         } else {
//             (None, children)
//         };
//
//         let batter_debt = BatterDebt {
//             batter_id: *batter_id,
//             fielder_id: *fielder_id,
//             sub_event,
//         };
//
//         Ok((Some(batter_debt), other_tags, rest_children))
//     } else {
//         Ok((None, player_tags, children))
//     }
// }
//
// fn make_echo(echoer_name: &str, events: &(Option<&EventuallyEvent>, &EventuallyEvent)) -> Result<Echo, FeedParseError> {
//     let (removed, added) = events;
//     // I could verify that the IDs all match, but the round-trip test should verify that
//     Ok(Echo {
//         receiver_team_id: added.next_team_id()?,
//         receiver_id: get_one_player_id(&added.player_tags, added.r#type)?,
//         receiver_name: echoer_name.to_string(),
//         mods_removed: removed.map(get_mods_removed).transpose()?,
//         mods_added: get_mods_added(added)?,
//     })
// }
//
// #[derive(Deserialize)]
// struct ModAndType {
//     r#mod: String,
//     // r#type: i32,
// }
//
// fn get_mods_removed(event: &EventuallyEvent) -> Result<MultipleModsAddedOrRemoved, FeedParseError> {
//     #[derive(Deserialize)]
//     struct EchoMetadata {
//         removes: Vec<ModAndType>,
//     }
//
//     let des: EchoMetadata = serde_json::from_value(event.metadata.other.clone())
//         .map_err(|_| FeedParseError::MissingMetadata {
//             event_type: event.r#type,
//             field: "removes",
//         })?;
//
//     let mod_ids = des.removes.into_iter()
//         .map(|mod_and_type| mod_and_type.r#mod)
//         .collect();
//     Ok(MultipleModsAddedOrRemoved { mod_ids, sub_event: event.as_sub_event() })
// }
//
// fn get_mods_added(event: &EventuallyEvent) -> Result<MultipleModsAddedOrRemoved, FeedParseError> {
//     #[derive(Deserialize)]
//     struct EchoMetadata {
//         adds: Vec<ModAndType>,
//     }
//
//     let des: EchoMetadata = serde_json::from_value(event.metadata.other.clone())
//         .map_err(|_| FeedParseError::MissingMetadata {
//             event_type: event.r#type,
//             field: "adds",
//         })?;
//
//     let mod_ids = des.adds.into_iter()
//         .map(|mod_and_type| mod_and_type.r#mod)
//         .collect();
//     Ok(MultipleModsAddedOrRemoved { mod_ids, sub_event: event.as_sub_event() })
// }
//
// fn zip_mod_change_events(names: Vec<&str>, children: &[EventuallyEvent]) -> Result<Vec<ModChangeSubEventWithNamedPlayer>, FeedParseError> {
//     names.iter().zip_eq(children)
//         .map(|(name, sub_event)| Ok(ModChangeSubEventWithNamedPlayer {
//             sub_event: sub_event.as_sub_event(),
//             team_id: sub_event.next_team_id()?,
//             player_id: get_one_player_id(&sub_event.player_tags, sub_event.r#type)?,
//             player_name: name.to_string(),
//         }))
//         .collect::<Result<_, _>>()
// }
//
// fn get_one_player_id_advanced(player_tags: &[Uuid], event_type: EventType, has_extra_id: bool) -> Result<Uuid, FeedParseError> {
//     if has_extra_id {
//         let (&id1, &id2) = player_tags.iter().collect_tuple()
//             .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//                 event_type,
//                 tag_type: "player",
//                 expected_num: 2,
//                 actual_num: player_tags.len(),
//             })?;
//         if id1 != id2 {
//             Err(FeedParseError::ExpectedEqualTags {
//                 event_type,
//                 tag_type: "player",
//                 tag1: id1,
//                 tag2: id2,
//             })
//         } else {
//             Ok(id1)
//         }
//     } else {
//         get_one_player_id(&player_tags, event_type)
//     }
// }
//
// fn extract_spicy_event(children: &[EventuallyEvent], spicy_status: ParsedSpicyStatus) -> Result<(&[EventuallyEvent], SpicyStatus), FeedParseError> {
//     Ok(match spicy_status {
//         ParsedSpicyStatus::None => { (children, SpicyStatus::None) }
//         ParsedSpicyStatus::HeatingUp => { (children, SpicyStatus::HeatingUp) }
//         ParsedSpicyStatus::RedHot => {
//             // TODO Is the spicy event always the last? first? neither?
//             if let Some((spicy_event, children)) = children.split_last() {
//                 if spicy_event.r#type == EventType::AddedMod {
//                     (children, SpicyStatus::RedHot(Some(ModChangeSubEvent {
//                         sub_event: spicy_event.as_sub_event(),
//                         team_id: spicy_event.next_team_id()?,
//                     })))
//                 } else {
//                     (&children, SpicyStatus::RedHot(None))
//                 }
//             } else {
//                 (&children, SpicyStatus::RedHot(None))
//             }
//         }
//     })
// }
//
// fn extract_cooled_off_event<'e, 't>(event: &'e EventuallyEvent, children: &'e [EventuallyEvent], cooled_off: bool, player_tags: &'t [Uuid]) -> Result<(&'e [EventuallyEvent], Option<ModChangeSubEventWithPlayer>, &'t [Uuid]), FeedParseError> {
//     Ok(match cooled_off {
//         false => { (children, None, player_tags) }
//         true => {
//             // TODO Is the spicy event always the last? first? neither?
//             if let Some((cooled_off_event, children)) = children.split_last() {
//                 // TODO Make this assert into a propagated error
//                 assert_eq!(cooled_off_event.r#type, EventType::RemovedMod);
//                 let (&player_id, remaining_tags) = player_tags.split_last()
//                     .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//                         event_type: event.r#type,
//                         tag_type: "player",
//                         expected_num: 1, // at least
//                         actual_num: 0,
//                     })?;
//
//                 (children, Some(ModChangeSubEventWithPlayer {
//                     sub_event: cooled_off_event.as_sub_event(),
//                     team_id: cooled_off_event.next_team_id()?,
//                     player_id,
//                 }), remaining_tags)
//             } else {
//                 Err(FeedParseError::MissingChild {
//                     event_type: event.r#type,
//                     expected_num_children: 1,  // at least one
//                 })?
//             }
//         }
//     })
// }
//
// fn merge_scores_with_ids(
//     scores: ParsedScores,
//     scorer_ids: &[Uuid],
//     children: &[EventuallyEvent],
//     event_type: EventType,
//     extra_player_tags: usize,
// ) -> Result<(ScoreInfo, Option<StoppedInhabiting>), FeedParseError> {
//     let mut children = children.iter();
//
//     if scorer_ids.len() != scores.scorers.len() {
//         return Err(FeedParseError::WrongNumberOfTags {
//             event_type,
//             tag_type: "player",
//             expected_num: scores.scorers.len() + extra_player_tags,
//             actual_num: scorer_ids.len() + extra_player_tags,
//         });
//     }
//
//     let scoring_players = scores.scorers.into_iter().zip(scorer_ids)
//         .map(|(score, &scorer_id)| Ok::<_, FeedParseError>(ScoringPlayer {
//             player_id: scorer_id,
//             player_name: score.to_string(),
//         }))
//         .collect::<Result<_, _>>()?;
//
//
//     let free_refills = scores.refillers.into_iter()
//         .map(|name| {
//             if let Some(event) = children.next() {
//                 let expected_description = format!("{} used their Free Refill.", name);
//                 if event.description == expected_description {
//                     Ok(FreeRefill {
//                         sub_event: event.as_sub_event(),
//                         player_name: name.to_string(),
//                         player_id: get_one_player_id(&event.player_tags, event.r#type)?,
//                         team_id: event.next_team_id()?,
//                         sub_play: get_sub_play(event)?,
//                     })
//                 } else {
//                     Err(FeedParseError::UnexpectedDescription {
//                         event_type,
//                         description: event.description.clone(),
//                         expected: expected_description,
//                     })
//                 }
//             } else {
//                 Err(FeedParseError::MissingChild {
//                     event_type,
//                     expected_num_children: -1,
//                 })
//             }
//         })
//         .collect::<Result<_, _>>()?;
//
//     let result = ScoreInfo {
//         scoring_players,
//         free_refills,
//     };
//
//     if let Some(extra_child) = children.next() {
//         if extra_child.r#type == EventType::RemovedMod && extra_child.metadata.other.as_object()
//             .and_then(|o| o.get("mod"))
//             .and_then(|m| m.as_str())
//             .map(|m| m == "INHABITING")
//             .unwrap_or(false) {
//             let name = run_parser(&extra_child.description, extra_child.r#type, parse_stopped_inhabiting)?;
//             Ok((result, Some(StoppedInhabiting {
//                 sub_event: extra_child.as_sub_event(),
//                 inhabiting_player_name: name.to_string(),
//                 inhabiting_player_id: get_one_player_id(&extra_child.player_tags, extra_child.r#type)?,
//                 inhabiting_player_team_id: if extra_child.team_tags.is_empty() {
//                     None
//                 } else {
//                     Some(extra_child.next_team_id()?)
//                 },
//             })))
//         } else {
//             Err(FeedParseError::MissingChild {
//                 event_type,
//                 expected_num_children: 0,
//             })
//         }
//     } else {
//         Ok((result, None))
//     }
// }
//
// fn make_free_refill(event_type: EventType, children: &mut Iter<EventuallyEvent>, refiller_name: &str) -> Result<FreeRefill, FeedParseError> {
//     let child = children.next()
//         .ok_or_else(|| {
//             FeedParseError::MissingChild {
//                 event_type,
//                 expected_num_children: -1, // Unknown at this point in the computation
//             }
//         })?;
//
//     let (&team_id, ) = child.team_tags.iter().collect_tuple()
//         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//             event_type,
//             tag_type: "team",
//             expected_num: 1,
//             actual_num: child.team_tags.len(),
//         })?;
//
//     let (&player_id, ) = child.player_tags.iter().collect_tuple()
//         .ok_or_else(|| FeedParseError::WrongNumberOfTags {
//             event_type,
//             tag_type: "player",
//             expected_num: 1,
//             actual_num: child.player_tags.len(),
//         })?;
//
//     Ok(FreeRefill {
//         sub_event: child.as_sub_event(),
//         player_name: refiller_name.to_string(),
//         player_id,
//         team_id,
//         sub_play: get_sub_play(child)?,
//     })
// }

fn is_known_team_name(name: &str) -> bool {
    vec!["Hawai'i Fridays", "Canada Moist Talkers", "San Francisco Lovers", "Seattle Garages",
         "Breckenridge Jazz Hands", "Hellmouth Sunbeams", "Hades Tigers", "Mexico City Wild Wings",
         "Boston Flowers", "New York Millennials", "Philly Pies", "Miami Dale", "Tokyo Lift",
         "Chicago Firefighters", "Dallas Steaks", "Yellowstone Magic", "Kansas City Breath Mints",
         "Houston Spies", "Charleston Shoe Thieves", "LA Unlimited Tacos", "Atlantis Georgias",
         "Ohio Worms", "Baltimore Crabs", "Core Mechanics",
    ].contains(&name)
}

fn is_known_team_nickname(name: &str) -> bool {
    KNOWN_TEAM_NICKNAMES.contains(&name)
}

fn sort_children(event: &mut EventuallyEvent) {
    if event.metadata.children.iter().all(|child| child.metadata.sub_play.is_some()) {
        event.metadata.children.sort_by_key(|e| e.metadata.sub_play
            .expect("Shouldn't get here if sub_play is None"));
    }
    for child in event.metadata.children.as_mut_slice() {
        sort_children(child);
    }
}

pub fn feed_event_from_json(str: &String) -> serde_json::Result<EventuallyEvent> {
    let mut feed_event: EventuallyEvent = serde_json::from_str(&str)?;

    sort_children(&mut feed_event);

    Ok(feed_event)
}
