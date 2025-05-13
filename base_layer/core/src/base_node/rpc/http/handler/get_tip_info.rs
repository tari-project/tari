use crate::base_node::rpc::BaseNodeWalletQueryService;
use crate::proto::base_node::TipInfoResponse;
use axum::http::StatusCode;
use axum::{Extension, Json};
use std::sync::Arc;

async fn handle(
    Extension(query_service): Extension<Arc<impl BaseNodeWalletQueryService>>,
) -> Result<Json<TipInfoResponse>, StatusCode> {
    let tip_info = query_service.get_tip_info().await
        .map_err(|error| {
            http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(tip_info))
}