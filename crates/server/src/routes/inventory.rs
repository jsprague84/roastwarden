//! Beans and planned roasts (`docs/batches.md`); purchases and costs (`docs/green-coffee.md`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::analysis::bean_guide::{self, BeanGuide, GuideQuery};
use crate::app::state::AppState;
use crate::coffee::cart::{CartRequest, Carts};
use crate::coffee::costs::RoastCost;
use crate::coffee::import::Draft;
use crate::coffee::inventory::{
    Bean, BeanDeleted, BeanDetail, BeanRequest, CostReport, InventoryInputError, InventoryService,
    MoveRequest, OrderRecorded, OrderRequest, PlanRequest, PlannedRoast, StockMove, StockRequest,
};
use crate::coffee::wanted::{
    Bought, BoughtRequest, Wanted, WantedRequest, WantedService, WantedUpdate,
};
use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_beans, create_bean))
        .routes(routes!(pick_lists))
        .routes(routes!(import_link))
        .routes(routes!(list_wanted, add_wanted))
        .routes(routes!(update_wanted, delete_wanted))
        .routes(routes!(wanted_bought))
        .routes(routes!(wanted_cart))
        .routes(routes!(get_bean, update_bean, delete_bean))
        .routes(routes!(bean_guide))
        .routes(routes!(add_stock))
        .routes(routes!(update_move, delete_move))
        .routes(routes!(record_order))
        .routes(routes!(cost_report))
        .routes(routes!(roast_cost))
        .routes(routes!(list_plans, create_plan))
        .routes(routes!(reorder_plans))
        .routes(routes!(get_plan, update_plan, delete_plan))
}

#[derive(Serialize, ToSchema)]
struct BeanDeletion {
    /// `deleted`, or `archived` when roasts use the bean (they keep it)
    result: BeanDeleted,
}

fn svc(state: &AppState) -> InventoryService {
    InventoryService::new(state.db.clone())
}

/// Bad input is a 400 with the reason; anything else is a logged 500.
fn fail(e: anyhow::Error, what: &str) -> Response {
    if let Some(bad) = e.downcast_ref::<InventoryInputError>() {
        return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct BeansQuery {
    /// Include archived beans
    #[serde(default)]
    archived: bool,
}

/// The green beans, with what is in stock (docs/batches.md).
#[utoipa::path(get, path = "/api/beans", tag = "beans", params(BeansQuery),
    responses((status = 200, body = Vec<Bean>)))]
async fn list_beans(State(state): State<AppState>, Query(q): Query<BeansQuery>) -> Response {
    match svc(&state).list_beans(q.archived).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to list beans"),
    }
}

/// Add a bean; `stock_g` records the first purchase.
#[utoipa::path(post, path = "/api/beans", tag = "beans", request_body = BeanRequest,
    responses((status = 201, body = Bean), (status = 400, body = ErrorBody)))]
async fn create_bean(State(state): State<AppState>, Json(req): Json<BeanRequest>) -> Response {
    match svc(&state).create_bean(req).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => fail(e, "Failed to create bean"),
    }
}

/// Suggestions for a bean's repeated fields: what was entered before (most used first),
/// then seeded countries, processes, varieties, certifications and suppliers.
#[utoipa::path(get, path = "/api/beans/suggestions", tag = "beans",
    responses((status = 200, body = crate::coffee::picklists::PickLists)))]
async fn pick_lists(State(state): State<AppState>) -> Response {
    match crate::coffee::picklists::pick_lists(&state.db).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to list suggestions"),
    }
}

#[derive(Deserialize, ToSchema)]
struct ImportRequest {
    /// A product page on a known supplier (docs/green-coffee-sources.md), https
    url: String,
}

/// Read one coffee from a supplier's product link: a preview, nothing is saved. Save the
/// draft as a bean (`POST /api/beans` takes its fields), a purchase, or a shopping-list
/// entry (`POST /api/wanted` with `draft`).
#[utoipa::path(post, path = "/api/beans/import", tag = "beans", request_body = ImportRequest,
    responses((status = 200, body = Draft), (status = 400, body = ErrorBody)))]
async fn import_link(State(state): State<AppState>, Json(req): Json<ImportRequest>) -> Response {
    match state.importer.draft(&req.url).await {
        Ok(d) => Json(d).into_response(),
        Err(e) => match crate::coffee::import::not_readable(&e) {
            Some(why) => (StatusCode::BAD_REQUEST, why.to_string()).into_response(),
            None => {
                tracing::warn!(error = %format!("{e:#}"), url = %req.url, "import failed");
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Could not read the page: {e:#}"),
                )
                    .into_response()
            }
        },
    }
}

fn wanted_svc(state: &AppState) -> WantedService {
    WantedService::new(state.db.clone())
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct WantedQuery {
    /// What was bought too
    #[serde(default)]
    all: bool,
}

/// The shopping list: coffees wanted (with `all`, also those bought).
#[utoipa::path(get, path = "/api/wanted", tag = "beans", params(WantedQuery),
    responses((status = 200, body = Vec<Wanted>)))]
async fn list_wanted(State(state): State<AppState>, Query(q): Query<WantedQuery>) -> Response {
    match wanted_svc(&state).list(q.all).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to list the shopping list"),
    }
}

/// Put a coffee on the shopping list: from an import's draft and a size, a known bean, or a
/// name.
#[utoipa::path(post, path = "/api/wanted", tag = "beans", request_body = WantedRequest,
    responses((status = 201, body = Wanted), (status = 400, body = ErrorBody)))]
async fn add_wanted(State(state): State<AppState>, Json(req): Json<WantedRequest>) -> Response {
    match wanted_svc(&state).add(req).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => fail(e, "Failed to add to the shopping list"),
    }
}

/// Edit a shopping-list entry (size, weight, price, notes, bean).
#[utoipa::path(put, path = "/api/wanted/{id}", tag = "beans", params(("id" = String, Path)),
    request_body = WantedUpdate,
    responses((status = 200, body = Wanted), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn update_wanted(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WantedUpdate>,
) -> Response {
    found(
        wanted_svc(&state).update(&id, req).await,
        "Failed to update the entry",
    )
}

/// Take a coffee off the shopping list.
#[utoipa::path(delete, path = "/api/wanted/{id}", tag = "beans", params(("id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn delete_wanted(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match wanted_svc(&state).delete(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to remove the entry"),
    }
}

/// It was bought: the bean (made from the draft when new) gets a purchase with the price.
#[utoipa::path(post, path = "/api/wanted/{id}/bought", tag = "beans",
    params(("id" = String, Path)), request_body = BoughtRequest,
    responses((status = 200, body = Bought), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn wanted_bought(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(req): OptJson<BoughtRequest>,
) -> Response {
    found(
        wanted_svc(&state)
            .bought(&id, req.unwrap_or_default())
            .await,
        "Failed to record the purchase",
    )
}

/// Cart links from the shopping list (docs/green-coffee.md, "Cart links"): per supplier one
/// link that opens a filled cart in the owner's browser. Each line's price and availability
/// are read from the store again first, a few seconds apart; nothing is bought.
#[utoipa::path(post, path = "/api/wanted/cart", tag = "beans", request_body = CartRequest,
    responses((status = 200, body = Carts), (status = 400, body = ErrorBody)))]
async fn wanted_cart(
    State(state): State<AppState>,
    OptJson(req): OptJson<CartRequest>,
) -> Response {
    match crate::coffee::cart::carts(
        &wanted_svc(&state),
        &state.importer,
        req.unwrap_or_default(),
    )
    .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to build the carts"),
    }
}

/// A bean with its stock ledger and roasts.
#[utoipa::path(get, path = "/api/beans/{id}", tag = "beans", params(("id" = String, Path)),
    responses((status = 200, body = BeanDetail), (status = 404, body = ErrorBody)))]
async fn get_bean(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    found(svc(&state).bean_detail(&id).await, "Failed to get bean")
}

/// A bean's guide (docs/new-bean.md): before its first roast on a roaster, what to expect
/// (first crack's BT and time on the profile, the supplier quoted); after a roast, while the
/// bean has fewer than three on its profile, what to change for the next one in seconds or
/// °C. Only reads: nothing changes a profile or a plan.
#[utoipa::path(get, path = "/api/beans/{id}/guide", tag = "beans",
    params(("id" = String, Path, description = "The bean's id"), GuideQuery),
    responses((status = 200, body = BeanGuide),
        (status = 400, description = "`after` is not a roast of this bean", body = ErrorBody),
        (status = 404, description = "No such bean, roast or profile", body = ErrorBody)))]
async fn bean_guide(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<GuideQuery>,
) -> Response {
    match bean_guide::guide(&state.db, &id, &q).await {
        Ok(g) => Json(g).into_response(),
        Err(crate::reports::ReportError::NotFound(m)) => (StatusCode::NOT_FOUND, m).into_response(),
        Err(crate::reports::ReportError::Bad(m) | crate::reports::ReportError::Design(m)) => {
            (StatusCode::BAD_REQUEST, m).into_response()
        }
        Err(crate::reports::ReportError::Internal(e)) => fail(e, "Failed to read the bean's guide"),
    }
}

/// Edit a bean: absent fields are kept, `null` clears the optional ones.
#[utoipa::path(put, path = "/api/beans/{id}", tag = "beans", params(("id" = String, Path)),
    request_body = BeanRequest,
    responses((status = 200, body = Bean), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn update_bean(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<BeanRequest>,
) -> Response {
    found(
        svc(&state).update_bean(&id, req).await,
        "Failed to update bean",
    )
}

/// Delete a bean; one that roasts use is archived instead.
#[utoipa::path(delete, path = "/api/beans/{id}", tag = "beans", params(("id" = String, Path)),
    responses((status = 200, body = BeanDeletion), (status = 404, body = ErrorBody)))]
async fn delete_bean(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match svc(&state).delete_bean(&id).await {
        Ok(BeanDeleted::NotFound) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Ok(done) => Json(BeanDeletion { result: done }).into_response(),
        Err(e) => fail(e, "Failed to delete bean"),
    }
}

/// A purchase or a correction: `delta_g` adds (or takes) grams, `set_g` sets the stock.
#[utoipa::path(post, path = "/api/beans/{id}/stock", tag = "beans", params(("id" = String, Path)),
    request_body = StockRequest,
    responses((status = 200, body = Bean), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn add_stock(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<StockRequest>,
) -> Response {
    found(
        svc(&state).add_stock(&id, req).await,
        "Failed to change stock",
    )
}

/// Edit a purchase or a correction: absent fields are kept, `null` clears. A roast's move
/// follows its roast.
#[utoipa::path(put, path = "/api/beans/{id}/moves/{move_id}", tag = "beans",
    params(("id" = String, Path), ("move_id" = String, Path)), request_body = MoveRequest,
    responses((status = 200, body = StockMove), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn update_move(
    State(state): State<AppState>,
    Path((id, move_id)): Path<(String, String)>,
    Json(req): Json<MoveRequest>,
) -> Response {
    found(
        svc(&state).update_move(&id, &move_id, req).await,
        "Failed to update the move",
    )
}

/// Take a purchase or a correction out of the ledger.
#[utoipa::path(delete, path = "/api/beans/{id}/moves/{move_id}", tag = "beans",
    params(("id" = String, Path), ("move_id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn delete_move(
    State(state): State<AppState>,
    Path((id, move_id)): Path<(String, String)>,
) -> Response {
    match svc(&state).delete_move(&id, &move_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to delete the move"),
    }
}

/// One order from a supplier: a purchase per bean, the shipping and fees split by weight.
#[utoipa::path(post, path = "/api/purchases", tag = "beans", request_body = OrderRequest,
    responses((status = 201, body = OrderRecorded), (status = 400, body = ErrorBody)))]
async fn record_order(State(state): State<AppState>, Json(req): Json<OrderRequest>) -> Response {
    match svc(&state).record_order(req).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => fail(e, "Failed to record the order"),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct CostQuery {
    /// How many months, newest first (default 12)
    months: Option<usize>,
}

/// Green coffee month by month: spent, bought, roasted, and what the roasted coffee cost.
#[utoipa::path(get, path = "/api/costs", tag = "beans", params(CostQuery),
    responses((status = 200, body = CostReport)))]
async fn cost_report(State(state): State<AppState>, Query(q): Query<CostQuery>) -> Response {
    match svc(&state)
        .cost_report(q.months.unwrap_or(12).clamp(1, 120))
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to add up the costs"),
    }
}

/// What a roast's green coffee cost: in all, per kg roasted, per dose and per bag; `null`
/// when it is not known (no bean, no green weight, or no priced purchase before the roast).
#[utoipa::path(get, path = "/api/sessions/{id}/cost", tag = "beans",
    params(("id" = String, Path)),
    responses((status = 200, body = Option<RoastCost>)))]
async fn roast_cost(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match svc(&state).roast_cost(&id).await {
        Ok(c) => Json(c).into_response(),
        Err(e) => fail(e, "Failed to cost the roast"),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PlanQuery {
    /// Only this roaster's (and those for any roaster)
    device_id: Option<String>,
    /// The roasted ones instead of the queue
    #[serde(default)]
    done: bool,
}

/// The queue of planned roasts, in order.
#[utoipa::path(get, path = "/api/plan", tag = "plan", params(PlanQuery),
    responses((status = 200, body = Vec<PlannedRoast>)))]
async fn list_plans(State(state): State<AppState>, Query(q): Query<PlanQuery>) -> Response {
    match svc(&state)
        .list_plans(q.device_id.as_deref().filter(|d| !d.is_empty()), q.done)
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to list planned roasts"),
    }
}

/// Plan a roast; it joins the end of the queue.
#[utoipa::path(post, path = "/api/plan", tag = "plan", request_body = PlanRequest,
    responses((status = 201, body = PlannedRoast), (status = 400, body = ErrorBody)))]
async fn create_plan(State(state): State<AppState>, Json(req): Json<PlanRequest>) -> Response {
    match svc(&state).create_plan(req).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => fail(e, "Failed to plan a roast"),
    }
}

/// One planned roast.
#[utoipa::path(get, path = "/api/plan/{id}", tag = "plan", params(("id" = String, Path)),
    responses((status = 200, body = PlannedRoast), (status = 404, body = ErrorBody)))]
async fn get_plan(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    found(svc(&state).plan(&id).await, "Failed to get planned roast")
}

/// Edit a planned roast: absent fields are kept, `null` clears.
#[utoipa::path(put, path = "/api/plan/{id}", tag = "plan", params(("id" = String, Path)),
    request_body = PlanRequest,
    responses((status = 200, body = PlannedRoast), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn update_plan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PlanRequest>,
) -> Response {
    found(
        svc(&state).update_plan(&id, req).await,
        "Failed to update planned roast",
    )
}

/// Take a roast off the plan.
#[utoipa::path(delete, path = "/api/plan/{id}", tag = "plan", params(("id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn delete_plan(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match svc(&state).delete_plan(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => fail(e, "Failed to delete planned roast"),
    }
}

#[derive(Deserialize, ToSchema)]
struct ReorderRequest {
    /// Planned roast ids in their new order
    ids: Vec<String>,
}

/// Put the queue in a new order.
#[utoipa::path(post, path = "/api/plan/reorder", tag = "plan", request_body = ReorderRequest,
    responses((status = 200, body = Vec<PlannedRoast>), (status = 400, body = ErrorBody)))]
async fn reorder_plans(State(state): State<AppState>, Json(req): Json<ReorderRequest>) -> Response {
    match svc(&state).reorder_plans(&req.ids).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(e, "Failed to reorder planned roasts"),
    }
}
