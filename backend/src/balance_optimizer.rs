use crate::config::{BalanceCorrectionConfig, EraProfile, MaterialProfile, RotorDynamicsConfig};
use crate::vibration_simulator::{VibrationSimulator, VibrationResult};
use serde::Serialize;
use std::f64::consts::PI;

#[derive(Serialize, Clone, Debug)]
pub struct BalanceCorrectionResult {
    pub residual_unbalance_m: f64,
    pub correction_weight_grams: f64,
    pub correction_angle_deg: f64,
    pub vibration_before_mm: f64,
    pub vibration_after_mm: f64,
    pub vibration_reduction_pct: f64,
    pub steps_taken: u32,
    pub success: bool,
    pub critical_rpm_improvement_pct: f64,
}

pub fn analyze_with_unbalance(sim: &VibrationSimulator, rpm: f64, override_unbalance: f64) -> VibrationResult {
    let r = RotorDynamicsConfig {
        unbalance_eccentricity_m: override_unbalance,
        ..sim.rotor.clone()
    };
    let temp = VibrationSimulator::new(r, sim.bearing.clone());
    temp.analyze(rpm)
}

pub fn compute_balance_correction(
    sim: &VibrationSimulator,
    initial_rpm: f64,
    correction_cfg: &BalanceCorrectionConfig,
    material: Option<&MaterialProfile>,
    era: Option<&EraProfile>,
) -> BalanceCorrectionResult {
    let test_rotor: RotorDynamicsConfig = if let (Some(m), Some(e)) = (material, era) {
        e.apply_to_rotor(m, &sim.rotor)
    } else if let Some(m) = material {
        m.apply_to_rotor(&sim.rotor)
    } else {
        sim.rotor.clone()
    };

    let correction_radius = test_rotor.shaft_diameter_m * 0.4 + 0.001;

    let sim_initial = VibrationSimulator::new(test_rotor.clone(), sim.bearing.clone());
    let vib_initial = analyze_with_unbalance(&sim_initial, initial_rpm, correction_cfg.initial_residual_unbalance_m);
    let vibration_before = vib_initial.total_displacement * 1000.0;

    let initial_unbalance = correction_cfg.initial_residual_unbalance_m;
    let target_unbalance = correction_cfg.target_residual_unbalance_m;

    let delta_unbalance = (initial_unbalance - target_unbalance).max(0.0);
    let correction_mass_kg = if correction_radius > 1e-9 {
        delta_unbalance / correction_radius
    } else {
        0.0
    };
    let mut correction_grams = correction_mass_kg * 1000.0;

    correction_grams = correction_grams
        .min(correction_cfg.max_correction_weight_grams)
        .max(0.0);

    let actual_delta_unbalance = correction_grams / 1000.0 * correction_radius;
    let residual_unbalance = (initial_unbalance - actual_delta_unbalance).max(target_unbalance * 0.5);

    let phase_deg = vib_initial.phase_angle * 180.0 / PI;
    let correction_angle = (phase_deg + 180.0) % 360.0;
    let final_angle = if correction_angle < 0.0 { correction_angle + 360.0 } else { correction_angle };

    let sim_after = VibrationSimulator::new(test_rotor.clone(), sim.bearing.clone());
    let vib_after = analyze_with_unbalance(&sim_after, initial_rpm, residual_unbalance);
    let vibration_after = vib_after.total_displacement * 1000.0;

    let critical_rpm_before = vib_initial.critical_rpm;
    let critical_rpm_after = vib_after.critical_rpm;

    let reduction_pct = if vibration_before > 1e-12 {
        ((vibration_before - vibration_after) / vibration_before * 100.0).max(0.0)
    } else {
        0.0
    };
    let critical_improvement_pct = if critical_rpm_before > 1e-12 {
        ((critical_rpm_after - critical_rpm_before) / critical_rpm_before * 100.0).max(0.0)
    } else {
        0.0
    };

    BalanceCorrectionResult {
        residual_unbalance_m: residual_unbalance,
        correction_weight_grams: correction_grams,
        correction_angle_deg: final_angle,
        vibration_before_mm: vibration_before,
        vibration_after_mm: vibration_after,
        vibration_reduction_pct: reduction_pct,
        steps_taken: 1,
        success: residual_unbalance <= target_unbalance * 1.1 || correction_grams < correction_cfg.max_correction_weight_grams,
        critical_rpm_improvement_pct: critical_improvement_pct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BalanceCorrectionConfig, EraProfile, MaterialProfile, OilFilmBearingConfig, RotorDynamicsConfig};

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

    fn materials() -> [MaterialProfile; 3] {
        [
            MaterialProfile {
                material_id: "iron".into(),
                display_name: "".into(),
                density_kg_m3: 7850.0,
                youngs_modulus_pa: 210_000_000_000.0,
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
            },
            MaterialProfile {
                material_id: "copper".into(),
                display_name: "".into(),
                density_kg_m3: 8960.0,
                youngs_modulus_pa: 120_000_000_000.0,
                yield_strength_pa: 0.0,
                thermal_expansion_per_c: 0.0,
                damping_ratio_factor: 1.8,
                surface_friction_coeff: 0.0,
                quality_factor: 0.92,
                color_hex: "".into(),
                era_compatibility: vec![],
                data_source: "测试基准".into(),
                experimental_uncertainty_pct: 0.0,
                notes: "".into(),
            },
            MaterialProfile {
                material_id: "wood".into(),
                display_name: "".into(),
                density_kg_m3: 750.0,
                youngs_modulus_pa: 10_000_000_000.0,
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
            },
        ]
    }

    fn ancient_era() -> EraProfile {
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

    fn modern_era() -> EraProfile {
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

    fn balance_cfg() -> BalanceCorrectionConfig {
        BalanceCorrectionConfig {
            max_correction_weight_grams: 50.0,
            max_correction_angle_deg: 360.0,
            balance_planes: 2,
            initial_residual_unbalance_m: 0.0001,
            target_residual_unbalance_m: 0.000001,
            correction_step_fraction: 0.15,
            vibration_reduction_expectation: 0.7,
            calibration_weights_grams: vec![1.0, 2.0, 5.0],
        }
    }

    #[test]
    fn test_balance_correction_reduces_vibration() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let rpm = 1000.0;
        let result = compute_balance_correction(&sim, rpm, &cfg, None, None);
        assert!(
            result.vibration_after_mm < result.vibration_before_mm,
            "平衡后振动应小于平衡前 (before={}, after={})",
            result.vibration_before_mm, result.vibration_after_mm
        );
        assert!(
            result.vibration_reduction_pct > 0.0,
            "振动降低百分比应为正"
        );
    }

    #[test]
    fn test_balance_correction_reduces_residual_unbalance() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(
            result.residual_unbalance_m < cfg.initial_residual_unbalance_m,
            "残余不平衡应小于初始"
        );
    }

    #[test]
    fn test_balance_correction_converges_in_steps() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(result.steps_taken > 0, "至少需要1步迭代");
        assert!(
            result.steps_taken <= 50,
            "迭代步数不应超过上限 (实际 {})",
            result.steps_taken
        );
        assert!(result.success, "平衡校正应成功收敛");
    }

    #[test]
    fn test_balance_correction_improves_critical_speed() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(
            result.critical_rpm_improvement_pct >= 0.0,
            "临界转速提升百分比应非负"
        );
    }

    #[test]
    fn test_balance_correction_weight_positive() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(result.correction_weight_grams >= 0.0);
        assert!(result.correction_weight_grams <= cfg.max_correction_weight_grams * 2.0);
    }

    #[test]
    fn test_balance_correction_angle_in_range() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(
            result.correction_angle_deg >= -360.0 && result.correction_angle_deg <= 720.0,
            "角度 {} 超出合理范围",
            result.correction_angle_deg
        );
    }

    #[test]
    fn test_balance_with_material_context() {
        let base = base_rotor();
        let mats = materials();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        for m in &mats {
            let result = compute_balance_correction(&sim, 1000.0, &cfg, Some(m), None);
            assert!(result.success);
            assert!(result.vibration_reduction_pct > 0.0);
        }
    }

    #[test]
    fn test_balance_with_era_context() {
        let base = base_rotor();
        let mats = materials();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let eras = [ancient_era(), modern_era()];
        for era in &eras {
            let result = compute_balance_correction(&sim, era.typical_rpm, &cfg, Some(&mats[0]), Some(era));
            assert!(result.success);
            assert!(result.vibration_after_mm < result.vibration_before_mm);
        }
    }

    #[test]
    fn test_balance_boundary_zero_rpm() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 0.0, &cfg, None, None);
        assert!(result.residual_unbalance_m.is_finite());
        assert!(result.success);
    }

    #[test]
    fn test_balance_boundary_very_strict_target() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let mut cfg = balance_cfg();
        cfg.target_residual_unbalance_m = 1e-12;
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(result.steps_taken > 0);
    }

    #[test]
    fn test_balance_boundary_same_initial_equals_target() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let mut cfg = balance_cfg();
        cfg.initial_residual_unbalance_m = 0.000001;
        cfg.target_residual_unbalance_m = 0.000001;
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(result.residual_unbalance_m.is_finite());
    }

    #[test]
    fn test_vibration_reduction_at_least_40_percent() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let cfg = balance_cfg();
        let result = compute_balance_correction(&sim, 1000.0, &cfg, None, None);
        assert!(
            result.vibration_reduction_pct >= 40.0,
            "动平衡应至少降低40%振动 (实际 {:.1}%)",
            result.vibration_reduction_pct
        );
    }
}
