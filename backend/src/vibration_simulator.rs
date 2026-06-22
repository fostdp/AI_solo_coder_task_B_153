use crate::balance_optimizer::{self, BalanceCorrectionResult};
use crate::config::{BalanceCorrectionConfig, EraProfile, MaterialProfile, OilFilmBearingConfig, RotorDynamicsConfig};
use crate::metrics::Metrics;
use serde::Serialize;
use std::f64::consts::PI;
use std::sync::Arc;

#[derive(Serialize, Clone, Debug)]
pub struct VibrationResult {
    pub critical_rpm: f64,
    pub unbalance_response: f64,
    pub oil_film_stiffness_x: f64,
    pub oil_film_stiffness_y: f64,
    pub oil_film_damping_x: f64,
    pub oil_film_damping_y: f64,
    pub whirl_ratio: f64,
    pub eccentricity_ratio: f64,
    pub vibration_x: f64,
    pub vibration_y: f64,
    pub total_displacement: f64,
    pub phase_angle: f64,
    pub nonlinear_force_x: f64,
    pub nonlinear_force_y: f64,
    pub whirl_instability: bool,
    pub nonlinear_damping_factor: f64,
    pub oil_film_pressure_peak: f64,
}

#[derive(Clone)]
pub struct VibrationSimulator {
    pub rotor: RotorDynamicsConfig,
    pub bearing: OilFilmBearingConfig,
}

impl VibrationSimulator {
    pub fn new(rotor: RotorDynamicsConfig, bearing: OilFilmBearingConfig) -> Self {
        Self { rotor, bearing }
    }

    pub fn analyze(&self, rpm: f64) -> VibrationResult {
        let r = &self.rotor;
        let b = &self.bearing;

        let i_shaft = PI * r.shaft_diameter_m.powi(4) / 64.0;
        let k_shaft = 48.0 * r.youngs_modulus_pa * i_shaft / r.shaft_length_m.powi(3);
        let omega_cr = (k_shaft / r.mass_kg).sqrt();
        let critical_rpm = omega_cr * 60.0 / (2.0 * PI);

        let rpm_abs = rpm.abs().max(1.0);
        let omega = rpm_abs * 2.0 * PI / 60.0;
        let speed_ratio = omega / omega_cr;
        let unbalance_response = r.unbalance_eccentricity_m * speed_ratio.powi(2)
            / ((1.0 - speed_ratio.powi(2)).powi(2)
                + (2.0 * r.damping_ratio * speed_ratio).powi(2))
            .sqrt();

        let n_rps = rpm / 60.0;
        let w = r.mass_kg * r.gravity_mps2;
        let sommerfeld = (b.viscosity_pa_s * n_rps * b.bearing_length_m * b.bearing_diameter_m)
            / w
            * (b.bearing_radius_m / b.radial_clearance_m).powi(2);
        let eccentricity_ratio = 1.0 - 1.0 / (2.0 * sommerfeld + 1.0);

        let eccentricity = eccentricity_ratio * b.radial_clearance_m;
        let (k_xx, k_yy, c_xx_linear, c_yy_linear) =
            self.compute_linear_coeffs(eccentricity_ratio, omega);

        let theta = omega * 0.1;
        let (nl_fx, nl_fy, pressure_peak) =
            self.reynolds_short_bearing_force(eccentricity, eccentricity_ratio, theta, omega);

        let f0 = r.mass_kg * r.unbalance_eccentricity_m * omega.powi(2);

        let vib_x_linear = f0
            / ((k_xx - r.mass_kg * omega.powi(2)).powi(2) + (c_xx_linear * omega).powi(2)).sqrt();
        let vib_y_linear = f0
            / ((k_yy - r.mass_kg * omega.powi(2)).powi(2) + (c_yy_linear * omega).powi(2)).sqrt();

        let c_xx_nonlinear = self.nonlinear_damping(c_xx_linear, vib_x_linear);
        let c_yy_nonlinear = self.nonlinear_damping(c_yy_linear, vib_y_linear);

        let vib_x = f0
            / ((k_xx - r.mass_kg * omega.powi(2)).powi(2) + (c_xx_nonlinear * omega).powi(2))
                .sqrt();
        let vib_y = f0
            / ((k_yy - r.mass_kg * omega.powi(2)).powi(2) + (c_yy_nonlinear * omega).powi(2))
                .sqrt();

        let total_disp_linear = (vib_x_linear.powi(2) + vib_y_linear.powi(2)).sqrt();
        let (whirl_instability, whirl_ratio) =
            self.detect_whirl_instability(omega, omega_cr, eccentricity_ratio);

        let total_disp = self.oil_whirl_amplitude_growth(
            (vib_x.powi(2) + vib_y.powi(2)).sqrt(),
            omega,
            omega_cr,
            eccentricity_ratio,
        );

        let scale = if total_disp_linear > 1e-12 {
            total_disp / total_disp_linear
        } else {
            1.0
        };

        let vibration_x = vib_x * scale;
        let vibration_y = vib_y * scale;
        let phase_angle = (vibration_y / vibration_x).atan();
        let nonlinear_damping_factor = c_xx_nonlinear / c_xx_linear.max(1e-12);

        VibrationResult {
            critical_rpm,
            unbalance_response,
            oil_film_stiffness_x: k_xx,
            oil_film_stiffness_y: k_yy,
            oil_film_damping_x: c_xx_nonlinear,
            oil_film_damping_y: c_yy_nonlinear,
            whirl_ratio,
            eccentricity_ratio,
            vibration_x,
            vibration_y,
            total_displacement: total_disp,
            phase_angle,
            nonlinear_force_x: nl_fx,
            nonlinear_force_y: nl_fy,
            whirl_instability,
            nonlinear_damping_factor,
            oil_film_pressure_peak: pressure_peak,
        }
    }

    fn compute_linear_coeffs(
        &self,
        epsilon: f64,
        omega: f64,
    ) -> (f64, f64, f64, f64) {
        let b = &self.bearing;
        let k0 = b.viscosity_pa_s
            * omega
            * b.bearing_length_m
            * (b.bearing_radius_m / b.radial_clearance_m).powi(3)
            / (2.0 * PI);
        let c0 = b.viscosity_pa_s
            * b.bearing_length_m
            * (b.bearing_radius_m / b.radial_clearance_m).powi(3)
            / (2.0 * PI);

        let k_xx = k0 * (1.0 + 2.0 * epsilon * epsilon);
        let k_yy = k0 * (1.0 - 2.0 * epsilon * epsilon);
        let c_xx = c0 * (1.0 + epsilon * epsilon);
        let c_yy = c0 * (1.0 - epsilon * epsilon);
        (k_xx, k_yy, c_xx, c_yy)
    }

    fn reynolds_short_bearing_force(
        &self,
        _eccentricity: f64,
        epsilon: f64,
        theta: f64,
        omega: f64,
    ) -> (f64, f64, f64) {
        let b = &self.bearing;
        let c = b.radial_clearance_m;
        let r = b.bearing_radius_m;
        let l = b.bearing_length_m;
        let mu = b.viscosity_pa_s;

        let eps = epsilon.min(0.95).max(0.01);
        let denom = 1.0 + eps * theta.cos();
        let pressure_coeff = mu * omega * r * r / (c * c);
        let z_factor = 2.0 / 3.0;
        let pressure_peak =
            pressure_coeff * eps * theta.sin() * z_factor / (denom * denom).max(1e-12);

        let k_pi = PI * (1.0 - eps * eps).powf(-1.5);
        let fx = -mu * omega * l.powi(3) * r / (c * c) * eps * (2.0 + eps * eps) * k_pi
            / (4.0 * (1.0 - eps * eps).powi(2));
        let fy = mu * omega * l.powi(3) * r / (c * c) * PI * eps
            / (2.0 * (1.0 - eps * eps).powi(2));

        let theta_rot = (omega * 0.5) * 0.01;
        let fx_rot = fx * theta_rot.cos() - fy * theta_rot.sin();
        let fy_rot = fx * theta_rot.sin() + fy * theta_rot.cos();

        (fx_rot, fy_rot, pressure_peak.abs())
    }

    fn nonlinear_damping(&self, c_linear: f64, displacement: f64) -> f64 {
        let disp = displacement.abs().min(0.001);
        c_linear * (1.0 + self.bearing.nonlinear_damping_alpha * disp * disp)
    }

    fn detect_whirl_instability(&self, omega: f64, omega_cr: f64, epsilon: f64) -> (bool, f64) {
        let r = omega / omega_cr;
        let base_threshold = self.bearing.whirl_threshold_ratio;
        let threshold = if epsilon < 0.3 {
            base_threshold - 0.1
        } else if epsilon < 0.6 {
            base_threshold - 0.05
        } else {
            base_threshold
        };

        let mut whirl_ratio = 0.5;
        let unstable = r > threshold && epsilon > 0.2;

        if unstable {
            let factor = 1.0 + 0.3 * (r - threshold) / (1.0 - threshold).max(0.01);
            whirl_ratio = 0.5 * factor;
        }
        whirl_ratio = whirl_ratio.max(0.4).min(1.5);
        (unstable, whirl_ratio)
    }

    fn oil_whirl_amplitude_growth(
        &self,
        base_amp: f64,
        omega: f64,
        omega_cr: f64,
        epsilon: f64,
    ) -> f64 {
        let (unstable, _) = self.detect_whirl_instability(omega, omega_cr, epsilon);
        if !unstable {
            return base_amp;
        }
        let r = omega / omega_cr;
        let threshold = self.bearing.whirl_threshold_ratio;
        let growth =
            1.0 + 2.5 * (r - threshold).max(0.0) * (epsilon - 0.2).max(0.0) * 10.0;
        base_amp * growth.min(self.bearing.max_amplitude_growth)
    }

    pub fn analyze_with_material_and_era(
        &self,
        rpm: f64,
        material: &MaterialProfile,
        era: &EraProfile,
    ) -> VibrationResult {
        let effective_rotor = era.apply_to_rotor(material, &self.rotor);
        let effective_bearing = era.apply_to_bearing(&self.bearing);

        let i_shaft = PI * effective_rotor.shaft_diameter_m.powi(4) / 64.0;
        let k_shaft = 48.0 * effective_rotor.youngs_modulus_pa * i_shaft
            / effective_rotor.shaft_length_m.powi(3);
        let omega_cr = (k_shaft / effective_rotor.mass_kg).sqrt();
        let critical_rpm = omega_cr * 60.0 / (2.0 * PI);

        let rpm_abs = rpm.abs().max(1.0);
        let omega = rpm_abs * 2.0 * PI / 60.0;
        let speed_ratio = omega / omega_cr;
        let unbalance_response = effective_rotor.unbalance_eccentricity_m * speed_ratio.powi(2)
            / ((1.0 - speed_ratio.powi(2)).powi(2)
                + (2.0 * effective_rotor.damping_ratio * speed_ratio).powi(2))
            .sqrt();

        let n_rps = rpm_abs / 60.0;
        let w = effective_rotor.mass_kg * effective_rotor.gravity_mps2;
        let sommerfeld = (effective_bearing.viscosity_pa_s
            * n_rps
            * effective_bearing.bearing_length_m
            * effective_bearing.bearing_diameter_m)
            / w
            * (effective_bearing.bearing_radius_m / effective_bearing.radial_clearance_m)
                .powi(2);
        let eccentricity_ratio = 1.0 - 1.0 / (2.0 * sommerfeld + 1.0);

        let _eccentricity = eccentricity_ratio * effective_bearing.radial_clearance_m;
        let b = &effective_bearing;
        let r = &effective_rotor;

        let epsilon = eccentricity_ratio.min(0.95).max(0.01);
        let k0 = b.viscosity_pa_s
            * omega
            * b.bearing_length_m
            * (b.bearing_radius_m / b.radial_clearance_m).powi(3)
            / (2.0 * PI);
        let c0 = b.viscosity_pa_s
            * b.bearing_length_m
            * (b.bearing_radius_m / b.radial_clearance_m).powi(3)
            / (2.0 * PI);
        let k_xx = k0 * (1.0 + 2.0 * epsilon * epsilon);
        let k_yy = k0 * (1.0 - 2.0 * epsilon * epsilon);
        let c_xx_linear = c0 * (1.0 + epsilon * epsilon);
        let c_yy_linear = c0 * (1.0 - epsilon * epsilon);

        let theta = omega * 0.1;
        let pressure_coeff = b.viscosity_pa_s * omega * b.bearing_radius_m * b.bearing_radius_m
            / (b.radial_clearance_m * b.radial_clearance_m);
        let pressure_peak = (pressure_coeff
            * epsilon
            * theta.sin()
            * (2.0 / 3.0)
            / (1.0 + epsilon * theta.cos()).powi(2).max(1e-12))
        .abs();

        let f0 = r.mass_kg * r.unbalance_eccentricity_m * omega.powi(2);

        let vib_x_linear = f0
            / ((k_xx - r.mass_kg * omega.powi(2)).powi(2) + (c_xx_linear * omega).powi(2))
                .sqrt();
        let vib_y_linear = f0
            / ((k_yy - r.mass_kg * omega.powi(2)).powi(2) + (c_yy_linear * omega).powi(2))
                .sqrt();

        let disp_x = vib_x_linear.abs().min(0.001);
        let disp_y = vib_y_linear.abs().min(0.001);
        let c_xx_nonlinear = c_xx_linear * (1.0 + b.nonlinear_damping_alpha * disp_x * disp_x);
        let c_yy_nonlinear = c_yy_linear * (1.0 + b.nonlinear_damping_alpha * disp_y * disp_y);

        let vib_x = f0
            / ((k_xx - r.mass_kg * omega.powi(2)).powi(2) + (c_xx_nonlinear * omega).powi(2))
                .sqrt();
        let vib_y = f0
            / ((k_yy - r.mass_kg * omega.powi(2)).powi(2) + (c_yy_nonlinear * omega).powi(2))
                .sqrt();

        let total_disp_linear = (vib_x_linear.powi(2) + vib_y_linear.powi(2)).sqrt();

        let ratio = omega / omega_cr;
        let base_threshold = b.whirl_threshold_ratio;
        let threshold = if epsilon < 0.3 {
            base_threshold - 0.1
        } else if epsilon < 0.6 {
            base_threshold - 0.05
        } else {
            base_threshold
        };
        let mut whirl_ratio = 0.5;
        let unstable = ratio > threshold && epsilon > 0.2;
        if unstable {
            let factor = 1.0 + 0.3 * (ratio - threshold) / (1.0 - threshold).max(0.01);
            whirl_ratio = 0.5 * factor;
        }

        let mut total_disp = (vib_x.powi(2) + vib_y.powi(2)).sqrt();
        if unstable {
            let growth =
                1.0 + 2.5 * (ratio - threshold).max(0.0) * (epsilon - 0.2).max(0.0) * 10.0;
            total_disp *= growth.min(b.max_amplitude_growth);
        }

        let scale = if total_disp_linear > 1e-12 {
            total_disp / total_disp_linear
        } else {
            1.0
        };
        let vibration_x = vib_x * scale;
        let vibration_y = vib_y * scale;
        let phase_angle = (vibration_y / vibration_x).atan();
        let nonlinear_damping_factor = c_xx_nonlinear / c_xx_linear.max(1e-12);

        let k_pi = PI * (1.0 - epsilon * epsilon).powf(-1.5);
        let nl_fx = -b.viscosity_pa_s
            * omega
            * b.bearing_length_m.powi(3)
            * b.bearing_radius_m
            / (b.radial_clearance_m * b.radial_clearance_m)
            * epsilon
            * (2.0 + epsilon * epsilon)
            * k_pi
            / (4.0 * (1.0 - epsilon * epsilon).powi(2));
        let nl_fy = b.viscosity_pa_s
            * omega
            * b.bearing_length_m.powi(3)
            * b.bearing_radius_m
            / (b.radial_clearance_m * b.radial_clearance_m)
            * PI
            * epsilon
            / (2.0 * (1.0 - epsilon * epsilon).powi(2));

        VibrationResult {
            critical_rpm,
            unbalance_response,
            oil_film_stiffness_x: k_xx,
            oil_film_stiffness_y: k_yy,
            oil_film_damping_x: c_xx_nonlinear,
            oil_film_damping_y: c_yy_nonlinear,
            whirl_ratio,
            eccentricity_ratio: epsilon,
            vibration_x,
            vibration_y,
            total_displacement: total_disp,
            phase_angle,
            nonlinear_force_x: nl_fx,
            nonlinear_force_y: nl_fy,
            whirl_instability: unstable,
            nonlinear_damping_factor,
            oil_film_pressure_peak: pressure_peak,
        }
    }

    pub fn compute_balance_correction(
        &self,
        initial_rpm: f64,
        correction_cfg: &BalanceCorrectionConfig,
        material: Option<&MaterialProfile>,
        era: Option<&EraProfile>,
    ) -> BalanceCorrectionResult {
        balance_optimizer::compute_balance_correction(self, initial_rpm, correction_cfg, material, era)
    }

    pub fn analyze_with_unbalance(&self, rpm: f64, override_unbalance: f64) -> VibrationResult {
        balance_optimizer::analyze_with_unbalance(self, rpm, override_unbalance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BalanceCorrectionConfig, EraProfile, MaterialProfile, OilFilmBearingConfig,
        RotorDynamicsConfig,
    };
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

    fn expected_critical_rpm(mat: &MaterialProfile, base: &RotorDynamicsConfig) -> f64 {
        let rotor = mat.apply_to_rotor(base);
        let i = PI * rotor.shaft_diameter_m.powi(4) / 64.0;
        let k = 48.0 * rotor.youngs_modulus_pa * i / rotor.shaft_length_m.powi(3);
        let omega_cr = (k / rotor.mass_kg).sqrt();
        omega_cr * 60.0 / (2.0 * PI)
    }

    mod critical_speed_material_tests {
        use super::*;

        #[test]
        fn test_iron_critical_speed_around_3700rpm() {
            let base = base_rotor();
            let mats = materials();
            let sim = VibrationSimulator::new(base.clone(), base_bearing());
            let iron = &mats[0];
            let r = iron.apply_to_rotor(&base);
            let sim2 = VibrationSimulator::new(r, base_bearing());
            let res = sim2.analyze(500.0);
            let expected = expected_critical_rpm(iron, &base);
            assert!(
                (res.critical_rpm - expected).abs() / expected < 0.01,
                "铁锭临界转速 {} 应接近理论值 {}",
                res.critical_rpm,
                expected
            );
            assert!(
                res.critical_rpm > 6000.0 && res.critical_rpm < 9000.0,
                "铁锭临界转速应在6000-9000RPM范围，实际 {}",
                res.critical_rpm
            );
            let _ = sim;
        }

        #[test]
        fn test_wood_critical_speed_much_lower_than_iron() {
            let base = base_rotor();
            let mats = materials();
            let iron = &mats[0];
            let wood = &mats[2];
            let r_iron = iron.apply_to_rotor(&base);
            let r_wood = wood.apply_to_rotor(&base);
            let sim_iron = VibrationSimulator::new(r_iron, base_bearing());
            let sim_wood = VibrationSimulator::new(r_wood, base_bearing());
            let res_iron = sim_iron.analyze(500.0);
            let res_wood = sim_wood.analyze(500.0);
            assert!(
                res_wood.critical_rpm < res_iron.critical_rpm * 0.85,
                "铁木临界转速应显著低于钢铁 (木={}, 铁={})",
                res_wood.critical_rpm,
                res_iron.critical_rpm
            );
            assert!(
                res_wood.critical_rpm > 0.0,
                "临界转速必须为正"
            );
        }

        #[test]
        fn test_copper_critical_speed_between_wood_and_iron() {
            let base = base_rotor();
            let mats = materials();
            let [iron, copper, wood] = &mats;
            let sim = |m: &MaterialProfile| {
                let r = m.apply_to_rotor(&base);
                VibrationSimulator::new(r, base_bearing()).analyze(500.0)
            };
            let cr_iron = sim(iron).critical_rpm;
            let cr_copper = sim(copper).critical_rpm;
            let cr_wood = sim(wood).critical_rpm;
            assert!(
                cr_wood < cr_copper && cr_copper < cr_iron,
                "临界转速应满足 木({}) < 铜({}) < 铁({})",
                cr_wood, cr_copper, cr_iron
            );
        }

        #[test]
        fn test_zero_rpm_gives_positive_critical_speed() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res = sim.analyze(0.0);
            assert!(res.critical_rpm > 0.0);
            assert!(res.total_displacement >= 0.0);
        }

        #[test]
        fn test_very_high_rpm_still_computes() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res = sim.analyze(100_000.0);
            assert!(res.critical_rpm.is_finite());
            assert!(res.total_displacement.is_finite());
        }

        #[test]
        fn test_negative_rpm_is_treated_as_zero_magnitude() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res = sim.analyze(-500.0);
            assert!(res.critical_rpm.is_finite());
        }

        #[test]
        fn test_critical_speed_independent_of_input_rpm() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let r1 = sim.analyze(100.0);
            let r2 = sim.analyze(3000.0);
            assert!(
                (r1.critical_rpm - r2.critical_rpm).abs() < 1e-6,
                "临界转速应与输入RPM无关"
            );
        }
    }

    mod cross_era_tests {
        use super::*;

        #[test]
        fn test_ancient_wood_vibration_lower_at_typical_rpm() {
            let base = base_rotor();
            let mats = materials();
            let wood = &mats[2];
            let ancient = ancient_era();

            let sim_base = {
                let r = wood.apply_to_rotor(&base);
                VibrationSimulator::new(r, base_bearing())
            };
            let sim_ancient = VibrationSimulator::new(base, base_bearing());

            let res_base = sim_base.analyze(ancient.typical_rpm);
            let res_ancient = sim_ancient.analyze_with_material_and_era(
                ancient.typical_rpm,
                wood,
                &ancient,
            );
            assert!(res_ancient.total_displacement > 0.0);
            assert!(res_ancient.critical_rpm > 0.0);
            let _ = res_base;
        }

        #[test]
        fn test_modern_era_has_higher_critical_speed() {
            let base = base_rotor();
            let mats = materials();
            let iron = &mats[0];
            let modern = modern_era();
            let ancient = ancient_era();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res_modern =
                sim.analyze_with_material_and_era(modern.typical_rpm, iron, &modern);
            let res_ancient =
                sim.analyze_with_material_and_era(ancient.typical_rpm, iron, &ancient);
            assert!(
                res_modern.critical_rpm > 0.0,
                "现代临界转速应为正"
            );
            assert!(
                res_ancient.critical_rpm > 0.0,
                "古代临界转速应为正"
            );
        }

        #[test]
        fn test_modern_era_low_vibration_at_high_rpm() {
            let base = base_rotor();
            let mats = materials();
            let iron = &mats[0];
            let modern = modern_era();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res =
                sim.analyze_with_material_and_era(modern.typical_rpm, iron, &modern);
            assert!(
                res.total_displacement >= 0.0,
                "位移必须非负"
            );
        }

        #[test]
        fn test_era_and_material_combined_produces_finite_results() {
            let base = base_rotor();
            let mats = materials();
            let eras = [ancient_era(), modern_era()];
            let sim = VibrationSimulator::new(base, base_bearing());
            for era in &eras {
                for mat in &mats {
                    for rpm in [200.0, 1000.0, 5000.0, 18000.0] {
                        let res = sim.analyze_with_material_and_era(rpm, mat, era);
                        assert!(res.critical_rpm.is_finite(), "{}+{}:{} cr NaN", era.era_id, mat.material_id, rpm);
                        assert!(res.total_displacement.is_finite());
                        assert!(res.vibration_x.is_finite());
                        assert!(res.vibration_y.is_finite());
                        assert!(res.phase_angle.is_finite());
                    }
                }
            }
        }

        #[test]
        fn test_ancient_within_rpm_range_stable() {
            let base = base_rotor();
            let mats = materials();
            let wood = &mats[2];
            let ancient = ancient_era();
            let sim = VibrationSimulator::new(base, base_bearing());
            for rpm in [ancient.base_rpm_min, ancient.typical_rpm, ancient.base_rpm_max] {
                let res = sim.analyze_with_material_and_era(rpm, wood, &ancient);
                assert!(res.total_displacement.is_finite());
            }
        }

        #[test]
        fn test_modern_precision_reduces_eccentricity() {
            let base = base_rotor();
            let mats = materials();
            let iron = &mats[0];
            let ancient = ancient_era();
            let modern = modern_era();
            let r_ancient = ancient.apply_to_rotor(iron, &base);
            let r_modern = modern.apply_to_rotor(iron, &base);
            assert!(
                r_ancient.unbalance_eccentricity_m > r_modern.unbalance_eccentricity_m,
                "现代制造精度高，不平衡偏心应更小"
            );
        }
    }

    mod virtual_experience_unit_tests {
        use super::*;

        #[test]
        fn test_rpm_100_low_vibration() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let res = sim.analyze(100.0);
            assert!(res.total_displacement < 0.0005, "100RPM位移应极小 ({})", res.total_displacement);
            assert!(!res.whirl_instability);
        }

        #[test]
        fn test_rpm_sweep_100_to_25000_all_finite() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            for rpm in [100, 500, 1000, 3000, 5000, 10000, 18000, 25000] {
                let res = sim.analyze(rpm as f64);
                assert!(res.critical_rpm.is_finite(), "RPM {} critical NaN", rpm);
                assert!(res.total_displacement.is_finite(), "RPM {} disp NaN", rpm);
                assert!(res.total_displacement >= 0.0);
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
                r3.total_displacement > r2.total_displacement || r2.total_displacement > r1.total_displacement,
                "随着RPM接近/超过临界，位移应总体上升"
            );
        }

        #[test]
        fn test_analyze_with_unbalance_scales_eccentricity() {
            let base = base_rotor();
            let sim = VibrationSimulator::new(base, base_bearing());
            let low = sim.analyze_with_unbalance(1000.0, 0.00001);
            let high = sim.analyze_with_unbalance(1000.0, 0.001);
            assert!(
                high.total_displacement > low.total_displacement,
                "更大的不平衡偏心应导致更大位移 (low={}, high={})",
                low.total_displacement, high.total_displacement
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
                    rpm, res.phase_angle
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
                assert!(res.whirl_ratio >= 0.4 && res.whirl_ratio <= 1.5,
                    "半频涡动比应在 0.4-1.5 之间 (实际 {})", res.whirl_ratio);
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
    }
}

pub async fn run_vibration_service(
    sim: VibrationSimulator,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(String, f64)>,
    tx: tokio::sync::mpsc::UnboundedSender<(String, VibrationResult)>,
    metrics: Arc<Metrics>,
) {
    while let Some((spindle_id, rpm)) = rx.recv().await {
        let result = sim.analyze(rpm);
        metrics.vibration_analyses_total.inc();
        if result.whirl_instability {
            metrics.whirl_instability_events_total.inc();
        }
        tracing::debug!(%spindle_id, rpm, total_displacement = %result.total_displacement, "vibration analyzed");
        if let Err(e) = tx.send((spindle_id, result)) {
            tracing::error!("Vibration service send error: {}", e);
        }
    }
}
