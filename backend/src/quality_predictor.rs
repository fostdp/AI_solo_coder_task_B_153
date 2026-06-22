use crate::config::{EraProfile, MaterialProfile, RegressionModelConfig};
use crate::metrics::Metrics;
use rand::Rng;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Clone, Debug)]
pub struct YarnQualityResult {
    pub predicted_uniformity: f64,
    pub predicted_strength: f64,
    pub twist_variance: f64,
    pub vibration_impact_factor: f64,
    pub wear_coefficient: f64,
    pub calibration_error: f64,
    pub sample_count: u64,
    pub beta0: f64,
    pub beta1: f64,
    pub alpha0: f64,
    pub alpha1: f64,
    pub material_boost: f64,
    pub era_boost: f64,
    pub balance_recovery: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct MaterialComparisonResult {
    pub material_id: String,
    pub display_name: String,
    pub critical_rpm: f64,
    pub total_displacement_mm: f64,
    pub uniformity: f64,
    pub strength: f64,
    pub whirl_risk: f64,
    pub cost_index: f64,
    pub relative_density: f64,
    pub damping_ratio_factor: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct EraComparisonResult {
    pub era_id: String,
    pub display_name: String,
    pub typical_rpm: f64,
    pub critical_rpm: f64,
    pub total_displacement_mm: f64,
    pub uniformity: f64,
    pub strength: f64,
    pub daily_output_kg: f64,
    pub manufacturing_precision: f64,
    pub bearing_technology: String,
}

#[derive(Clone, Debug)]
pub struct CalibrationSample {
    pub vibration_amplitude: f64,
    pub twist_per_meter: f64,
    pub measured_uniformity: f64,
    pub measured_strength: f64,
    pub timestamp_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct CalibrationState {
    pub wear_coefficient: f64,
    pub beta0: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub beta3: f64,
    pub alpha0: f64,
    pub alpha1: f64,
    pub alpha2: f64,
    pub alpha3: f64,
    pub cumulative_vibration_energy: f64,
    pub total_runtime_seconds: f64,
    pub sample_count: u64,
    pub last_prediction_error: f64,
}

pub struct OnlineCalibrator {
    cfg: RegressionModelConfig,
    state: CalibrationState,
    window: VecDeque<CalibrationSample>,
    last_timestamp: f64,
}

impl OnlineCalibrator {
    pub fn new(cfg: RegressionModelConfig) -> Self {
        let window_cap = cfg.calibration_window_size;
        let state = CalibrationState {
            wear_coefficient: 0.0,
            beta0: cfg.initial_uniformity_coeffs.beta0,
            beta1: cfg.initial_uniformity_coeffs.beta1,
            beta2: cfg.initial_uniformity_coeffs.beta2,
            beta3: cfg.initial_uniformity_coeffs.beta3,
            alpha0: cfg.initial_strength_coeffs.alpha0,
            alpha1: cfg.initial_strength_coeffs.alpha1,
            alpha2: cfg.initial_strength_coeffs.alpha2,
            alpha3: cfg.initial_strength_coeffs.alpha3,
            cumulative_vibration_energy: 0.0,
            total_runtime_seconds: 0.0,
            sample_count: 0,
            last_prediction_error: 0.0,
        };
        Self {
            cfg,
            state,
            window: VecDeque::with_capacity(window_cap),
            last_timestamp: 0.0,
        }
    }

    pub fn state(&self) -> &CalibrationState {
        &self.state
    }

    pub fn add_sample(&mut self, sample: CalibrationSample, metrics: Option<&Arc<Metrics>>) {
        if self.last_timestamp > 0.0 {
            let dt = (sample.timestamp_seconds - self.last_timestamp).max(0.0);
            self.state.total_runtime_seconds += dt;
            self.state.cumulative_vibration_energy +=
                sample.vibration_amplitude.powi(2) * dt;
        }
        self.last_timestamp = sample.timestamp_seconds;

        let cap = self.cfg.calibration_window_size;
        self.window.push_back(sample);
        if self.window.len() > cap {
            self.window.pop_front();
        }

        self.state.sample_count += 1;
        self.update_wear_coefficient();
        self.lms_update(metrics);
    }

    fn update_wear_coefficient(&mut self) {
        let energy_term = self.cfg.wear_energy_coefficient
            * self.state.cumulative_vibration_energy.sqrt();
        let time_term = self.cfg.wear_time_coefficient * self.state.total_runtime_seconds;
        self.state.wear_coefficient = (energy_term + time_term).min(self.cfg.wear_max_coefficient);
    }

    fn lms_update(&mut self, metrics: Option<&Arc<Metrics>>) {
        if self.window.len() < 10 {
            return;
        }
        let target_twist = self.cfg.target_twist_per_meter;
        let lr = self.cfg.lms_learning_rate;
        let recent: Vec<_> = self.window.iter().rev().take(50).collect();

        for sample in &recent {
            let twist_var = (sample.twist_per_meter - target_twist).abs() / target_twist;

            let pred_uniformity = self.state.beta0
                + self.state.beta1 * sample.vibration_amplitude
                + self.state.beta2 * twist_var
                + self.state.beta3 * sample.vibration_amplitude * twist_var;
            let error_u = sample.measured_uniformity - pred_uniformity;
            self.state.last_prediction_error = error_u;
            let wear_factor = 1.0 + self.state.wear_coefficient;

            self.state.beta0 += lr * error_u;
            self.state.beta1 += lr * error_u * sample.vibration_amplitude * wear_factor;
            self.state.beta2 += lr * error_u * twist_var;
            self.state.beta3 += lr * error_u * sample.vibration_amplitude * twist_var;

            let twist_factor = sample.twist_per_meter / 100.0;
            let pred_strength = self.state.alpha0
                + self.state.alpha1 * twist_factor
                + self.state.alpha2 * sample.vibration_amplitude
                + self.state.alpha3 * twist_factor * twist_factor;
            let error_s = sample.measured_strength - pred_strength;

            self.state.alpha0 += lr * error_s;
            self.state.alpha1 += lr * error_s * twist_factor;
            self.state.alpha2 += lr * error_s * sample.vibration_amplitude * wear_factor;
            self.state.alpha3 += lr * error_s * twist_factor * twist_factor;
        }
        if let Some(m) = metrics {
            m.lms_updates_total.inc();
        }

        self.state.beta1 = self.state.beta1.clamp(-5.0, 0.0);
        self.state.beta2 = self.state.beta2.clamp(-2.0, 0.0);
        self.state.beta3 = self.state.beta3.clamp(-0.5, 0.0);
        self.state.alpha1 = self.state.alpha1.clamp(0.0, 0.2);
        self.state.alpha2 = self.state.alpha2.clamp(-5.0, 0.0);
        self.state.alpha3 = self.state.alpha3.clamp(-0.0001, 0.0);
    }

    pub fn predict(&self, vibration_amplitude: f64, twist_per_meter: f64) -> (f64, f64, f64, f64) {
        let target_twist = self.cfg.target_twist_per_meter;
        let twist_var = (twist_per_meter - target_twist).abs() / target_twist;
        let wear_penalty = self.state.wear_coefficient * 3.0;

        let predicted_uniformity = self.state.beta0
            + self.state.beta1 * vibration_amplitude
            + self.state.beta2 * twist_var
            + self.state.beta3 * vibration_amplitude * twist_var
            - wear_penalty;

        let twist_factor = twist_per_meter / 100.0;
        let predicted_strength = self.state.alpha0
            + self.state.alpha1 * twist_factor
            + self.state.alpha2 * vibration_amplitude
            + self.state.alpha3 * twist_factor * twist_factor
            - wear_penalty * 0.5;

        (
            predicted_uniformity,
            predicted_strength,
            twist_var,
            self.state.wear_coefficient,
        )
    }
}

pub fn simulate_measured_values(
    cfg: &RegressionModelConfig,
    vibration_amplitude: f64,
    twist_per_meter: f64,
    wear_coeff: f64,
    noise: f64,
) -> (f64, f64) {
    let target_twist = cfg.target_twist_per_meter;
    let twist_var = (twist_per_meter - target_twist).abs() / target_twist;
    let wear_penalty = wear_coeff * 5.0;
    let b = &cfg.initial_uniformity_coeffs;
    let a = &cfg.initial_strength_coeffs;

    let true_uniformity = b.beta0
        + (b.beta1 - 0.1) * vibration_amplitude
        + (b.beta2 - 0.05) * twist_var
        + (b.beta3 - 0.01) * vibration_amplitude * twist_var
        - wear_penalty
        + noise;

    let twist_factor = twist_per_meter / 100.0;
    let true_strength = a.alpha0
        + (a.alpha1 + 0.005) * twist_factor
        + (a.alpha2 - 0.1) * vibration_amplitude
        + (a.alpha3 - 0.000005) * twist_factor * twist_factor
        - wear_penalty * 0.5
        + noise * 0.3;

    (true_uniformity.max(0.0), true_strength.max(0.0))
}

pub struct QualityPredictor {
    cfg: RegressionModelConfig,
    calibrators: Mutex<HashMap<String, OnlineCalibrator>>,
    metrics: Arc<Metrics>,
}

impl QualityPredictor {
    pub fn new(cfg: RegressionModelConfig, metrics: Arc<Metrics>) -> Self {
        Self {
            cfg,
            calibrators: Mutex::new(HashMap::new()),
            metrics,
        }
    }

    pub fn predict(
        &self,
        spindle_id: &str,
        vibration_amplitude: f64,
        twist_per_meter: f64,
        timestamp_seconds: f64,
    ) -> YarnQualityResult {
        let mut map = self.calibrators.lock().unwrap();
        let calibrator = map
            .entry(spindle_id.to_string())
            .or_insert_with(|| OnlineCalibrator::new(self.cfg.clone()));

        let (_pred_uniformity_base, _pred_strength_base, _twist_variance, wear_coeff) =
            calibrator.predict(vibration_amplitude, twist_per_meter);

        let state = calibrator.state().clone();
        drop(state);

        let mut rng = rand::thread_rng();
        let noise: f64 = rng.gen_range(-1.0..1.0) * 0.5;
        let (measured_uniformity, measured_strength) = simulate_measured_values(
            &self.cfg,
            vibration_amplitude,
            twist_per_meter,
            wear_coeff,
            noise,
        );

        let sample = CalibrationSample {
            vibration_amplitude,
            twist_per_meter,
            measured_uniformity,
            measured_strength,
            timestamp_seconds,
        };
        calibrator.add_sample(sample, Some(&self.metrics));

        let state = calibrator.state().clone();
        let (predicted_uniformity, predicted_strength, twist_variance, wear_coefficient) =
            calibrator.predict(vibration_amplitude, twist_per_meter);

        let lambda = self.cfg.vibration_impact_lambda;
        let vibration_impact_factor = 1.0 - (-lambda * vibration_amplitude).exp();

        self.metrics.quality_predictions_total.inc();

        YarnQualityResult {
            predicted_uniformity: predicted_uniformity.max(0.0),
            predicted_strength: predicted_strength.max(0.0),
            twist_variance,
            vibration_impact_factor,
            wear_coefficient,
            calibration_error: state.last_prediction_error.abs(),
            sample_count: state.sample_count,
            beta0: state.beta0,
            beta1: state.beta1,
            alpha0: state.alpha0,
            alpha1: state.alpha1,
            material_boost: 1.0,
            era_boost: 1.0,
            balance_recovery: 0.0,
        }
    }

    pub fn predict_with_context(
        &self,
        spindle_id: &str,
        vibration_amplitude: f64,
        twist_per_meter: f64,
        timestamp_seconds: f64,
        material: Option<&MaterialProfile>,
        era: Option<&EraProfile>,
        balance_correction_fraction: Option<f64>,
    ) -> YarnQualityResult {
        let material_boost = material.map(|m| m.quality_factor).unwrap_or(1.0);
        let era_boost = era
            .map(|e| 1.0 / e.manufacturing_precision_factor.sqrt().max(0.5))
            .unwrap_or(1.0);
        let effective_twist = if let Some(e) = era {
            twist_per_meter * (1.0 + (e.rpm_scaling_factor - 1.0) * 0.1)
        } else {
            twist_per_meter
        };
        let balance_recovery = balance_correction_fraction.unwrap_or(0.0);
        let effective_vib = vibration_amplitude * (1.0 - balance_recovery * 0.7);

        let mut result = self.predict(spindle_id, effective_vib, effective_twist, timestamp_seconds);
        result.predicted_uniformity *= material_boost * era_boost;
        result.predicted_strength *= material_boost * era_boost;
        result.material_boost = material_boost;
        result.era_boost = era_boost;
        result.balance_recovery = balance_recovery;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        EraProfile, MaterialProfile, StrengthCoeffs, RegressionModelConfig, UniformityCoeffs,
    };
    use crate::metrics::Metrics;
    use std::sync::{Arc, OnceLock};

    static METRICS: OnceLock<Arc<Metrics>> = OnceLock::new();
    fn shared_metrics() -> Arc<Metrics> {
        METRICS.get_or_init(|| Metrics::new().unwrap()).clone()
    }

    fn cfg() -> RegressionModelConfig {
        RegressionModelConfig {
            target_twist_per_meter: 800.0,
            initial_uniformity_coeffs: UniformityCoeffs {
                beta0: 95.0,
                beta1: -0.8,
                beta2: -0.3,
                beta3: -0.05,
            },
            initial_strength_coeffs: StrengthCoeffs {
                alpha0: 15.0,
                alpha1: 0.02,
                alpha2: -1.5,
                alpha3: -0.00001,
            },
            lms_learning_rate: 0.01,
            wear_energy_coefficient: 1e-9,
            wear_time_coefficient: 2e-10,
            wear_max_coefficient: 0.3,
            calibration_window_size: 200,
            vibration_impact_lambda: 2.0,
        }
    }

    fn material_iron() -> MaterialProfile {
        MaterialProfile {
            material_id: "iron".into(),
            display_name: "".into(),
            density_kg_m3: 7850.0,
            youngs_modulus_pa: 210e9,
            yield_strength_pa: 0.0,
            thermal_expansion_per_c: 0.0,
            damping_ratio_factor: 1.0,
            surface_friction_coeff: 0.0,
            quality_factor: 1.0,
            color_hex: "".into(),
            era_compatibility: vec![],
            data_source: "测试基准".into(),
            experimental_uncertainty_pct: 0.0,
            notes: "".into(),
        }
    }

    fn material_wood() -> MaterialProfile {
        MaterialProfile {
            material_id: "wood".into(),
            display_name: "".into(),
            density_kg_m3: 750.0,
            youngs_modulus_pa: 10e9,
            yield_strength_pa: 0.0,
            thermal_expansion_per_c: 0.0,
            damping_ratio_factor: 3.5,
            surface_friction_coeff: 0.0,
            quality_factor: 0.85,
            color_hex: "".into(),
            era_compatibility: vec![],
            data_source: "测试基准".into(),
            experimental_uncertainty_pct: 0.0,
            notes: "".into(),
        }
    }

    fn era_ancient() -> EraProfile {
        EraProfile {
            era_id: "ancient".into(),
            display_name: "".into(),
            era_year: "".into(),
            description: "".into(),
            default_material: "wood".into(),
            base_rpm_min: 200.0,
            base_rpm_max: 800.0,
            typical_rpm: 500.0,
            unbalance_tolerance_m: 0.0,
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

    fn era_modern() -> EraProfile {
        EraProfile {
            era_id: "modern".into(),
            display_name: "".into(),
            era_year: "".into(),
            description: "".into(),
            default_material: "iron".into(),
            base_rpm_min: 8000.0,
            base_rpm_max: 25000.0,
            typical_rpm: 18000.0,
            unbalance_tolerance_m: 0.0,
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

    mod predict_with_context_tests {
        use super::*;

        #[test]
        fn test_iron_material_boost_is_one() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let result = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&material_iron()), None, None);
            assert!((result.material_boost - 1.0).abs() < 1e-6);
        }

        #[test]
        fn test_wood_lowers_quality_vs_iron() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let iron = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&material_iron()), None, None);
            let wood = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&material_wood()), None, None);
            assert!(wood.predicted_uniformity < iron.predicted_uniformity);
            assert!(wood.predicted_strength < iron.predicted_strength);
        }

        #[test]
        fn test_modern_era_boosts_quality() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let ancient = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&material_iron()), Some(&era_ancient()), None);
            let modern = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&material_iron()), Some(&era_modern()), None);
            assert!(modern.era_boost > ancient.era_boost);
            assert!(modern.predicted_uniformity > ancient.predicted_uniformity);
        }

        #[test]
        fn test_balance_correction_reduces_vibration_impact() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let without = qp.predict_with_context("s1", 0.5, 800.0, 1000.0, None, None, Some(0.0));
            let with = qp.predict_with_context("s1", 0.5, 800.0, 1000.0, None, None, Some(0.8));
            assert!(with.balance_recovery == 0.8);
            assert!(with.predicted_uniformity >= without.predicted_uniformity);
        }

        #[test]
        fn test_balance_recovery_boundary_zero() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let r = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, None, None, Some(0.0));
            assert_eq!(r.balance_recovery, 0.0);
        }

        #[test]
        fn test_balance_recovery_boundary_one() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let r = qp.predict_with_context("s1", 0.5, 800.0, 1000.0, None, None, Some(1.0));
            assert!(r.balance_recovery == 1.0);
            assert!(r.predicted_uniformity > 0.0);
        }

        #[test]
        fn test_higher_vibration_lowers_uniformity() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let low = qp.predict_with_context("s1", 0.01, 800.0, 1000.0, None, None, None);
            let high = qp.predict_with_context("s1", 0.5, 800.0, 1000.0, None, None, None);
            assert!(low.predicted_uniformity > high.predicted_uniformity);
        }

        #[test]
        fn test_quality_outputs_are_nonnegative() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            for vib in [0.0, 0.01, 0.1, 0.5, 1.0, 5.0] {
                for twist in [100.0, 800.0, 2000.0] {
                    let r = qp.predict_with_context("s", vib, twist, 1000.0, None, None, None);
                    assert!(r.predicted_uniformity >= 0.0, "uniformity {} negative (vib={})", r.predicted_uniformity, vib);
                    assert!(r.predicted_strength >= 0.0);
                    assert!(r.vibration_impact_factor >= 0.0);
                    assert!(r.wear_coefficient >= 0.0);
                }
            }
        }

        #[test]
        fn test_material_era_combined_is_deterministic() {
            let qp = QualityPredictor::new(cfg(), shared_metrics());
            let iron = material_iron();
            let modern = era_modern();
            let r1 = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&iron), Some(&modern), Some(0.5));
            let r2 = qp.predict_with_context("s1", 0.1, 800.0, 1000.0, Some(&iron), Some(&modern), Some(0.5));
            assert!((r1.predicted_uniformity - r2.predicted_uniformity).abs() < 1e-6);
        }
    }
}

pub async fn run_quality_service(
    predictor: std::sync::Arc<QualityPredictor>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(
        String,
        f64,
        f64,
        f64,
    )>,
    tx: tokio::sync::mpsc::UnboundedSender<(String, YarnQualityResult)>,
) {
    while let Some((spindle_id, vib, twist, ts)) = rx.recv().await {
        let result = predictor.predict(&spindle_id, vib, twist, ts);
        if let Err(e) = tx.send((spindle_id, result)) {
            tracing::error!("Quality service send error: {}", e);
        }
    }
}
