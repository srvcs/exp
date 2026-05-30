use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-exp";
pub const CONCERN: &str = "arithmetic: e^value";
pub const DEPENDS_ON: &[&str] = &["srvcs-isnumber"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub isnumber_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    #[schema(value_type = Object)]
    pub value: Value,
}

#[derive(Serialize, ToSchema)]
pub struct ExpResponse {
    #[schema(value_type = Object)]
    pub value: Value,
    pub result: f64,
}

/// The single concern: the natural exponential `e^value`.
///
/// Both integers and floats are valid input: `exp(0.0) == 1.0`,
/// `exp(1.0) ~= 2.718281828459045`.
pub fn exp(value: f64) -> f64 {
    value.exp()
}

fn ok(value: Value, result: f64) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "value": value, "result": result })),
    )
        .into_response()
}

fn invalid(reason: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": reason })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for invalid
/// input, so exp reports the same rejection its dependency did).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// Validate `value` is a number by asking `srvcs-isnumber`, mapping its
/// failures to the response this service should return.
async fn ask_is_number(url: &str, value: &Value, dependency: &str) -> Result<(), Response> {
    match client::call(url, &json!({ "value": value })).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => {
            let is_number = body.get("result").and_then(Value::as_bool).unwrap_or(false);
            if is_number {
                Ok(())
            } else {
                Err(invalid("value is not a number"))
            }
        }
        // Invalid input propagates from the leaf dependency; forward it.
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute `e^value`.
///
/// Input validation is delegated to `srvcs-isnumber` over HTTP (the single
/// source of truth for "is this a number"). Both integers and floats are valid
/// input here: this service raises e to the given power and returns an `f64`.
/// If the dependency is unreachable, this service reports itself degraded
/// rather than guessing.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = ExpResponse),
        (status = 422, description = "value is not a number"),
        (status = 500, description = "value passed validation but is not representable as a number"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    // 1. Delegate "is this a number" to srvcs-isnumber.
    if let Err(resp) = ask_is_number(&deps.isnumber_url, &req.value, "srvcs-isnumber").await {
        return resp;
    }

    // 2. Coerce to f64 (integers and floats both accepted).
    let Some(f) = req.value.as_f64() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "value validated as a number but is not representable as f64" })),
        )
            .into_response();
    };

    ok(req.value, exp(f))
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, ExpResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[test]
    fn exp_of_zero_is_one() {
        assert!((exp(0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn exp_of_one_is_e() {
        assert!((exp(1.0) - std::f64::consts::E).abs() < 1e-9);
    }

    #[test]
    fn exp_of_two_matches_e_squared() {
        // e^2 computed independently of a direct call to exp(2).
        let expected = std::f64::consts::E * std::f64::consts::E;
        assert!((exp(2.0) - expected).abs() < 1e-9);
    }

    #[test]
    fn exp_of_negative_is_reciprocal() {
        assert!((exp(-1.0) - (1.0 / std::f64::consts::E)).abs() < 1e-9);
    }

    #[tokio::test]
    async fn index_reports_dependency() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-exp");
        assert_eq!(info.concern, "arithmetic: e^value");
        assert_eq!(info.depends_on, vec!["srvcs-isnumber"]);
    }
}
