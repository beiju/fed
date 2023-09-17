use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use serde_repr::{Serialize_repr, Deserialize_repr};
use uuid::Uuid;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use derive_builder::Builder;
use schemars::gen::SchemaGenerator;
use schemars::JsonSchema;
use schemars::schema::Schema;


#[derive(Deserialize, Serialize)]
pub struct EventuallyResponse(pub(crate) Vec<EventuallyEvent>);

impl EventuallyResponse {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

impl IntoIterator for EventuallyResponse {
    type Item = EventuallyEvent;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        T: Default + Deserialize<'de>,
        D: serde::Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventMetadata {
    // In addition to collecting useful metadata, this should collect any metadata that isn't used
    // in game update' lastUpdateFull field
    #[serde(default)]
    #[schemars(skip)]
    pub children: Vec<EventuallyEvent>,
    #[serde(default)]
    #[schemars(skip)]
    #[serde(rename = "_eventually_siblingEvents")]
    pub siblings: Vec<EventuallyEvent>,
    #[serde(rename = "_eventually_ingest_time")]
    pub ingest_time: Option<i64>,
    #[serde(rename = "_eventually_ingest_source")]
    pub ingest_source: Option<String>,

    pub play: Option<i64>,
    pub sub_play: Option<i64>,
    pub sibling_ids: Option<Vec<Uuid>>,
    pub parent: Option<Uuid>,

    #[serde(flatten)]
    pub other: Value,
}

#[derive(Copy, Clone, Debug, Default, Serialize_repr, Deserialize_repr, PartialEq, JsonSchema)]
#[repr(i32)]
pub enum EventCategory {
    Redacted = -1,
    #[default]
    Game = 0,
    Changes = 1,
    Special = 2,
    Outcomes = 3,
    Narrative = 4,
}

impl EventCategory {
    pub fn special_if(cond: bool) -> Self {
        if cond { EventCategory::Special } else { EventCategory::Game }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Builder)]
#[builder(pattern = "owned")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventuallyEvent {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub r#type: EventType,
    pub category: EventCategory,
    // Some event types have "metadata: null", this replaces that with a default EventMetadata
    #[serde(deserialize_with = "deserialize_null_default")]
    #[builder(default)] pub metadata: EventMetadata,
    pub blurb: String,
    pub description: String,
    // These three are null for redacted events
    #[builder(default)] pub player_tags: Option<Vec<Uuid>>,
    #[builder(default)] pub game_tags: Option<Vec<Uuid>>,
    #[builder(default)] pub team_tags: Option<Vec<Uuid>>,
    pub sim: String,
    pub day: i32,
    pub season: i32,
    pub tournament: i32,
    pub phase: i32,
    pub nuts: i32,
}

// impl EventuallyEvent {
//     pub fn game_id(&self) -> Result<Uuid, anyhow::Error> {
//         self.game_tags.iter()
//             .exactly_one()
//             .map_err(|err| anyhow!("Expected exactly one game id but found {:?}", err))
//             .cloned()
//     }
//
//     pub fn player_id(&self) -> Result<Uuid, anyhow::Error> {
//         self.player_tags.iter()
//             .exactly_one()
//             .map_err(|err| anyhow!("Expected exactly one player id but found {:?}", err))
//             .cloned()
//     }
//
//     pub fn team_id(&self) -> Result<Uuid, anyhow::Error> {
//         self.team_id_excluding(Uuid::nil())
//     }
//
//     pub fn player_id_excluding(&self, excluding: Uuid) -> Result<Uuid, anyhow::Error> {
//         self.player_tags.iter()
//             .filter(|uuid| uuid != &&excluding)
//             .exactly_one()
//             .map_err(|err| anyhow!("Expected exactly one player id, excluding {}, but found {:?}", excluding, err))
//             .cloned()
//     }
//
//     pub fn team_id_excluding(&self, excluding: Uuid) -> Result<Uuid, anyhow::Error> {
//         self.team_tags.iter()
//             .filter(|uuid| uuid != &&excluding)
//             .exactly_one()
//             .map_err(|err| anyhow!("Expected exactly one team id, excluding {}, but found {:?}", excluding, err))
//             .cloned()
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr, JsonSchema, IntoPrimitive, TryFromPrimitive)]
#[repr(i32)]
pub enum Weather {
    Void = 0,
    Sun2 = 1,
    Overcast = 2,
    Rainy = 3,
    Sandstorm = 4,
    Snowy = 5,
    Acidic = 6,
    SolarEclipse = 7,
    Glitter = 8,
    Blooddrain = 9,
    Peanuts = 10,
    Birds = 11,
    Feedback = 12,
    Reverb = 13,
    BlackHole = 14,
    Coffee = 15,
    Coffee2 = 16,
    Coffee3s = 17,
    Flooding = 18,
    Salmon = 19,
    PolarityPlus = 20,
    PolarityMinus = 21,
    Sun90 = 23,
    SunPoint1 = 24,
    SumSun = 25,
    SupernovaEclipse = 26,
    BlackHoleBlackHole = 27,
    Jazz = 28,
    Night = 29,
}

//noinspection SpellCheckingInspection
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize_repr, Deserialize_repr, JsonSchema)]
#[repr(i32)]
pub enum EventType {
    #[default]
    Undefined = -1,
    LetsGo = 0,
    PlayBall = 1,
    HalfInning = 2,
    PitcherChange = 3,
    StolenBase = 4,
    Walk = 5,
    Strikeout = 6,
    FlyOut = 7,
    GroundOut = 8,
    HomeRun = 9,
    Hit = 10,
    GameEnd = 11,
    BatterUp = 12,
    Strike = 13,
    Ball = 14,
    FoulBall = 15,
    RunsOverflowing = 20,
    HomeFieldAdvantage = 21,
    HitByPitch = 22,
    BatterSkipped = 23,
    Party = 24,
    StrikeZapped = 25,
    WeatherChange = 26,
    MildPitch = 27,
    InningEnd = 28,
    BigDeal = 29,
    BlackHole = 30,
    Sun2 = 31,
    BirdsCircle = 33,
    AmbushedByCrows = 34,
    BirdsUnshell = 35,
    BecomeTripleThreat = 36,
    GainFreeRefill = 37,
    CoffeeBean = 39,
    FeedbackBlocked = 40,
    FeedbackSwap = 41,
    SuperallergicReaction = 45,
    AllergicReaction = 47,
    ReverbBestowsReverberating = 48,
    ReverbRosterShuffle = 49,
    Blooddrain = 51,
    BlooddrainSiphon = 52,
    BlooddrainBlocked = 53,
    Incineration = 54,
    IncinerationBlocked = 55,
    FlagPlanted = 56,
    RenovationBuilt = 57,
    LightSwitchToggled = 58,
    DecreePassed = 59,
    BlessingOrGiftWon = 60,
    WillRecieved = 61,
    FloodingSwept = 62,
    SalmonSwim = 63,
    PolarityShift = 64,
    EnterSecretBase = 65,
    ExitSecretBase = 66,
    ConsumersAttack = 67,
    EchoChamber = 69,
    GrindRail = 70,
    TunnelsUsed = 71,
    PeanutMister = 72,
    PeanutFlavorText = 73,
    TasteTheInfinite = 74,
    EventHorizonActivation = 76,
    EventHorizonAwaits = 77,
    SolarPanelsAwait = 78,
    SolarPanelsActivation = 79,
    TarotReading = 81,
    EmergencyAlert = 82,
    ReturnFromElsewhere = 84,
    OverUnder = 85,
    UnderOver = 86,
    Undersea = 88,
    Homebody = 91,
    Superyummy = 92,
    Perk = 93,
    Earlbird = 96,
    LateToTheParty = 97,
    ShameDonor = 99,
    AddedMod = 106,
    RemovedMod = 107,
    ModExpires = 108,
    PlayerAddedToTeam = 109,
    PlayerReplacedByNecromancy = 110,
    PlayerReplacesReturned = 111,
    PlayerRemovedFromTeam = 112,
    PlayerTraded = 113,
    PlayerSwap = 114,
    PlayerMoved = 115,
    PlayerBornFromIncineration = 116,
    PlayerStatIncrease = 117,
    PlayerStatDecrease = 118,
    PlayerStatReroll = 119,
    PlayerStatDecreaseFromSuperallergic = 122,
    PlayerMoveFailedForce = 124,
    EnterHallOfFlame = 125,
    ExitHallOfFlame = 126,
    PlayerGainedItem = 127,
    PlayerLostItem = 128,
    ReverbFullShuffle = 130,
    ReverbLineupShuffle = 131,
    ReverbRotationShuffle = 132,
    // At this point I got bored typing them all and only filled in the ones I encountered
    TeamDivisionMove = 135,
    PlayerDivisionMove = 136,
    PlayerHatched = 137,
    PlayerEvolves = 139,
    TeamWonInternetSeries = 141,
    EarnedPostseasonSlot = 142,
    FinalStandings = 143,
    ModChange = 144,
    PlayerAlternated = 145,
    AddedModFromOtherMod = 146,
    RemovedModFromOtherMod = 147,
    ChangedModFromOtherMod = 148,
    NecromancyOrPlunderNarration = 149,
    PlayerPermittedToStay = 150,
    DecreeNarration = 151,
    WillResults = 152,
    TeamStatAdjustment = 153,
    TeamWasShamed = 154,
    TeamDidShame = 155,
    Sun2SetWin = 156,
    BlackHoleSwallowedWin = 157,
    PostseasonEliminated = 158,
    PostseasonAdvance = 159,
    GainBloodType = 161,
    HighPressure = 165,
    LineupSorted = 166,
    NutButton = 168,
    Echo = 169,
    EchoIntoStatic = 170,
    RemovedModsFromAnotherMod = 171,
    AddedModsFromAnotherMod = 172,
    Psychoacoustics = 173,
    EchoReciever = 174,
    InvestigationMessage = 175,
    Tidings = 176,
    GlitterCrateDrop = 177,
    Middling = 178,
    PlayerAttributeIncrease = 179,
    PlayerAttributeDecrease = 180,
    EnterCrimeScene = 181,
    Ambitious = 182,
    Coasting = 184,
    ItemBreaks = 185,
    ItemDamaged = 186,
    BrokenItemRepaired = 187,
    DamagedItemRepaired = 188,
    CommunityChestOpens = 189,
    NoFreeItemSlot = 190,
    FaxMachine = 191,
    HolidayInning = 192,
    PrizeMatch = 193,
    TeamReceivedGifts = 194,
    Smithy = 195,
    PlayerSoulIncrease = 199,
    Announcement = 201,
    RunsScored = 209,
    WinCollectedRegular = 214,
    WinCollectedPostseason = 215,
    GameOver = 216,
    StormWarning = 263,
    Snowflakes = 264,
}

impl JsonSchema for EventuallyEvent {
    fn schema_name() -> String {
        todo!()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        todo!()
    }
}
