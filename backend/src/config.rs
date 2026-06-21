use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_ENV: &str = "APP_CONFIG_PATH";
const DEFAULT_CONFIG_PATH: &str = "config/app_config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub mqtt: MqttConfig,
    pub clickhouse: ClickHouseConfig,
    pub api: ApiConfig,
    pub rotor_dynamics: RotorDynamicsConfig,
    pub oil_film_bearing: OilFilmBearingConfig,
    pub regression_model: RegressionModelConfig,
    pub validation: ValidationConfig,
    pub alert_thresholds: AlertThresholdsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub subscriber_client_id: String,
    pub alert_publisher_client_id: String,
    pub sensor_topic: String,
    pub alert_topic: String,
    pub keep_alive_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseConfig {
    pub base_url: String,
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub bind_address: String,
    pub bind_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotorDynamicsConfig {
    pub mass_kg: f64,
    pub shaft_length_m: f64,
    pub shaft_diameter_m: f64,
    pub unbalance_eccentricity_m: f64,
    pub damping_ratio: f64,
    pub youngs_modulus_pa: f64,
    pub gravity_mps2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OilFilmBearingConfig {
    pub viscosity_pa_s: f64,
    pub bearing_length_m: f64,
    pub bearing_diameter_m: f64,
    pub bearing_radius_m: f64,
    pub radial_clearance_m: f64,
    pub nonlinear_damping_alpha: f64,
    pub whirl_threshold_ratio: f64,
    pub max_amplitude_growth: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionModelConfig {
    pub target_twist_per_meter: f64,
    pub initial_uniformity_coeffs: UniformityCoeffs,
    pub initial_strength_coeffs: StrengthCoeffs,
    pub lms_learning_rate: f64,
    pub wear_energy_coefficient: f64,
    pub wear_time_coefficient: f64,
    pub wear_max_coefficient: f64,
    pub calibration_window_size: usize,
    pub vibration_impact_lambda: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniformityCoeffs {
    pub beta0: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub beta3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrengthCoeffs {
    pub alpha0: f64,
    pub alpha1: f64,
    pub alpha2: f64,
    pub alpha3: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub rpm_min: f64,
    pub rpm_max: f64,
    pub vibration_min_mm: f64,
    pub vibration_max_mm: f64,
    pub temperature_min_c: f64,
    pub temperature_max_c: f64,
    pub twist_min_per_meter: f64,
    pub twist_max_per_meter: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholdsConfig {
    pub vibration_warning_mm: f64,
    pub vibration_critical_mm: f64,
    pub twist_variance_warning: f64,
    pub twist_variance_critical: f64,
    pub critical_speed_tolerance_pct: f64,
    pub temperature_warning_c: f64,
    pub temperature_critical_c: f64,
}

impl AppConfig {
    pub fn default_path() -> PathBuf {
        std::env::var(DEFAULT_CONFIG_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH))
    }

    pub fn load(path: &str) -> anyhow::Result<Self> {
        let p = Path::new(path);
        let content = std::fs::read_to_string(p)?;
        let cfg: AppConfig = serde_json::from_str(&content)?;
        Ok(cfg)
    }

    pub fn load_default() -> anyhow::Result<Self> {
        let p = Self::default_path();
        Self::load(p.to_str().expect("config path should be valid utf-8"))
    }
}
