//! Orders from receipts (`docs/green-coffee.md`, "Orders"; `orders.rs`).

use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::coffee::inventory::InventoryInputError;
use crate::coffee::orders::{
    MAX_RECEIPT_BYTES, OrderConflict, OrderImport, OrderImported, OrderService, OrdersQuery,
    PurchaseOrder, Receipt, ReceivedRequest, ShippedRequest,
};
use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_orders, import_order))
        .routes(routes!(get_order, delete_order))
        .routes(routes!(order_shipped))
        .routes(routes!(order_received))
        .routes(routes!(order_cancel))
        .routes(routes!(get_receipt, delete_receipt))
        .merge(
            // A receipt is a file of up to 5 MB, more than the default limit
            OpenApiRouter::new()
                .routes(routes!(add_receipt))
                .layer(DefaultBodyLimit::max(MAX_RECEIPT_BYTES + 1024)),
        )
}

fn svc(state: &AppState) -> OrderService {
    OrderService::new(state.db.clone())
}

/// Bad input is a 400, the same order or receipt again a 409, both with the reason.
fn fail(e: anyhow::Error, what: &str) -> Response {
    if let Some(bad) = e.downcast_ref::<InventoryInputError>() {
        return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
    }
    if let Some(c) = e.downcast_ref::<OrderConflict>() {
        return (StatusCode::CONFLICT, c.to_string()).into_response();
    }
    tracing::error!(?e, "{what}");
    (StatusCode::INTERNAL_SERVER_ERROR, what.to_string()).into_response()
}

fn found<T: serde::Serialize>(r: anyhow::Result<Option<T>>, what: &str) -> Response {
    match r {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, what),
    }
}

/// Supplier orders: open ones (ordered or shipped) unless `status` says otherwise, newest
/// first, with their lines and what is kept of their receipts.
#[utoipa::path(get, path = "/api/orders", tag = "beans", params(OrdersQuery),
    responses((status = 200, body = Vec<PurchaseOrder>), (status = 400, body = ErrorBody)))]
async fn list_orders(State(state): State<AppState>, Query(q): Query<OrdersQuery>) -> Response {
    match svc(&state).list(&q).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to list the orders"),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ImportQuery {
    /// Check only: the totals, the duplicate, each line's bean; nothing is written
    #[serde(default)]
    dry_run: bool,
}

/// A receipt read into an order (`roast order import`): checked (unique per supplier, the
/// totals to the cent, each line's bean), then recorded as ordered, shipped or received.
/// Only received writes the purchases. `dry_run` answers the check and writes nothing.
#[utoipa::path(post, path = "/api/orders", tag = "beans", params(ImportQuery),
    request_body = OrderImport,
    responses((status = 200, body = OrderImported, description = "The dry run's check"),
        (status = 201, body = OrderImported), (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "That order or receipt is recorded")))]
async fn import_order(
    State(state): State<AppState>,
    Query(q): Query<ImportQuery>,
    Json(req): Json<OrderImport>,
) -> Response {
    match svc(&state).import(req, q.dry_run).await {
        Ok(v) if v.dry_run => Json(v).into_response(),
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => fail(e, "Failed to record the order"),
    }
}

/// One order with its lines and receipts.
#[utoipa::path(get, path = "/api/orders/{id}", tag = "beans", params(("id" = String, Path)),
    responses((status = 200, body = PurchaseOrder), (status = 404, body = ErrorBody)))]
async fn get_order(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    found(svc(&state).get(&id).await, "Failed to read the order")
}

/// Remove an order with its lines and receipts; a received order's purchases too.
#[utoipa::path(delete, path = "/api/orders/{id}", tag = "beans", params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_order(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match svc(&state).delete(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to remove the order"),
    }
}

/// It shipped: the carrier and tracking from the shipping notice.
#[utoipa::path(post, path = "/api/orders/{id}/shipped", tag = "beans",
    params(("id" = String, Path)), request_body = ShippedRequest,
    responses((status = 200, body = PurchaseOrder), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn order_shipped(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(req): OptJson<ShippedRequest>,
) -> Response {
    found(
        svc(&state).shipped(&id, req.unwrap_or_default()).await,
        "Failed to record the shipment",
    )
}

/// It came: a purchase per coffee line with its share of the discount, shipping and tax;
/// a line that arrived different gives its weight (or another bean).
#[utoipa::path(post, path = "/api/orders/{id}/received", tag = "beans",
    params(("id" = String, Path)), request_body = ReceivedRequest,
    responses((status = 200, body = PurchaseOrder), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn order_received(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(req): OptJson<ReceivedRequest>,
) -> Response {
    found(
        svc(&state).received(&id, req.unwrap_or_default()).await,
        "Failed to receive the order",
    )
}

/// It will not come: cancelled, its shopping-list entries wanted again.
#[utoipa::path(post, path = "/api/orders/{id}/cancel", tag = "beans",
    params(("id" = String, Path)),
    responses((status = 200, body = PurchaseOrder), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn order_cancel(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    found(svc(&state).cancel(&id).await, "Failed to cancel the order")
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ReceiptQuery {
    /// confirmation (default) | shipping | invoice | other
    kind: Option<String>,
    /// The file's name, "Order 412345.eml"
    file_name: Option<String>,
    /// The email's Message-ID
    message_id: Option<String>,
}

/// Keep a receipt with its order, as received: the body is the file (an email, web page,
/// PDF, image or text; at most 5 MB). The same file twice is a 409.
#[utoipa::path(post, path = "/api/orders/{id}/receipts", tag = "beans",
    params(("id" = String, Path), ReceiptQuery),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 201, body = Receipt), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
        (status = 413, body = ErrorBody)))]
async fn add_receipt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ReceiptQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|c| !c.starts_with("application/octet-stream"));
    match svc(&state)
        .add_receipt(
            &id,
            q.kind.as_deref(),
            ct,
            q.file_name.as_deref(),
            q.message_id.as_deref(),
            &body,
        )
        .await
    {
        Ok(Some(r)) => (StatusCode::CREATED, Json(r)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to keep the receipt"),
    }
}

/// A receipt's file, as received. Private: the owner, or an inventory or admin token.
/// Always a download, sandboxed: an HTML email never runs in the dashboard's origin.
#[utoipa::path(get, path = "/api/orders/{id}/receipts/{receipt_id}", tag = "beans",
    params(("id" = String, Path), ("receipt_id" = String, Path)),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, body = ErrorBody)))]
async fn get_receipt(
    State(state): State<AppState>,
    Path((id, receipt_id)): Path<(String, String)>,
) -> Response {
    match svc(&state).receipt(&id, &receipt_id).await {
        Ok(Some((r, body))) => {
            let name: String = r
                .file_name
                .unwrap_or_else(|| format!("receipt-{}", r.id))
                .chars()
                .map(|c| {
                    if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            (
                [
                    (header::CONTENT_TYPE, r.content_type),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{name}\""),
                    ),
                    (
                        header::CONTENT_SECURITY_POLICY,
                        "default-src 'none'; sandbox".to_string(),
                    ),
                    (header::CACHE_CONTROL, "private, no-store".to_string()),
                ],
                body,
            )
                .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to read the receipt"),
    }
}

/// Remove one receipt from its order.
#[utoipa::path(delete, path = "/api/orders/{id}/receipts/{receipt_id}", tag = "beans",
    params(("id" = String, Path), ("receipt_id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_receipt(
    State(state): State<AppState>,
    Path((id, receipt_id)): Path<(String, String)>,
) -> Response {
    match svc(&state).delete_receipt(&id, &receipt_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to remove the receipt"),
    }
}
