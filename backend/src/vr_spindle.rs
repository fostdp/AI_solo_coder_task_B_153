use crate::config::{EraProfiles, MaterialProfiles, RotorDynamicsConfig};
use crate::vibration_simulator::VibrationSimulator;
use serde::Serialize;
use std::f64::consts::PI;

#[derive(Serialize, Clone, Debug)]
pub struct ForceFeedbackResult {
    pub strength_pct: f64,
    pub near_critical: bool,
    pub critical_rpm: f64,
    pub current_rpm: f64,
    pub speed_ratio: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct RpmSweepPoint {
    pub rpm: f64,
    pub total_displacement: f64,
    pub critical_rpm: f64,
    pub whirl_instability: bool,
    pub is_finite: bool,
}

pub fn compute_critical_rpm(
    material_id: &str,
    era_id: Option<&str>,
    materials: &MaterialProfiles,
    eras: &EraProfiles,
    base_rotor: &RotorDynamicsConfig,
) -> f64 {
    let mat = match material_id {
        "iron" => &materials.iron,
        "copper" => &materials.copper,
        "wood" => &materials.wood,
        _ => &materials.iron,
    };

    let effective_rotor = if let Some(eid) = era_id {
        let era = match eid {
            "ancient_yuan" => &eras.ancient_yuan,
            "modern_high_speed" => &eras.modern_high_speed,
            _ => &eras.ancient_yuan,
        };
        era.apply_to_rotor(mat, base_rotor)
    } else {
        mat.apply_to_rotor(base_rotor)
    };

    let i_shaft = PI * effective_rotor.shaft_diameter_m.powi(4) / 64.0;
    let k_shaft =
        48.0 * effective_rotor.youngs_modulus_pa * i_shaft / effective_rotor.shaft_length_m.powi(3);
    let omega_cr = (k_shaft / effective_rotor.mass_kg).sqrt();
    omega_cr * 60.0 / (2.0 * PI)
}

pub fn compute_force_feedback(rpm: f64, critical_rpm: f64) -> ForceFeedbackResult {
    let speed_ratio = if critical_rpm > 0.0 {
        rpm / critical_rpm
    } else {
        0.0
    };
    let strength = if speed_ratio < 0.6 || speed_ratio > 1.4 {
        0.0
    } else {
        ((1.0 - (speed_ratio - 1.0).abs() / 0.4) * 100.0).clamp(0.0, 100.0)
    };
    let near_critical = strength > 70.0;
    ForceFeedbackResult {
        strength_pct: strength,
        near_critical,
        critical_rpm,
        current_rpm: rpm,
        speed_ratio,
    }
}

pub fn validate_rpm_sweep(sim: &VibrationSimulator, rpm_values: &[f64]) -> Vec<RpmSweepPoint> {
    rpm_values
        .iter()
        .map(|&rpm| {
            let res = sim.analyze(rpm);
            let is_finite = res.critical_rpm.is_finite() && res.total_displacement.is_finite();
            RpmSweepPoint {
                rpm,
                total_displacement: res.total_displacement,
                critical_rpm: res.critical_rpm,
                whirl_instability: res.whirl_instability,
                is_finite,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EraProfile, MaterialProfile, OilFilmBearingConfig, RotorDynamicsConfig};
    use crate::vibration_simulator::VibrationSimulator;
    use std::f64::consts::PI;

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

    fn material_profiles() -> MaterialProfiles {
        let mats = materials();
        MaterialProfiles {
            wood: mats[2].clone(),
            copper: mats[1].clone(),
            iron: mats[0].clone(),
        }
    }

    fn era_profiles() -> EraProfiles {
        EraProfiles {
            ancient_yuan: ancient_era(),
            modern_high_speed: modern_era(),
        }
    }

    #[test]
    fn test_rpm_100_low_vibration() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let res = sim.analyze(100.0);
        assert!(
            res.total_displacement < 0.0005,
            "100RPM位移应极小 ({})",
            res.total_displacement
        );
        assert!(!res.whirl_instability);
    }

    #[test]
    fn test_rpm_sweep_100_to_25000_all_finite() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let rpms: Vec<f64> = vec![
            100.0, 500.0, 1000.0, 3000.0, 5000.0, 10000.0, 18000.0, 25000.0,
        ];
        let sweep = validate_rpm_sweep(&sim, &rpms);
        for point in &sweep {
            assert!(point.is_finite, "RPM {} not finite", point.rpm);
            assert!(point.total_displacement >= 0.0);
        }
    }

    #[test]
    fn test_amplitude_monotonic_increase_near_resonance() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let r1 = sim.analyze(500.0);
        let r2 = sim.analyze(3000.0);
        let r3 = sim.analyze(5000.0);
        assert!(
            r3.total_displacement > r2.total_displacement
                || r2.total_displacement > r1.total_displacement,
            "随着RPM接近/超过临界，位移应总体上升"
        );
    }

    #[test]
    fn test_nonlinear_damping_factor_greater_than_one() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let res = sim.analyze(5000.0);
        assert!(
            res.nonlinear_damping_factor >= 1.0,
            "非线性阻尼因子应≥1 (实际 {})",
            res.nonlinear_damping_factor
        );
    }

    #[test]
    fn test_phase_angle_in_pi_range() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        for rpm in [100.0, 1000.0, 3000.0, 10000.0] {
            let res = sim.analyze(rpm);
            assert!(
                res.phase_angle >= -PI / 2.0 && res.phase_angle <= PI / 2.0,
                "RPM {} phase {} 超出范围",
                rpm,
                res.phase_angle
            );
        }
    }

    #[test]
    fn test_oil_film_stiffness_positive() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let res = sim.analyze(3000.0);
        assert!(res.oil_film_stiffness_x > 0.0);
        assert!(res.oil_film_stiffness_y > 0.0);
        assert!(res.oil_film_damping_x > 0.0);
        assert!(res.oil_film_damping_y > 0.0);
    }

    #[test]
    fn test_whirl_ratio_in_valid_range() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        for rpm in [100.0, 2000.0, 10000.0] {
            let res = sim.analyze(rpm);
            assert!(
                res.whirl_ratio >= 0.4 && res.whirl_ratio <= 1.5,
                "半频涡动比应在 0.4-1.5 之间 (实际 {})",
                res.whirl_ratio
            );
        }
    }

    #[test]
    fn test_eccentricity_ratio_bounded() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        for rpm in [100.0, 500.0, 2000.0, 5000.0] {
            let res = sim.analyze(rpm);
            assert!(
                res.eccentricity_ratio >= 0.0 && res.eccentricity_ratio <= 1.0,
                "偏心率 {} 越界",
                res.eccentricity_ratio
            );
        }
    }

    #[test]
    fn test_force_feedback_near_critical() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let res = sim.analyze(500.0);
        let fb = compute_force_feedback(res.critical_rpm, res.critical_rpm);
        assert!(
            fb.strength_pct > 70.0,
            "临界转速处力反馈应大于70% (实际 {})",
            fb.strength_pct
        );
        assert!(fb.near_critical);
        assert!((fb.speed_ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_force_feedback_far_from_critical() {
        let base = base_rotor();
        let sim = VibrationSimulator::new(base, base_bearing());
        let res = sim.analyze(100.0);
        let fb = compute_force_feedback(100.0, res.critical_rpm);
        assert!(
            fb.strength_pct < 10.0 || !fb.near_critical,
            "远离临界转速处力反馈应很弱 (实际 {}%)",
            fb.strength_pct
        );
    }

    #[test]
    fn test_compute_critical_rpm_with_material() {
        let base = base_rotor();
        let mats = material_profiles();
        let eras = era_profiles();
        let cr_iron = compute_critical_rpm("iron", None, &mats, &eras, &base);
        let cr_wood = compute_critical_rpm("wood", None, &mats, &eras, &base);
        assert!(cr_iron > 0.0, "铁的临界转速应为正");
        assert!(cr_wood > 0.0, "木的临界转速应为正");
        assert!(
            cr_wood < cr_iron,
            "木的临界转速应低于铁 (木={}, 铁={})",
            cr_wood,
            cr_iron
        );
    }

    #[test]
    fn test_compute_critical_rpm_with_era() {
        let base = base_rotor();
        let mats = material_profiles();
        let eras = era_profiles();
        let cr_no_era = compute_critical_rpm("iron", None, &mats, &eras, &base);
        let cr_ancient =
            compute_critical_rpm("iron", Some("ancient_yuan"), &mats, &eras, &base);
        let cr_modern =
            compute_critical_rpm("iron", Some("modern_high_speed"), &mats, &eras, &base);
        assert!(cr_no_era > 0.0);
        assert!(cr_ancient > 0.0);
        assert!(cr_modern > 0.0);
    }
}
