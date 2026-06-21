use crate::alarm_mqtt::{self, AlertRecord};
use crate::ch_writer::ClickHouseWriter;
use crate::config::AppConfig;
use crate::metrics::Metrics;
use crate::quality_predictor::QualityPredictor;
use crate::vibration_simulator::VibrationSimulator;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub struct AppState {
    pub ch_writer: Arc<ClickHouseWriter>,
    pub quality_predictor: Arc<QualityPredictor>,
    pub vibration_simulator: VibrationSimulator,
    pub config: Arc<AppConfig>,
    pub metrics: Arc<Metrics>,
}

#[derive(Serialize, Deserialize)]
pub struct SimulationRequest {
    pub spindle_id: String,
    pub rpm: f64,
    pub vibration_amplitude: f64,
    pub temperature: f64,
    pub twist_per_meter: f64,
}

#[derive(Serialize)]
pub struct SimulationResponse {
    pub vibration: crate::vibration_simulator::VibrationResult,
    pub yarn_quality: crate::quality_predictor::YarnQualityResult,
    pub alerts: Vec<AlertRecord>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let metrics_clone = Arc::clone(&state.metrics);

    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/api/sensor-data", get(get_sensor_data))
        .route("/api/vibration-analysis", get(get_vibration_analysis))
        .route("/api/yarn-quality", get(get_yarn_quality))
        .route("/api/alerts", get(get_alerts))
        .route("/api/simulate", post(run_simulation))
        .route("/api/spindle-list", get(get_spindle_list))
        .route("/api/latest/:spindle_id", get(get_latest))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(move |req: Request<Body>, next: axum::middleware::Next<Body>| {
            let m = Arc::clone(&metrics_clone);
            async move {
                let start = Instant::now();
                let method = req.method().clone();
                let endpoint = route_label(req.uri());
                let resp = next.run(req).await;
                let status = resp.status().as_str().to_string();
                let dur = start.elapsed().as_secs_f64();
                m.api_request_duration_seconds
                    .with_label_values(&[&endpoint, method.as_str(), &status])
                    .observe(dur);
                resp
            }
        }))
        .with_state(state)
}

fn route_label(uri: &Uri) -> String {
    let p = uri.path();
    if p.starts_with("/api/sensor-data") {
        "/api/sensor-data".into()
    } else if p.starts_with("/api/vibration-analysis") {
        "/api/vibration-analysis".into()
    } else if p.starts_with("/api/yarn-quality") {
        "/api/yarn-quality".into()
    } else if p.starts_with("/api/alerts") {
        "/api/alerts".into()
    } else if p.starts_with("/api/simulate") {
        "/api/simulate".into()
    } else if p.starts_with("/api/spindle-list") {
        "/api/spindle-list".into()
    } else if p.starts_with("/api/latest") {
        "/api/latest/:spindle_id".into()
    } else if p == "/metrics" {
        "/metrics".into()
    } else {
        p.into()
    }
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.metrics.encode_text() {
        Ok(txt) => (
            StatusCode::OK,
            [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
            txt,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("metrics encode error: {}", e),
        )
            .into_response(),
    }
}

async fn get_sensor_data(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.get("limit").map(|s| s.as_str()).unwrap_or("100");
    let spindle_id = params.get("spindle_id").map(|s| s.as_str()).unwrap_or("");
    let sql = if spindle_id.is_empty() {
        format!(
            "SELECT * FROM spindle_system.spindle_sensor_data ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            limit
        )
    } else {
        format!(
            "SELECT * FROM spindle_system.spindle_sensor_data WHERE spindle_id = '{}' ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            spindle_id, limit
        )
    };
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_vibration_analysis(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.get("limit").map(|s| s.as_str()).unwrap_or("100");
    let spindle_id = params.get("spindle_id").map(|s| s.as_str()).unwrap_or("");
    let sql = if spindle_id.is_empty() {
        format!(
            "SELECT * FROM spindle_system.vibration_analysis ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            limit
        )
    } else {
        format!(
            "SELECT * FROM spindle_system.vibration_analysis WHERE spindle_id = '{}' ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            spindle_id, limit
        )
    };
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_yarn_quality(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.get("limit").map(|s| s.as_str()).unwrap_or("100");
    let spindle_id = params.get("spindle_id").map(|s| s.as_str()).unwrap_or("");
    let sql = if spindle_id.is_empty() {
        format!(
            "SELECT * FROM spindle_system.yarn_quality ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            limit
        )
    } else {
        format!(
            "SELECT * FROM spindle_system.yarn_quality WHERE spindle_id = '{}' ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
            spindle_id, limit
        )
    };
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_alerts(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.get("limit").map(|s| s.as_str()).unwrap_or("100");
    let sql = format!(
        "SELECT * FROM spindle_system.alerts ORDER BY timestamp DESC LIMIT {} FORMAT JSON",
        limit
    );
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn run_simulation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SimulationRequest>,
) -> Json<SimulationResponse> {
    let vibration = state.vibration_simulator.analyze(req.rpm);
    let yarn_quality = state.quality_predictor.predict(
        &req.spindle_id,
        req.vibration_amplitude,
        req.twist_per_meter,
        chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
    );
    let alerts = alarm_mqtt::check_alerts(
        &req.spindle_id,
        req.rpm,
        req.vibration_amplitude,
        req.temperature,
        req.twist_per_meter,
        vibration.critical_rpm,
        vibration.whirl_instability,
        vibration.whirl_ratio,
        &state.config.alert_thresholds,
        state.config.regression_model.target_twist_per_meter,
    );
    Json(SimulationResponse {
        vibration,
        yarn_quality,
        alerts,
    })
}

async fn get_spindle_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    let sql = "SELECT DISTINCT spindle_id FROM spindle_system.spindle_sensor_data ORDER BY spindle_id FORMAT JSON".to_string();
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_latest(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(spindle_id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let sql = format!(
        "SELECT s.*, v.*, y.* FROM spindle_system.spindle_sensor_data s LEFT JOIN spindle_system.vibration_analysis v ON s.spindle_id = v.spindle_id AND s.timestamp = v.timestamp LEFT JOIN spindle_system.yarn_quality y ON s.spindle_id = y.spindle_id AND s.timestamp = y.timestamp WHERE s.spindle_id = '{}' ORDER BY s.timestamp DESC LIMIT 1 FORMAT JSON",
        spindle_id
    );
    match state.ch_writer.query(&sql).await {
        Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"data": body}));
            Ok(Json(parsed))
        }
        Err(e) => {
            tracing::error!("Query error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
