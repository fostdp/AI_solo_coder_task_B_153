use crate::alarm_mqtt::AlertRecord;
use crate::metrics::Metrics;
use crate::quality_predictor::YarnQualityResult;
use crate::vibration_simulator::VibrationResult;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ClickHouseWriter {
    client: Client,
    base_url: String,
    metrics: Arc<Metrics>,
}

impl ClickHouseWriter {
    pub fn new(base_url: &str, metrics: Arc<Metrics>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            metrics,
        }
    }

    pub async fn insert_sensor_data(
        &self,
        timestamp: &str,
        spindle_id: &str,
        rpm: f64,
        vibration_amplitude: f64,
        temperature: f64,
        twist_per_meter: f64,
    ) -> anyhow::Result<()> {
        let query = format!(
            "INSERT INTO spindle_system.spindle_sensor_data (timestamp, spindle_id, rpm, vibration_amplitude, temperature, twist_per_meter) VALUES ('{}', '{}', {}, {}, {}, {})",
            timestamp, spindle_id, rpm, vibration_amplitude, temperature, twist_per_meter
        );
        self.execute(&query).await
    }

    pub async fn insert_vibration_analysis(
        &self,
        timestamp: &str,
        spindle_id: &str,
        result: &VibrationResult,
    ) -> anyhow::Result<()> {
        let query = format!(
            "INSERT INTO spindle_system.vibration_analysis (timestamp, spindle_id, critical_rpm, unbalance_response, oil_film_stiffness_x, oil_film_stiffness_y, oil_film_damping_x, oil_film_damping_y, whirl_ratio, eccentricity_ratio, vibration_x, vibration_y, total_displacement, phase_angle, nonlinear_force_x, nonlinear_force_y, whirl_instability, nonlinear_damping_factor, oil_film_pressure_peak) VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            timestamp,
            spindle_id,
            result.critical_rpm,
            result.unbalance_response,
            result.oil_film_stiffness_x,
            result.oil_film_stiffness_y,
            result.oil_film_damping_x,
            result.oil_film_damping_y,
            result.whirl_ratio,
            result.eccentricity_ratio,
            result.vibration_x,
            result.vibration_y,
            result.total_displacement,
            result.phase_angle,
            result.nonlinear_force_x,
            result.nonlinear_force_y,
            if result.whirl_instability { 1 } else { 0 },
            result.nonlinear_damping_factor,
            result.oil_film_pressure_peak
        );
        self.execute(&query).await
    }

    pub async fn insert_yarn_quality(
        &self,
        timestamp: &str,
        spindle_id: &str,
        result: &YarnQualityResult,
    ) -> anyhow::Result<()> {
        let query = format!(
            "INSERT INTO spindle_system.yarn_quality (timestamp, spindle_id, predicted_uniformity, predicted_strength, twist_variance, vibration_impact_factor, wear_coefficient, calibration_error, sample_count, beta0, beta1, alpha0, alpha1) VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            timestamp,
            spindle_id,
            result.predicted_uniformity,
            result.predicted_strength,
            result.twist_variance,
            result.vibration_impact_factor,
            result.wear_coefficient,
            result.calibration_error,
            result.sample_count as i64,
            result.beta0,
            result.beta1,
            result.alpha0,
            result.alpha1
        );
        self.execute(&query).await
    }

    pub async fn insert_alert(&self, alert: &AlertRecord) -> anyhow::Result<()> {
        let query = format!(
            "INSERT INTO spindle_system.alerts (timestamp, spindle_id, alert_type, severity, message, value, threshold) VALUES ('{}', '{}', '{}', '{}', '{}', {}, {})",
            alert.timestamp,
            alert.spindle_id,
            alert.alert_type,
            alert.severity,
            alert.message.replace('\'', "\\'"),
            alert.value,
            alert.threshold
        );
        self.execute(&query).await
    }

    pub async fn query(&self, sql: &str) -> anyhow::Result<String> {
        let url = format!("{}/?query={}", self.base_url, urlencoding(&sql));
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(body)
    }

    async fn execute(&self, sql: &str) -> anyhow::Result<()> {
        let url = format!("{}/?query={}", self.base_url, urlencoding(sql));
        let resp = self.client.post(&url).send().await?;
        if !resp.status().is_success() {
            let body = resp.text().await?;
            anyhow::bail!("ClickHouse error: {}", body);
        }
        Ok(())
    }
}

fn urlencoding(s: &str) -> String {
    s.replace(' ', "+")
        .replace('\'', "%27")
        .replace('\n', "%0A")
}

pub async fn writer_loop(
    mut rx: mpsc::UnboundedReceiver<WriteCommand>,
    writer: Arc<ClickHouseWriter>,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            WriteCommand::SensorData {
                timestamp,
                spindle_id,
                rpm,
                vibration_amplitude,
                temperature,
                twist_per_meter,
            } => {
                let r = writer
                    .insert_sensor_data(
                        &timestamp,
                        &spindle_id,
                        rpm,
                        vibration_amplitude,
                        temperature,
                        twist_per_meter,
                    )
                    .await;
                let status = if r.is_ok() { "ok" } else { "err" };
                writer
                    .metrics
                    .clickhouse_write_total
                    .with_label_values(&["spindle_sensor_data", status])
                    .inc();
                if let Err(e) = r {
                    tracing::error!("Failed to write sensor data: {}", e);
                }
            }
            WriteCommand::VibrationAnalysis {
                timestamp,
                spindle_id,
                result,
            } => {
                let r = writer
                    .insert_vibration_analysis(&timestamp, &spindle_id, &result)
                    .await;
                let status = if r.is_ok() { "ok" } else { "err" };
                writer
                    .metrics
                    .clickhouse_write_total
                    .with_label_values(&["vibration_analysis", status])
                    .inc();
                if let Err(e) = r {
                    tracing::error!("Failed to write vibration analysis: {}", e);
                }
            }
            WriteCommand::YarnQuality {
                timestamp,
                spindle_id,
                result,
            } => {
                let r = writer
                    .insert_yarn_quality(&timestamp, &spindle_id, &result)
                    .await;
                let status = if r.is_ok() { "ok" } else { "err" };
                writer
                    .metrics
                    .clickhouse_write_total
                    .with_label_values(&["yarn_quality", status])
                    .inc();
                if let Err(e) = r {
                    tracing::error!("Failed to write yarn quality: {}", e);
                }
            }
            WriteCommand::Alert { alert } => {
                let r = writer.insert_alert(&alert).await;
                let status = if r.is_ok() { "ok" } else { "err" };
                writer
                    .metrics
                    .clickhouse_write_total
                    .with_label_values(&["alerts", status])
                    .inc();
                if let Err(e) = r {
                    tracing::error!("Failed to write alert: {}", e);
                }
            }
        }
    }
}

pub enum WriteCommand {
    SensorData {
        timestamp: String,
        spindle_id: String,
        rpm: f64,
        vibration_amplitude: f64,
        temperature: f64,
        twist_per_meter: f64,
    },
    VibrationAnalysis {
        timestamp: String,
        spindle_id: String,
        result: VibrationResult,
    },
    YarnQuality {
        timestamp: String,
        spindle_id: String,
        result: YarnQualityResult,
    },
    Alert {
        alert: AlertRecord,
    },
}
