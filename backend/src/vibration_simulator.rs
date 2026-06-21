use crate::config::{OilFilmBearingConfig, RotorDynamicsConfig};
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

        let omega = rpm * 2.0 * PI / 60.0;
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
