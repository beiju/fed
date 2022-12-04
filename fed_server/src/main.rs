#![feature(try_trait_v2)]
#[macro_use]
extern crate rocket;

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ops::FromResidual;
use rocket::http::uri::Origin;
use rocket::serde::json::Json;
use rocket::serde::Serialize;
use rocket::State;
use fed::FedEvent;

const EVENTUALLY_ENDPOINT: &str = "https://api.sibr.dev/eventually/v2/events";

#[derive(Responder, Debug)]
pub enum ApiResult<'a> {
    #[response(status = 200, content_type = "Json")]
    Success(Json<Vec<FedEvent>>),
    #[response(status = 400, content_type = "Json")]
    ApiUseError(Json<ApiUseError<'a>>),
    #[response(status = 500, content_type = "Json")]
    ApiServerError(Json<ApiServerError>),
    #[response(status = 400, content_type = "plain")]
    NotImplemented(String),
    //there could be more here
}

impl<'a> FromResidual<Result<Infallible, ApiUseError<'a>>> for ApiResult<'a> {
    fn from_residual(residual: Result<Infallible, ApiUseError<'a>>) -> Self {
        Self::ApiUseError(Json(residual.unwrap_err()))
    }
}

impl FromResidual<Result<Infallible, reqwest::Error>> for ApiResult<'_> {
    fn from_residual(residual: Result<Infallible, reqwest::Error>) -> Self {
        Self::ApiServerError(Json(ApiServerError::HttpFailed {
            message: residual.unwrap_err().to_string(),
        }))
    }
}

impl FromResidual<Result<Infallible, serde_json::Error>> for ApiResult<'_> {
    fn from_residual(residual: Result<Infallible, serde_json::Error>) -> Self {
        Self::ApiServerError(Json(ApiServerError::JsonParseFailed {
            message: residual.unwrap_err().to_string(),
        }))
    }
}

impl FromResidual<Result<Infallible, fed::FeedParseError>> for ApiResult<'_> {
    fn from_residual(residual: Result<Infallible, fed::FeedParseError>) -> Self {
        Self::ApiServerError(Json(ApiServerError::FeedParseFailed {
            message: residual.unwrap_err().to_string(),
        }))
    }
}


#[derive(Debug, Clone, Serialize)]
pub struct InvalidParameter<'a> {
    name: &'a str,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ApiUseError<'a> {
    InvalidParameters {
        parameters: Vec<InvalidParameter<'a>>,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ApiServerError {
    HttpFailed {
        message: String,
    },

    JsonParseFailed {
        message: String,
    },

    FeedParseFailed {
        message: String,
    },
}

fn validate_parameter<'a>(entry: Entry<&'a str, &'a str>, expected: &'a str) -> Option<InvalidParameter<'a>> {
    let name = *entry.key();
    let value = entry.or_insert(expected);
    if value != &expected {
        Some(InvalidParameter {
            name,
            reason: format!("Fed requires {name}={expected}, but received {name}={value}. \
                              Either pass {name}={expected} or remove the {name} attribute."),
        })
    } else {
        None
    }
}

#[get("/events")]
async fn get_events<'a>(uri: &'a Origin<'_>, client: &State<reqwest::Client>) -> ApiResult<'a> {
    let mut params = if let Some(query) = uri.query() {
        query.segments().collect()
    } else {
        HashMap::new()
    };

    let errs: Vec<_> = [
        validate_parameter(params.entry("expand_children"), "true"),
        validate_parameter(params.entry("expand_siblings"), "true"),
        validate_parameter(params.entry("metadata.parent"), "notexists"),
    ].into_iter().flatten().collect();

    if !errs.is_empty() {
        Err(ApiUseError::InvalidParameters {
            parameters: errs
        })?;
    }

    let eventually_response = client.get(EVENTUALLY_ENDPOINT)
        .query(&params)
        .send()
        .await?
        .text()
        .await?;
    let eventually_events = eventually_api::events_from_str(&eventually_response)?;
    let fed_events = eventually_events.iter()
        .map(fed::parse_feed_event)
        .collect::<Result<Vec<_>, _>>()?;

    ApiResult::Success(Json(fed_events))
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let _rocket = rocket::build()
        .mount("/v1", routes![get_events])
        .manage(reqwest::Client::new())
        .launch()
        .await?;

    Ok(())
}
