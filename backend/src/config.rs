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
    pub material_profiles: MaterialProfiles,
    pub era_profiles: EraProfiles,
    pub balance_correction: BalanceCorrectionConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialProfile {
    pub material_id: String,
    pub display_name: String,
    pub density_kg_m3: f64,
    pub youngs_modulus_pa: f64,
    pub yield_strength_pa: f64,
    pub thermal_expansion_per_c: f64,
    pub damping_ratio_factor: f64,
    pub surface_friction_coeff: f64,
    pub quality_factor: f64,
    pub color_hex: String,
    pub era_compatibility: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialProfiles {
    pub wood: MaterialProfile,
    pub copper: MaterialProfile,
    pub iron: MaterialProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraProfile {
    pub era_id: String,
    pub display_name: String,
    pub era_year: String,
    pub description: String,
    pub default_material: String,
    pub base_rpm_min: f64,
    pub base_rpm_max: f64,
    pub typical_rpm: f64,
    pub unbalance_tolerance_m: f64,
    pub surface_roughness_factor: f64,
    pub manufacturing_precision_factor: f64,
    pub bearing_technology: String,
    pub typical_yarn: String,
    pub rpm_scaling_factor: f64,
    pub shaft_length_factor: f64,
    pub shaft_diameter_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EraProfiles {
    pub ancient_yuan: EraProfile,
    pub modern_high_speed: EraProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceCorrectionConfig {
    pub max_correction_weight_grams: f64,
    pub max_correction_angle_deg: f64,
    pub balance_planes: u32,
    pub initial_residual_unbalance_m: f64,
    pub target_residual_unbalance_m: f64,
    pub correction_step_fraction: f64,
    pub vibration_reduction_expectation: f64,
    pub calibration_weights_grams: Vec<f64>,
}

impl MaterialProfile {
    pub fn apply_to_rotor(&self, base: &RotorDynamicsConfig) -> RotorDynamicsConfig {
        let volume = std::f64::consts::PI
            * (base.shaft_diameter_m / 2.0).powi(2)
            * base.shaft_length_m;
        let actual_mass = self.density_kg_m3 * volume;
        RotorDynamicsConfig {
            mass_kg: actual_mass,
            shaft_length_m: base.shaft_length_m,
            shaft_diameter_m: base.shaft_diameter_m,
            unbalance_eccentricity_m: base.unbalance_eccentricity_m,
            damping_ratio: base.damping_ratio * self.damping_ratio_factor,
            youngs_modulus_pa: self.youngs_modulus_pa,
            gravity_mps2: base.gravity_mps2,
        }
    }

    pub fn quality_impact(&self) -> f64 {
        self.quality_factor
    }
}

impl EraProfile {
    pub fn apply_to_rotor(&self, material_profile: &MaterialProfile, base: &RotorDynamicsConfig) -> RotorDynamicsConfig {
        let mat_rotor = material_profile.apply_to_rotor(base);
        RotorDynamicsConfig {
            shaft_length_m: mat_rotor.shaft_length_m * self.shaft_length_factor,
            shaft_diameter_m: mat_rotor.shaft_diameter_m * self.shaft_diameter_factor,
            unbalance_eccentricity_m: mat_rotor.unbalance_eccentricity_m * self.manufacturing_precision_factor,
            ..mat_rotor
        }
    }

    pub fn apply_to_bearing(&self, base: &OilFilmBearingConfig) -> OilFilmBearingConfig {
        let clearance_scale = self.manufacturing_precision_factor;
        let damping_scale = self.surface_roughness_factor;
        OilFilmBearingConfig {
            radial_clearance_m: base.radial_clearance_m * clearance_scale,
            nonlinear_damping_alpha: base.nonlinear_damping_alpha * damping_scale,
            ..base.clone()
        }
    }
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
