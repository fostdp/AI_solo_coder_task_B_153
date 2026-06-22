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
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub era_id: Option<String>,
    #[serde(default)]
    pub balance_correction_fraction: Option<f64>,
}

#[derive(Serialize)]
pub struct SimulationResponse {
    pub vibration: crate::vibration_simulator::VibrationResult,
    pub yarn_quality: crate::quality_predictor::YarnQualityResult,
    pub alerts: Vec<AlertRecord>,
}

#[derive(Serialize, Deserialize)]
pub struct MaterialComparisonRequest {
    pub rpm: f64,
    pub era_id: Option<String>,
    #[serde(default = "default_material_list")]
    pub material_ids: Vec<String>,
}

fn default_material_list() -> Vec<String> {
    vec!["wood".into(), "copper".into(), "iron".into()]
}

#[derive(Serialize, Deserialize)]
pub struct EraComparisonRequest {
    pub rpm: Option<f64>,
    pub material_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct BalanceCorrectionRequest {
    pub rpm: f64,
    pub material_id: Option<String>,
    pub era_id: Option<String>,
    pub initial_unbalance_m: Option<f64>,
    pub target_unbalance_m: Option<f64>,
    pub max_correction_weight_g: Option<f64>,
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
        .route("/api/materials", get(get_material_list))
        .route("/api/eras", get(get_era_list))
        .route("/api/material-comparison", post(material_comparison))
        .route("/api/era-comparison", post(era_comparison))
        .route("/api/balance-correction", post(balance_correction))
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
    let material = req.material_id.as_ref().and_then(|id| match id.as_str() {
        "wood" => Some(&state.config.material_profiles.wood),
        "copper" => Some(&state.config.material_profiles.copper),
        "iron" => Some(&state.config.material_profiles.iron),
        _ => None,
    });
    let era = req.era_id.as_ref().and_then(|id| match id.as_str() {
        "ancient_yuan" => Some(&state.config.era_profiles.ancient_yuan),
        "modern_high_speed" => Some(&state.config.era_profiles.modern_high_speed),
        _ => None,
    });

    let vibration = match (material, era) {
        (Some(m), Some(e)) => state
            .vibration_simulator
            .analyze_with_material_and_era(req.rpm, m, e),
        _ => state.vibration_simulator.analyze(req.rpm),
    };

    let yarn_quality = state.quality_predictor.predict_with_context(
        &req.spindle_id,
        req.vibration_amplitude,
        req.twist_per_meter,
        chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        material,
        era,
        req.balance_correction_fraction,
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

async fn get_material_list(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let mats = &state.config.material_profiles;
    Json(json!({
        "materials": [
            {
                "material_id": mats.wood.material_id,
                "display_name": mats.wood.display_name,
                "density_kg_m3": mats.wood.density_kg_m3,
                "youngs_modulus_pa": mats.wood.youngs_modulus_pa,
                "damping_ratio_factor": mats.wood.damping_ratio_factor,
                "quality_factor": mats.wood.quality_factor,
                "color_hex": mats.wood.color_hex,
                "era_compatibility": mats.wood.era_compatibility,
            },
            {
                "material_id": mats.copper.material_id,
                "display_name": mats.copper.display_name,
                "density_kg_m3": mats.copper.density_kg_m3,
                "youngs_modulus_pa": mats.copper.youngs_modulus_pa,
                "damping_ratio_factor": mats.copper.damping_ratio_factor,
                "quality_factor": mats.copper.quality_factor,
                "color_hex": mats.copper.color_hex,
                "era_compatibility": mats.copper.era_compatibility,
            },
            {
                "material_id": mats.iron.material_id,
                "display_name": mats.iron.display_name,
                "density_kg_m3": mats.iron.density_kg_m3,
                "youngs_modulus_pa": mats.iron.youngs_modulus_pa,
                "damping_ratio_factor": mats.iron.damping_ratio_factor,
                "quality_factor": mats.iron.quality_factor,
                "color_hex": mats.iron.color_hex,
                "era_compatibility": mats.iron.era_compatibility,
            },
        ]
    }))
}

async fn get_era_list(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let eras = &state.config.era_profiles;
    Json(json!({
        "eras": [
            {
                "era_id": eras.ancient_yuan.era_id,
                "display_name": eras.ancient_yuan.display_name,
                "era_year": eras.ancient_yuan.era_year,
                "description": eras.ancient_yuan.description,
                "default_material": eras.ancient_yuan.default_material,
                "base_rpm_min": eras.ancient_yuan.base_rpm_min,
                "base_rpm_max": eras.ancient_yuan.base_rpm_max,
                "typical_rpm": eras.ancient_yuan.typical_rpm,
                "bearing_technology": eras.ancient_yuan.bearing_technology,
                "typical_yarn": eras.ancient_yuan.typical_yarn,
            },
            {
                "era_id": eras.modern_high_speed.era_id,
                "display_name": eras.modern_high_speed.display_name,
                "era_year": eras.modern_high_speed.era_year,
                "description": eras.modern_high_speed.description,
                "default_material": eras.modern_high_speed.default_material,
                "base_rpm_min": eras.modern_high_speed.base_rpm_min,
                "base_rpm_max": eras.modern_high_speed.base_rpm_max,
                "typical_rpm": eras.modern_high_speed.typical_rpm,
                "bearing_technology": eras.modern_high_speed.bearing_technology,
                "typical_yarn": eras.modern_high_speed.typical_yarn,
            },
        ]
    }))
}

async fn material_comparison(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MaterialComparisonRequest>,
) -> Json<Value> {
    let sim = &state.vibration_simulator;
    let cfg = &state.config;
    let era = req.era_id.as_ref().and_then(|id| match id.as_str() {
        "ancient_yuan" => Some(&cfg.era_profiles.ancient_yuan),
        "modern_high_speed" => Some(&cfg.era_profiles.modern_high_speed),
        _ => None,
    });

    let mut results: Vec<Value> = Vec::new();
    let mats = [
        ("wood", &cfg.material_profiles.wood),
        ("copper", &cfg.material_profiles.copper),
        ("iron", &cfg.material_profiles.iron),
    ];

    for (id, mat) in &mats {
        if !req.material_ids.iter().any(|m| m == id) {
            continue;
        }
        let vib = if let Some(e) = era {
            sim.analyze_with_material_and_era(req.rpm, mat, e)
        } else {
            let r = mat.apply_to_rotor(&sim.rotor);
            let s2 = VibrationSimulator::new(r, sim.bearing.clone());
            s2.analyze(req.rpm)
        };
        let disp_mm = vib.total_displacement * 1000.0;
        let cost_index = match *id {
            "wood" => 1.0,
            "copper" => 5.0,
            "iron" => 3.0,
            _ => 1.0,
        };
        results.push(json!({
            "material_id": mat.material_id,
            "display_name": mat.display_name,
            "critical_rpm": vib.critical_rpm,
            "total_displacement_mm": disp_mm,
            "whirl_risk": if vib.whirl_instability { 1.0 } else { 0.0 },
            "eccentricity_ratio": vib.eccentricity_ratio,
            "quality_factor": mat.quality_factor,
            "relative_density": mat.density_kg_m3 / cfg.material_profiles.iron.density_kg_m3,
            "damping_ratio_factor": mat.damping_ratio_factor,
            "cost_index": cost_index,
            "youngs_modulus_pa": mat.youngs_modulus_pa,
            "estimated_uniformity": (95.0 - disp_mm * 50.0).max(0.0) * mat.quality_factor,
            "estimated_strength": (15.0 - disp_mm * 3.0).max(0.0) * mat.quality_factor,
        }));
    }

    Json(json!({
        "rpm": req.rpm,
        "era_id": req.era_id,
        "comparisons": results,
    }))
}

async fn era_comparison(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EraComparisonRequest>,
) -> Json<Value> {
    let sim = &state.vibration_simulator;
    let cfg = &state.config;

    let eras: Vec<(&str, &crate::config::EraProfile)> = vec![
        ("ancient_yuan", &cfg.era_profiles.ancient_yuan),
        ("modern_high_speed", &cfg.era_profiles.modern_high_speed),
    ];

    let mut results: Vec<Value> = Vec::new();
    for (era_id, era) in &eras {
        let mat_id = req
            .material_id
            .clone()
            .unwrap_or_else(|| era.default_material.clone());
        let mat = match mat_id.as_str() {
            "wood" => &cfg.material_profiles.wood,
            "copper" => &cfg.material_profiles.copper,
            _ => &cfg.material_profiles.iron,
        };
        let rpm = req.rpm.unwrap_or(era.typical_rpm);
        let vib = sim.analyze_with_material_and_era(rpm, mat, era);
        let disp_mm = vib.total_displacement * 1000.0;

        let output_kg_per_hour = match era_id {
            &"ancient_yuan" => rpm / 500.0 * 2.5,
            &"modern_high_speed" => rpm / 18000.0 * 35.0,
            _ => 0.0,
        };

        results.push(json!({
            "era_id": era.era_id,
            "display_name": era.display_name,
            "era_year": era.era_year,
            "description": era.description,
            "rpm": rpm,
            "typical_rpm": era.typical_rpm,
            "critical_rpm": vib.critical_rpm,
            "total_displacement_mm": disp_mm,
            "whirl_instability": vib.whirl_instability,
            "whirl_ratio": vib.whirl_ratio,
            "material_id": mat.material_id,
            "manufacturing_precision_factor": era.manufacturing_precision_factor,
            "bearing_technology": era.bearing_technology,
            "typical_yarn": era.typical_yarn,
            "daily_output_kg": (output_kg_per_hour * 24.0).round(),
            "estimated_uniformity": (95.0 - disp_mm * 30.0).max(0.0) * mat.quality_factor,
            "estimated_strength": (15.0 - disp_mm * 2.0).max(0.0) * mat.quality_factor,
        }));
    }

    Json(json!({
        "comparisons": results,
    }))
}

async fn balance_correction(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BalanceCorrectionRequest>,
) -> Json<Value> {
    let sim = &state.vibration_simulator;
    let cfg = &state.config;

    let mut bal_cfg = cfg.balance_correction.clone();
    if let Some(v) = req.initial_unbalance_m {
        bal_cfg.initial_residual_unbalance_m = v;
    }
    if let Some(v) = req.target_unbalance_m {
        bal_cfg.target_residual_unbalance_m = v;
    }
    if let Some(v) = req.max_correction_weight_g {
        bal_cfg.max_correction_weight_grams = v;
    }

    let material = req.material_id.as_ref().and_then(|id| match id.as_str() {
        "wood" => Some(&cfg.material_profiles.wood),
        "copper" => Some(&cfg.material_profiles.copper),
        "iron" => Some(&cfg.material_profiles.iron),
        _ => None,
    });
    let era = req.era_id.as_ref().and_then(|id| match id.as_str() {
        "ancient_yuan" => Some(&cfg.era_profiles.ancient_yuan),
        "modern_high_speed" => Some(&cfg.era_profiles.modern_high_speed),
        _ => None,
    });

    let result = sim.compute_balance_correction(req.rpm, &bal_cfg, material, era);

    Json(json!({
        "rpm": req.rpm,
        "material_id": req.material_id,
        "era_id": req.era_id,
        "initial_unbalance_um": bal_cfg.initial_residual_unbalance_m * 1_000_000.0,
        "target_unbalance_um": bal_cfg.target_residual_unbalance_m * 1_000_000.0,
        "result": {
            "residual_unbalance_um": result.residual_unbalance_m * 1_000_000.0,
            "correction_weight_grams": result.correction_weight_grams,
            "correction_angle_deg": result.correction_angle_deg,
            "vibration_before_mm": result.vibration_before_mm,
            "vibration_after_mm": result.vibration_after_mm,
            "vibration_reduction_pct": result.vibration_reduction_pct,
            "steps_taken": result.steps_taken,
            "success": result.success,
            "critical_rpm_improvement_pct": result.critical_rpm_improvement_pct,
        },
        "calibration_weights_available": bal_cfg.calibration_weights_grams,
    }))
}
