//! An optional JSON body for POSTs whose body is optional.
//!
//! axum 0.8's `Option<Json<T>>` is `None` only when the request has no JSON content type; the
//! dashboard sends `Content-Type: application/json` on every request, bodiless POSTs included,
//! so those became 400s. `OptJson` reads an empty (or `null`) body as `None` and still rejects
//! malformed JSON with a 400.

use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

pub struct OptJson<T>(pub Option<T>);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for OptJson<T> {
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(IntoResponse::into_response)?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(OptJson(None));
        }
        serde_json::from_slice::<Option<T>>(&bytes)
            .map(OptJson)
            .map_err(|e| {
                (StatusCode::BAD_REQUEST, format!("invalid JSON body: {e}")).into_response()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    struct Req {
        n: i32,
    }

    async fn extract(body: &'static str) -> Result<Option<Req>, StatusCode> {
        let req = Request::builder()
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        OptJson::<Req>::from_request(req, &())
            .await
            .map(|OptJson(v)| v)
            .map_err(|r| r.status())
    }

    #[tokio::test]
    async fn empty_or_null_body_is_none() {
        assert_eq!(extract("").await, Ok(None));
        assert_eq!(extract("  \n").await, Ok(None));
        assert_eq!(extract("null").await, Ok(None));
    }

    #[tokio::test]
    async fn a_body_is_parsed_and_a_bad_one_rejected() {
        assert_eq!(extract(r#"{"n":3}"#).await, Ok(Some(Req { n: 3 })));
        assert_eq!(extract("{").await, Err(StatusCode::BAD_REQUEST));
    }
}
