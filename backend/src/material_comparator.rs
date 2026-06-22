use crate::config::{EraProfile, EraProfiles, MaterialProfile, MaterialProfiles};
use crate::vibration_simulator::VibrationSimulator;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize, Clone, Debug)]
pub struct MaterialComparisonResult {
    pub material_id: String,
    pub display_name: String,
    pub critical_rpm: f64,
    pub total_displacement_mm: f64,
    pub whirl_risk: f64,
    pub cost_index: f64,
    pub relative_density: f64,
    pub damping_ratio_factor: f64,
    pub estimated_uniformity: f64,
    pub estimated_strength: f64,
}

pub fn compare_materials(
    sim: &VibrationSimulator,
    mats: &MaterialProfiles,
    eras: &EraProfiles,
    rpm: f64,
    era_id: Option<&str>,
    material_ids: &[String],
) -> Vec<MaterialComparisonResult> {
    let era: Option<&EraProfile> = era_id.and_then(|id| match id {
        "ancient_yuan" => Some(&eras.ancient_yuan),
        "modern_high_speed" => Some(&eras.modern_high_speed),
        _ => None,
    });

    let all_mats: [(&str, &MaterialProfile); 3] = [
        ("wood", &mats.wood),
        ("copper", &mats.copper),
        ("iron", &mats.iron),
    ];

    let mut results = Vec::new();

    for (id, mat) in &all_mats {
        if !material_ids.iter().any(|m| m == *id) {
            continue;
        }

        let vib = if let Some(e) = era {
            sim.analyze_with_material_and_era(rpm, mat, e)
        } else {
            let r = mat.apply_to_rotor(&sim.rotor);
            let s2 = VibrationSimulator::new(r, sim.bearing.clone());
            s2.analyze(rpm)
        };

        let disp_mm = vib.total_displacement * 1000.0;

        let cost_index = match *id {
            "wood" => 1.0,
            "copper" => 5.0,
            "iron" => 3.0,
            _ => 1.0,
        };

        results.push(MaterialComparisonResult {
            material_id: mat.material_id.clone(),
            display_name: mat.display_name.clone(),
            critical_rpm: vib.critical_rpm,
            total_displacement_mm: disp_mm,
            whirl_risk: if vib.whirl_instability { 1.0 } else { 0.0 },
            cost_index,
            relative_density: mat.density_kg_m3 / mats.iron.density_kg_m3,
            damping_ratio_factor: mat.damping_ratio_factor,
            estimated_uniformity: (95.0 - disp_mm * 50.0).max(0.0) * mat.quality_factor,
            estimated_strength: (15.0 - disp_mm * 3.0).max(0.0) * mat.quality_factor,
        });
    }

    results
}

pub fn render_comparison_json(
    rpm: f64,
    era_id: Option<&str>,
    comparisons: Vec<MaterialComparisonResult>,
) -> Value {
    json!({
        "rpm": rpm,
        "era_id": era_id,
        "comparisons": comparisons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EraProfile, MaterialProfile, OilFilmBearingConfig, RotorDynamicsConfig};

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

    fn test_material_profiles() -> MaterialProfiles {
        MaterialProfiles {
            wood: wood_profile(),
            copper: copper_profile(),
            iron: iron_profile(),
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
    fn test_compare_all_three_materials() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "copper".into(), "iron".into()];
        let results = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.critical_rpm > 0.0, "{} critical_rpm should be positive", r.material_id);
        }
    }

    #[test]
    fn test_wood_has_lowest_critical_rpm() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "copper".into(), "iron".into()];
        let results = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);
        let wood = results.iter().find(|r| r.material_id == "wood").unwrap();
        let copper = results.iter().find(|r| r.material_id == "copper").unwrap();
        let iron = results.iter().find(|r| r.material_id == "iron").unwrap();
        assert!(
            wood.critical_rpm < copper.critical_rpm,
            "wood critical_rpm ({}) should be < copper ({})",
            wood.critical_rpm, copper.critical_rpm
        );
        assert!(
            copper.critical_rpm < iron.critical_rpm,
            "copper critical_rpm ({}) should be < iron ({})",
            copper.critical_rpm, iron.critical_rpm
        );
    }

    #[test]
    fn test_iron_has_highest_uniformity() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "copper".into(), "iron".into()];
        let results = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);
        let wood = results.iter().find(|r| r.material_id == "wood").unwrap();
        let copper = results.iter().find(|r| r.material_id == "copper").unwrap();
        let iron = results.iter().find(|r| r.material_id == "iron").unwrap();
        assert!(
            iron.estimated_uniformity > copper.estimated_uniformity,
            "iron estimated_uniformity ({}) should be > copper ({})",
            iron.estimated_uniformity, copper.estimated_uniformity
        );
        assert!(
            copper.estimated_uniformity > wood.estimated_uniformity,
            "copper estimated_uniformity ({}) should be > wood ({})",
            copper.estimated_uniformity, wood.estimated_uniformity
        );
    }

    #[test]
    fn test_compare_with_era_context() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "copper".into(), "iron".into()];

        let results_no_era = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);
        let results_with_era = compare_materials(&sim, &mats, &eras, 500.0, Some("ancient_yuan"), &ids);

        assert_eq!(results_no_era.len(), results_with_era.len());

        let no_era_wood = results_no_era.iter().find(|r| r.material_id == "wood").unwrap();
        let with_era_wood = results_with_era.iter().find(|r| r.material_id == "wood").unwrap();
        assert_ne!(
            no_era_wood.critical_rpm, with_era_wood.critical_rpm,
            "era context should change critical_rpm"
        );
    }

    #[test]
    fn test_compare_selective_materials() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "iron".into()];
        let results = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);
        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|r| r.material_id.as_str()).collect();
        assert!(ids.contains(&"wood"));
        assert!(ids.contains(&"iron"));
        assert!(!ids.contains(&"copper"));
    }

    #[test]
    fn test_cost_index_mapping() {
        let sim = test_sim();
        let mats = test_material_profiles();
        let eras = test_era_profiles();
        let ids = vec!["wood".into(), "copper".into(), "iron".into()];
        let results = compare_materials(&sim, &mats, &eras, 500.0, None, &ids);

        let wood = results.iter().find(|r| r.material_id == "wood").unwrap();
        let copper = results.iter().find(|r| r.material_id == "copper").unwrap();
        let iron = results.iter().find(|r| r.material_id == "iron").unwrap();

        assert!((wood.cost_index - 1.0).abs() < 1e-6, "wood cost_index should be 1.0");
        assert!((copper.cost_index - 5.0).abs() < 1e-6, "copper cost_index should be 5.0");
        assert!((iron.cost_index - 3.0).abs() < 1e-6, "iron cost_index should be 3.0");
    }
}
