use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use super::{skill_http, ApiError, AppState, MachineDoctorReport};

#[derive(Debug, Default, Deserialize)]
struct DoctorQuery {
    #[serde(default)]
    force: bool,
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/doctor", get(machine_doctor))
}

async fn machine_doctor(
    State(state): State<AppState>,
    Query(query): Query<DoctorQuery>,
) -> Result<Json<MachineDoctorReport>, ApiError> {
    Ok(Json(
        skill_http::build_machine_doctor(&state, query.force).await?,
    ))
}
