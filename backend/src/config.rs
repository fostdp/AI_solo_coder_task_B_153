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
    #[serde(default = "default_pool_size")]
    pub vibration_pool_size: Option<u32>,
}

fn default_pool_size() -> Option<u32> {
    Some(4)
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
    #[serde(default = "default_data_source")]
    pub data_source: String,
    #[serde(default)]
    pub experimental_uncertainty_pct: f64,
    #[serde(default)]
    pub notes: String,
}

fn default_data_source() -> String {
    "工程估算".into()
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
    #[serde(default = "default_standard_reference")]
    pub standard_reference: String,
    #[serde(default = "default_balance_grade")]
    pub balance_quality_grade: String,
    #[serde(default)]
    pub standard_source: String,
}

fn default_standard_reference() -> String {
    "企业经验值".into()
}
fn default_balance_grade() -> String {
    "G6.3".into()
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
        let new_shaft_length = mat_rotor.shaft_length_m * self.shaft_length_factor;
        let new_shaft_diameter = mat_rotor.shaft_diameter_m * self.shaft_diameter_factor;
        let volume = std::f64::consts::PI * (new_shaft_diameter / 2.0).powi(2) * new_shaft_length;
        let new_mass = material_profile.density_kg_m3 * volume;
        RotorDynamicsConfig {
            shaft_length_m: new_shaft_length,
            shaft_diameter_m: new_shaft_diameter,
            unbalance_eccentricity_m: mat_rotor.unbalance_eccentricity_m * self.manufacturing_precision_factor,
            mass_kg: new_mass,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_rotor() -> RotorDynamicsConfig {
        RotorDynamicsConfig {
            mass_kg: 0.5,
            shaft_length_m: 0.3,
            shaft_diameter_m: 0.008,
            unbalance_eccentricity_m: 0.0001,
            damping_ratio: 0.02,
            youngs_modulus_pa: 210_000_000_000.0,
            gravity_mps2: 9.81,
        }
    }

    fn base_bearing() -> OilFilmBearingConfig {
        OilFilmBearingConfig {
            viscosity_pa_s: 0.01,
            bearing_length_m: 0.02,
            bearing_diameter_m: 0.016,
            bearing_radius_m: 0.008,
            radial_clearance_m: 0.00005,
            nonlinear_damping_alpha: 5_000_000.0,
            whirl_threshold_ratio: 0.55,
            max_amplitude_growth: 8.0,
        }
    }

    fn iron_profile() -> MaterialProfile {
        MaterialProfile {
            material_id: "iron".into(),
            display_name: "钢铁".into(),
            density_kg_m3: 7850.0,
            youngs_modulus_pa: 210_000_000_000.0,
            yield_strength_pa: 350_000_000.0,
            thermal_expansion_per_c: 0.000012,
            damping_ratio_factor: 1.0,
            surface_friction_coeff: 0.08,
            quality_factor: 1.0,
            color_hex: "#8899aa".into(),
            era_compatibility: vec!["modern_high_speed".into()],
            data_source: "测试基准".into(),
            experimental_uncertainty_pct: 0.0,
            notes: "".into(),
        }
    }

    fn wood_profile() -> MaterialProfile {
        MaterialProfile {
            material_id: "wood".into(),
            display_name: "铁木".into(),
            density_kg_m3: 750.0,
            youngs_modulus_pa: 10_000_000_000.0,
            yield_strength_pa: 60_000_000.0,
            thermal_expansion_per_c: 0.000005,
            damping_ratio_factor: 3.5,
            surface_friction_coeff: 0.35,
            quality_factor: 0.85,
            color_hex: "#6b4423".into(),
            era_compatibility: vec!["ancient_yuan".into()],
            data_source: "测试基准".into(),
            experimental_uncertainty_pct: 0.0,
            notes: "".into(),
        }
    }

    fn copper_profile() -> MaterialProfile {
        MaterialProfile {
            material_id: "copper".into(),
            display_name: "青铜".into(),
            density_kg_m3: 8960.0,
            youngs_modulus_pa: 120_000_000_000.0,
            yield_strength_pa: 200_000_000.0,
            thermal_expansion_per_c: 0.000017,
            damping_ratio_factor: 1.8,
            surface_friction_coeff: 0.15,
            quality_factor: 0.92,
            color_hex: "#b87333".into(),
            era_compatibility: vec!["ancient_yuan".into(), "modern_high_speed".into()],
            data_source: "测试基准".into(),
            experimental_uncertainty_pct: 0.0,
            notes: "".into(),
        }
    }

    fn ancient_era() -> EraProfile {
        EraProfile {
            era_id: "ancient_yuan".into(),
            display_name: "元代".into(),
            era_year: "1280".into(),
            description: "".into(),
            default_material: "wood".into(),
            base_rpm_min: 200.0,
            base_rpm_max: 800.0,
            typical_rpm: 500.0,
            unbalance_tolerance_m: 0.0005,
            surface_roughness_factor: 2.5,
            manufacturing_precision_factor: 5.0,
            bearing_technology: "".into(),
            typical_yarn: "".into(),
            rpm_scaling_factor: 0.25,
            shaft_length_factor: 1.2,
            shaft_diameter_factor: 1.5,
            standard_reference: "测试基准".into(),
            balance_quality_grade: "G40".into(),
            standard_source: "".into(),
        }
    }

    fn modern_era() -> EraProfile {
        EraProfile {
            era_id: "modern_high_speed".into(),
            display_name: "现代".into(),
            era_year: "2024".into(),
            description: "".into(),
            default_material: "iron".into(),
            base_rpm_min: 8000.0,
            base_rpm_max: 25000.0,
            typical_rpm: 18000.0,
            unbalance_tolerance_m: 0.000002,
            surface_roughness_factor: 0.3,
            manufacturing_precision_factor: 0.05,
            bearing_technology: "".into(),
            typical_yarn: "".into(),
            rpm_scaling_factor: 10.0,
            shaft_length_factor: 0.8,
            shaft_diameter_factor: 0.7,
            standard_reference: "测试基准".into(),
            balance_quality_grade: "G2.5".into(),
            standard_source: "".into(),
        }
    }

    fn balance_cfg() -> BalanceCorrectionConfig {
        BalanceCorrectionConfig {
            max_correction_weight_grams: 50.0,
            max_correction_angle_deg: 360.0,
            balance_planes: 2,
            initial_residual_unbalance_m: 0.0001,
            target_residual_unbalance_m: 0.000001,
            correction_step_fraction: 0.15,
            vibration_reduction_expectation: 0.7,
            calibration_weights_grams: vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0],
        }
    }

    mod material_tests {
        use super::*;

        #[test]
        fn test_iron_density_should_reflect_steel() {
            let m = iron_profile();
            assert!((m.density_kg_m3 - 7850.0).abs() < 1e-6, "钢铁密度应接近7850kg/m³");
        }

        #[test]
        fn test_wood_should_have_lower_density_than_iron() {
            let wood = wood_profile();
            let iron = iron_profile();
            assert!(wood.density_kg_m3 < iron.density_kg_m3, "木材密度应低于钢铁");
            assert!(wood.density_kg_m3 < 1000.0, "木材应能浮于水 (<1000kg/m³)");
        }

        #[test]
        fn test_iron_youngs_modulus_should_be_highest() {
            let iron = iron_profile();
            let copper = copper_profile();
            let wood = wood_profile();
            assert!(iron.youngs_modulus_pa > copper.youngs_modulus_pa);
            assert!(copper.youngs_modulus_pa > wood.youngs_modulus_pa);
        }

        #[test]
        fn test_wood_damping_should_be_highest() {
            let wood = wood_profile();
            let copper = copper_profile();
            let iron = iron_profile();
            assert!(wood.damping_ratio_factor > copper.damping_ratio_factor);
            assert!(copper.damping_ratio_factor > iron.damping_ratio_factor);
            assert_eq!(iron.damping_ratio_factor, 1.0);
        }

        #[test]
        fn test_material_apply_preserves_shaft_length() {
            let iron = iron_profile();
            let base = base_rotor();
            let applied = iron.apply_to_rotor(&base);
            assert_eq!(applied.shaft_length_m, base.shaft_length_m);
            assert_eq!(applied.shaft_diameter_m, base.shaft_diameter_m);
        }

        #[test]
        fn test_material_apply_changes_density_based_mass() {
            let base = base_rotor();
            let iron = iron_profile();
            let wood = wood_profile();
            let applied_iron = iron.apply_to_rotor(&base);
            let applied_wood = wood.apply_to_rotor(&base);
            let volume = std::f64::consts::PI
                * (base.shaft_diameter_m / 2.0).powi(2)
                * base.shaft_length_m;
            assert!(
                (applied_iron.mass_kg - iron.density_kg_m3 * volume).abs() < 1e-6,
                "钢铁质量应等于密度×体积"
            );
            assert!(
                applied_wood.mass_kg < applied_iron.mass_kg,
                "木材锭质量应小于钢铁锭"
            );
        }

        #[test]
        fn test_material_apply_uses_material_youngs_modulus() {
            let base = base_rotor();
            let wood = wood_profile();
            let applied = wood.apply_to_rotor(&base);
            assert_eq!(applied.youngs_modulus_pa, wood.youngs_modulus_pa);
        }

        #[test]
        fn test_material_quality_factor_in_reasonable_range() {
            for m in &[iron_profile(), copper_profile(), wood_profile()] {
                assert!(
                    m.quality_factor > 0.0 && m.quality_factor <= 1.0,
                    "{} quality_factor 应在 (0, 1]",
                    m.material_id
                );
            }
        }

        #[test]
        fn test_material_era_compatibility() {
            let wood = wood_profile();
            assert!(wood.era_compatibility.contains(&"ancient_yuan".to_string()));
            assert!(!wood.era_compatibility.contains(&"modern_high_speed".to_string()));
        }
    }

    mod era_tests {
        use super::*;

        #[test]
        fn test_ancient_typical_rpm_in_range() {
            let a = ancient_era();
            assert!(a.typical_rpm >= a.base_rpm_min);
            assert!(a.typical_rpm <= a.base_rpm_max);
        }

        #[test]
        fn test_modern_rpm_much_higher_than_ancient() {
            let a = ancient_era();
            let m = modern_era();
            assert!(m.base_rpm_min > a.base_rpm_max * 5.0, "现代转速应至少是古代的5倍");
        }

        #[test]
        fn test_ancient_manufacturing_less_precise() {
            let a = ancient_era();
            let m = modern_era();
            assert!(a.manufacturing_precision_factor > m.manufacturing_precision_factor);
            assert!(a.manufacturing_precision_factor > 1.0);
            assert!(m.manufacturing_precision_factor < 1.0);
        }

        #[test]
        fn test_era_apply_scales_shaft_dimensions() {
            let base = base_rotor();
            let mat = iron_profile();
            let ancient = ancient_era();
            let applied = ancient.apply_to_rotor(&mat, &base);
            assert!(applied.shaft_length_m > base.shaft_length_m);
            assert!(applied.shaft_diameter_m > base.shaft_diameter_m);
        }

        #[test]
        fn test_era_apply_increases_unbalance_for_ancient() {
            let base = base_rotor();
            let mat = iron_profile();
            let ancient = ancient_era();
            let modern = modern_era();
            let a = ancient.apply_to_rotor(&mat, &base);
            let m = modern.apply_to_rotor(&mat, &base);
            assert!(a.unbalance_eccentricity_m > base.unbalance_eccentricity_m);
            assert!(m.unbalance_eccentricity_m < base.unbalance_eccentricity_m);
        }

        #[test]
        fn test_era_bearing_clearage_ancient_larger() {
            let bearing = base_bearing();
            let a = ancient_era().apply_to_bearing(&bearing);
            let m = modern_era().apply_to_bearing(&bearing);
            assert!(a.radial_clearance_m > bearing.radial_clearance_m);
            assert!(m.radial_clearance_m < bearing.radial_clearance_m);
        }

        #[test]
        fn test_era_surface_roughness_affects_nonlinear_damping() {
            let bearing = base_bearing();
            let a = ancient_era().apply_to_bearing(&bearing);
            let m = modern_era().apply_to_bearing(&bearing);
            assert!(a.nonlinear_damping_alpha > bearing.nonlinear_damping_alpha);
            assert!(m.nonlinear_damping_alpha < bearing.nonlinear_damping_alpha);
        }
    }

    mod balance_tests {
        use super::*;

        #[test]
        fn test_balance_config_default_values_sane() {
            let c = balance_cfg();
            assert!(c.initial_residual_unbalance_m > c.target_residual_unbalance_m);
            assert!(c.correction_step_fraction > 0.0 && c.correction_step_fraction <= 1.0);
            assert!(c.max_correction_weight_grams > 0.0);
            assert!(c.balance_planes >= 1);
        }

        #[test]
        fn test_calibration_weights_positive() {
            let c = balance_cfg();
            for w in &c.calibration_weights_grams {
                assert!(*w > 0.0);
            }
        }
    }

    mod config_load_tests {
        use super::*;

        #[test]
        fn test_default_config_file_exists_and_loads() {
            let cfg = AppConfig::load_default();
            assert!(cfg.is_ok(), "默认配置加载失败: {:?}", cfg.err());
            let cfg = cfg.unwrap();
            assert!(cfg.mqtt.broker_port > 0);
            assert!(cfg.rotor_dynamics.mass_kg > 0.0);
            assert_eq!(cfg.material_profiles.iron.material_id, "iron");
            assert_eq!(cfg.material_profiles.copper.material_id, "copper");
            assert_eq!(cfg.material_profiles.wood.material_id, "wood");
            assert_eq!(cfg.era_profiles.ancient_yuan.era_id, "ancient_yuan");
            assert_eq!(cfg.era_profiles.modern_high_speed.era_id, "modern_high_speed");
        }

        #[test]
        fn test_material_profiles_have_positive_density() {
            let cfg = AppConfig::load_default().unwrap();
            for m in &[
                &cfg.material_profiles.wood,
                &cfg.material_profiles.copper,
                &cfg.material_profiles.iron,
            ] {
                assert!(m.density_kg_m3 > 0.0, "材料 {} 密度必须为正", m.material_id);
                assert!(m.youngs_modulus_pa > 0.0);
            }
        }

        #[test]
        fn test_era_profiles_have_valid_rpm_ranges() {
            let cfg = AppConfig::load_default().unwrap();
            for e in &[&cfg.era_profiles.ancient_yuan, &cfg.era_profiles.modern_high_speed] {
                assert!(e.base_rpm_min > 0.0);
                assert!(e.base_rpm_max >= e.base_rpm_min);
                assert!(e.typical_rpm >= e.base_rpm_min);
                assert!(e.typical_rpm <= e.base_rpm_max);
            }
        }
    }
}
