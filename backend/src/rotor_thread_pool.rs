use crate::config::AppConfig;
use crate::metrics::Metrics;
use crate::vibration_simulator::{VibrationResult, VibrationSimulator};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::debug;

pub type VibrationRequest = (String, f64);
pub type VibrationResponse = (String, VibrationResult);

pub struct RotorThreadPool {
    request_tx: mpsc::UnboundedSender<PoolTask>,
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

struct PoolTask {
    spindle_id: String,
    rpm: f64,
    reply: oneshot::Sender<VibrationResponse>,
}

impl RotorThreadPool {
    pub fn new(
        base_sim: VibrationSimulator,
        pool_size: usize,
        metrics: Arc<Metrics>,
    ) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel::<PoolTask>();
        let shared_rx = Arc::new(Mutex::new(request_rx));
        let mut handles = Vec::with_capacity(pool_size);
        let sim_arc = Arc::new(base_sim);

        for worker_id in 0..pool_size {
            let rx = Arc::clone(&shared_rx);
            let sim = Arc::clone(&sim_arc);
            let metrics_wk = Arc::clone(&metrics);
            let handle = tokio::spawn(async move {
                debug!(worker_id, "Rotor dynamics pool worker started");
                loop {
                    let task_opt = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };
                    match task_opt {
                        Some(task) => {
                            metrics_wk
                                .rotor_pool_tasks_total
                                .with_label_values(&[&worker_id.to_string()])
                                .inc();
                            let now = std::time::Instant::now();
                            let result = sim.analyze(task.rpm);
                            let elapsed = now.elapsed().as_secs_f64();
                            metrics_wk
                                .rotor_pool_task_duration_seconds
                                .with_label_values(&[&worker_id.to_string()])
                                .observe(elapsed);
                            let _ = task.reply.send((task.spindle_id.clone(), result));
                        }
                        None => {
                            debug!(worker_id, "Rotor pool worker channel closed, exiting");
                            break;
                        }
                    }
                }
            });
            handles.push(handle);
        }

        Self {
            request_tx,
            _handles: handles,
        }
    }

    pub fn from_config(cfg: &AppConfig, metrics: Arc<Metrics>) -> Self {
        let base_sim = VibrationSimulator::new(
            cfg.rotor_dynamics.clone(),
            cfg.oil_film_bearing.clone(),
        );
        let pool_size = cfg.api.vibration_pool_size.unwrap_or(4).max(1) as usize;
        Self::new(base_sim, pool_size, metrics)
    }

    pub async fn submit(&self, spindle_id: String, rpm: f64) -> VibrationResponse {
        let (reply_tx, reply_rx) = oneshot::channel::<VibrationResponse>();
        let task = PoolTask {
            spindle_id,
            rpm,
            reply: reply_tx,
        };
        let _ = self.request_tx.send(task);
        reply_rx.await.unwrap_or_else(|e| {
            panic!("Rotor pool response channel lost: {}", e);
        })
    }
}

pub async fn run_vibration_service_via_pool(
    pool: Arc<RotorThreadPool>,
    mut vib_rx: mpsc::UnboundedReceiver<VibrationRequest>,
    vib_out_tx: mpsc::UnboundedSender<VibrationResponse>,
    metrics: Arc<Metrics>,
) {
    loop {
        match vib_rx.recv().await {
            Some((spindle_id, rpm)) => {
                metrics.vibration_analyses_total.inc();
                let resp = pool.submit(spindle_id.clone(), rpm).await;
                let _ = vib_out_tx.send(resp);
            }
            None => {
                tracing::info!("vibration service channel closed via pool, exiting");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OilFilmBearingConfig, RotorDynamicsConfig};
    use crate::metrics::Metrics;

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

    #[tokio::test]
    async fn test_pool_submit_returns_valid_result() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 2, metrics);
        let (sid, res) = pool.submit("SP-TEST".to_string(), 1000.0).await;
        assert_eq!(sid, "SP-TEST");
        assert!(res.critical_rpm > 0.0);
        assert!(res.total_displacement.is_finite());
    }

    #[tokio::test]
    async fn test_pool_multiple_spindles_concurrent() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = Arc::new(RotorThreadPool::new(sim, 4, metrics));
        let ids = vec!["SP-1", "SP-2", "SP-3", "SP-4", "SP-5", "SP-6"];
        let mut handles = Vec::new();
        for id in ids {
            let p = Arc::clone(&pool);
            let h = tokio::spawn(async move {
                let (sid, res) = p.submit(id.to_string(), 1000.0).await;
                (sid, res.critical_rpm)
            });
            handles.push(h);
        }
        for h in handles {
            let (sid, cr) = h.await.unwrap();
            assert!(!sid.is_empty());
            assert!(cr > 1000.0);
        }
    }

    #[tokio::test]
    async fn test_pool_deterministic_same_rpm() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 2, metrics);
        let r1 = pool.submit("A".into(), 500.0).await;
        let r2 = pool.submit("B".into(), 500.0).await;
        assert!((r1.1.critical_rpm - r2.1.critical_rpm).abs() < 1e-6);
        assert!((r1.1.total_displacement - r2.1.total_displacement).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_pool_rpm_sweep_results_are_finite() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 3, metrics);
        let rpms = [100.0, 500.0, 1000.0, 3000.0, 8000.0, 18000.0, 25000.0];
        for rpm in rpms {
            let (_, res) = pool.submit(format!("SP-{}", rpm), rpm).await;
            assert!(res.critical_rpm.is_finite(), "cr not finite at {}", rpm);
            assert!(res.total_displacement.is_finite(), "disp not finite at {}", rpm);
        }
    }

    #[tokio::test]
    async fn test_pool_single_worker_sequential() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 1, metrics);
        let r1 = pool.submit("S1".into(), 1000.0).await;
        let r2 = pool.submit("S2".into(), 2000.0).await;
        let r3 = pool.submit("S3".into(), 3000.0).await;
        assert_eq!(r1.0, "S1");
        assert_eq!(r2.0, "S2");
        assert_eq!(r3.0, "S3");
        assert!(r1.1.critical_rpm > 0.0);
        assert!(r2.1.critical_rpm > 0.0);
        assert!(r3.1.critical_rpm > 0.0);
    }

    #[tokio::test]
    async fn test_pool_high_concurrency_20_tasks() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = Arc::new(RotorThreadPool::new(sim, 4, metrics));
        let mut handles = Vec::new();
        for i in 0..20 {
            let p = Arc::clone(&pool);
            let h = tokio::spawn(async move {
                let rpm = 500.0 + i as f64 * 100.0;
                let (sid, res) = p.submit(format!("HCON-{}", i), rpm).await;
                (sid, res.total_displacement.is_finite(), res.critical_rpm > 0.0)
            });
            handles.push(h);
        }
        for h in handles {
            let (sid, finite, positive) = h.await.unwrap();
            assert!(!sid.is_empty());
            assert!(finite, "displacement not finite for {}", sid);
            assert!(positive, "critical_rpm not positive for {}", sid);
        }
    }

    #[tokio::test]
    async fn test_pool_zero_rpm_handled() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 2, metrics);
        let (sid, res) = pool.submit("ZERO".into(), 0.0).await;
        assert_eq!(sid, "ZERO");
        assert!(res.critical_rpm > 0.0);
        assert!(res.total_displacement.is_finite());
    }

    #[tokio::test]
    async fn test_pool_negative_rpm_handled() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 2, metrics);
        let (sid, res) = pool.submit("NEG".into(), -1000.0).await;
        assert_eq!(sid, "NEG");
        assert!(res.critical_rpm > 0.0);
        assert!(res.total_displacement.is_finite());
    }

    #[tokio::test]
    async fn test_pool_extreme_rpm_handled() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = RotorThreadPool::new(sim, 2, metrics);
        let (sid, res) = pool.submit("EXT".into(), 100000.0).await;
        assert_eq!(sid, "EXT");
        assert!(res.critical_rpm.is_finite());
        assert!(res.total_displacement.is_finite());
    }

    #[tokio::test]
    async fn test_pool_vibration_service_via_pool() {
        let metrics = Metrics::new().unwrap();
        let sim = VibrationSimulator::new(base_rotor(), base_bearing());
        let pool = Arc::new(RotorThreadPool::new(sim, 2, Arc::clone(&metrics)));
        let (req_tx, req_rx) = mpsc::unbounded_channel::<VibrationRequest>();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<VibrationResponse>();

        let pool_clone = Arc::clone(&pool);
        let metrics_clone = Arc::clone(&metrics);
        let svc = tokio::spawn(async move {
            run_vibration_service_via_pool(pool_clone, req_rx, resp_tx, metrics_clone).await;
        });

        req_tx.send(("SP-VIA".into(), 1500.0)).unwrap();
        let resp = resp_rx.recv().await.unwrap();
        assert_eq!(resp.0, "SP-VIA");
        assert!(resp.1.critical_rpm > 0.0);

        drop(req_tx);
        tokio::time::timeout(std::time::Duration::from_secs(2), svc).await.unwrap().unwrap();
    }
}
