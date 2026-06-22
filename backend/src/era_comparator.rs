use crate::config::{EraProfiles, MaterialProfiles};
use crate::vibration_simulator::VibrationSimulator;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize, Clone, Debug)]
pub struct EraComparisonResult {
    pub era_id: String,
    pub display_name: String,
    pub era_year: String,
    pub description: String,
    pub typical_rpm: f64,
    pub critical_rpm: f64,
    pub total_displacement_mm: f64,
    pub uniformity: f64,
    pub strength: f64,
    pub daily_output_kg: f64,
    pub manufacturing_precision_factor: f64,
    pub bearing_technology: String,
    pub typical_yarn: String,
    pub whirl_instability: bool,
    pub whirl_ratio: f64,
    pub material_id: String,
}

pub fn compare_eras(
    sim: &VibrationSimulator,
    mats: &MaterialProfiles,
    eras: &EraProfiles,
    material_id: Option<&str>,
    rpm: Option<f64>,
) -> Vec<EraComparisonResult> {
    let era_list: Vec<(&str, &crate::config::EraProfile)> = vec![
        ("ancient_yuan", &eras.ancient_yuan),
        ("modern_high_speed", &eras.modern_high_speed),
    ];

    let mut results = Vec::new();
    for (era_id, era) in &era_list {
        let mat_id = material_id.unwrap_or(&era.default_material);
        let mat = match mat_id {
            "wood" => &mats.wood,
            "copper" => &mats.copper,
            _ => &mats.iron,
        };
        let resolved_rpm = rpm.unwrap_or(era.typical_rpm);
        let vib = sim.analyze_with_material_and_era(resolved_rpm, mat, era);
        let disp_mm = vib.total_displacement * 1000.0;

        let output_kg_per_hour = match *era_id {
            "ancient_yuan" => resolved_rpm / 500.0 * 2.5,
            "modern_high_speed" => resolved_rpm / 18000.0 * 35.0,
            _ => 0.0,
        };

        results.push(EraComparisonResult {
            era_id: era.era_id.clone(),
            display_name: era.display_name.clone(),
            era_year: era.era_year.clone(),
            description: era.description.clone(),
            typical_rpm: era.typical_rpm,
            critical_rpm: vib.critical_rpm,
            total_displacement_mm: disp_mm,
            uniformity: (95.0 - disp_mm * 30.0).max(0.0) * mat.quality_factor,
            strength: (15.0 - disp_mm * 2.0).max(0.0) * mat.quality_factor,
            daily_output_kg: (output_kg_per_hour * 24.0).round(),
            manufacturing_precision_factor: era.manufacturing_precision_factor,
            bearing_technology: era.bearing_technology.clone(),
            typical_yarn: era.typical_yarn.clone(),
            whirl_instability: vib.whirl_instability,
            whirl_ratio: vib.whirl_ratio,
            material_id: mat.material_id.clone(),
        });
    }
    results
}

pub fn render_comparison_json(results: Vec<EraComparisonResult>) -> Value {
    json!({
        "comparisons": results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        EraProfile, EraProfiles, MaterialProfile, MaterialProfiles, OilFilmBearingConfig,
        RotorDynamicsConfig,
    };

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

    fn wood_profile() -> MaterialProfile {
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
        }
    }

    fn iron_profile() -> MaterialProfile {
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
        }
    }

    fn copper_profile() -> MaterialProfile {
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
        }
    }

    fn test_material_profiles() -> MaterialProfiles {
        MaterialProfiles {
            wood: wood_profile(),
            copper: copper_profile(),
            iron: iron_profile(),
        }
    }

    fn ancient_era() -> EraProfile {
        EraProfile {
            era_id: "ancient_yuan".into(),
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
            era_id: "modern_high_speed".into(),
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

    fn test_era_profiles() -> EraProfiles {
        EraProfiles {
            ancient_yuan: ancient_era(),
            modern_high_speed: modern_era(),
        }
    }

    fn test_sim() -> VibrationSimulator {
        VibrationSimulator::new(base_rotor(), base_bearing())
    }

    #[test]
    fn test_compare_both_eras() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, None, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_modern_higher_critical_rpm() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, Some("iron"), None);
        let ancient = &results[0];
        let modern = &results[1];
        assert!(
            modern.critical_rpm > ancient.critical_rpm,
            "同材料下现代临界转速 {} 应高于古代 {}",
            modern.critical_rpm,
            ancient.critical_rpm
        );
    }

    #[test]
    fn test_modern_lower_displacement() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, Some("iron"), None);
        let ancient = &results[0];
        let modern = &results[1];
        assert!(
            modern.total_displacement_mm < ancient.total_displacement_mm,
            "现代位移 {} 应低于古代 {} (同材料)",
            modern.total_displacement_mm,
            ancient.total_displacement_mm
        );
    }

    #[test]
    fn test_era_with_explicit_material() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, Some("wood"), None);
        assert_eq!(results[0].material_id, "wood");
        assert_eq!(results[1].material_id, "wood");
    }

    #[test]
    fn test_era_with_explicit_rpm() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, None, Some(1000.0));
        let ancient = &results[0];
        let modern = &results[1];
        let ancient_expected: f64 = (1000.0_f64 / 500.0 * 2.5 * 24.0).round();
        let modern_expected: f64 = (1000.0_f64 / 18000.0 * 35.0 * 24.0).round();
        assert!(
            (ancient.daily_output_kg - ancient_expected).abs() < 1e-6,
            "古代日产量 {} 应对应1000RPM期望值 {}",
            ancient.daily_output_kg,
            ancient_expected
        );
        assert!(
            (modern.daily_output_kg - modern_expected).abs() < 1e-6,
            "现代日产量 {} 应对应1000RPM期望值 {}",
            modern.daily_output_kg,
            modern_expected
        );
    }

    #[test]
    fn test_daily_output_modern_much_higher() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let results = compare_eras(&sim, &mats, &eras, None, None);
        let ancient = &results[0];
        let modern = &results[1];
        assert!(
            modern.daily_output_kg > ancient.daily_output_kg * 10.0,
            "现代日产量 {} 应远高于古代 {} (10倍以上)",
            modern.daily_output_kg,
            ancient.daily_output_kg
        );
    }
}
