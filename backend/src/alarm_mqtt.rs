use chrono::Utc;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::{AlertThresholdsConfig, MqttConfig};
use crate::metrics::Metrics;

#[derive(Serialize, Clone, Debug)]
pub struct AlertRecord {
    pub timestamp: String,
    pub spindle_id: String,
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
}

pub fn check_alerts(
    spindle_id: &str,
    rpm: f64,
    vibration_amplitude: f64,
    temperature: f64,
    twist_per_meter: f64,
    critical_rpm: f64,
    whirl_instability: bool,
    whirl_ratio: f64,
    thresholds: &AlertThresholdsConfig,
    target_twist: f64,
) -> Vec<AlertRecord> {
    let mut alerts = Vec::new();

    if vibration_amplitude > thresholds.vibration_critical_mm {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "vibration_overload".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "Vibration amplitude {:.3} mm exceeds critical threshold {:.1} mm",
                vibration_amplitude, thresholds.vibration_critical_mm
            ),
            value: vibration_amplitude,
            threshold: thresholds.vibration_critical_mm,
        });
    } else if vibration_amplitude > thresholds.vibration_warning_mm {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "vibration_overload".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "Vibration amplitude {:.3} mm exceeds warning threshold {:.1} mm",
                vibration_amplitude, thresholds.vibration_warning_mm
            ),
            value: vibration_amplitude,
            threshold: thresholds.vibration_warning_mm,
        });
    }

    let twist_variance = (twist_per_meter - target_twist).abs() / target_twist;

    if twist_variance > thresholds.twist_variance_critical {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "twist_uneven".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "Twist variance {:.3} exceeds critical threshold {:.2}",
                twist_variance, thresholds.twist_variance_critical
            ),
            value: twist_variance,
            threshold: thresholds.twist_variance_critical,
        });
    } else if twist_variance > thresholds.twist_variance_warning {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "twist_uneven".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "Twist variance {:.3} exceeds warning threshold {:.2}",
                twist_variance, thresholds.twist_variance_warning
            ),
            value: twist_variance,
            threshold: thresholds.twist_variance_warning,
        });
    }

    let tol = thresholds.critical_speed_tolerance_pct / 100.0;
    if critical_rpm > 0.0 && (rpm - critical_rpm).abs() / critical_rpm <= tol {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "critical_speed".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "RPM {:.1} is within {:.0}% of critical RPM {:.1}",
                rpm,
                thresholds.critical_speed_tolerance_pct,
                critical_rpm
            ),
            value: rpm,
            threshold: critical_rpm,
        });
    }

    if temperature > thresholds.temperature_critical_c {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "temperature_high".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "Temperature {:.1}°C exceeds critical threshold {:.0}°C",
                temperature, thresholds.temperature_critical_c
            ),
            value: temperature,
            threshold: thresholds.temperature_critical_c,
        });
    } else if temperature > thresholds.temperature_warning_c {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "temperature_high".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "Temperature {:.1}°C exceeds warning threshold {:.0}°C",
                temperature, thresholds.temperature_warning_c
            ),
            value: temperature,
            threshold: thresholds.temperature_warning_c,
        });
    }

    if whirl_instability {
        alerts.push(AlertRecord {
            timestamp: Utc::now().to_rfc3339(),
            spindle_id: spindle_id.to_string(),
            alert_type: "oil_whirl".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "Oil whirl instability detected, whirl ratio {:.3} exceeds threshold {:.2}",
                whirl_ratio, 0.55
            ),
            value: whirl_ratio,
            threshold: 0.55,
        });
    }

    alerts
}

pub fn alert_to_mqtt_payload(alert: &AlertRecord) -> String {
    serde_json::to_string(alert).unwrap()
}

pub async fn run_alert_mqtt_service(
    mqtt_cfg: MqttConfig,
    mut rx: mpsc::UnboundedReceiver<AlertRecord>,
    _metrics: Arc<Metrics>,
) -> anyhow::Result<()> {
    let mut options = MqttOptions::new(
        &mqtt_cfg.alert_publisher_client_id,
        &mqtt_cfg.broker_host,
        mqtt_cfg.broker_port,
    );
    options.set_keep_alive(Duration::from_secs(mqtt_cfg.keep_alive_seconds));

    let (client, mut eventloop) = AsyncClient::new(options, 10);

    loop {
        tokio::select! {
            Some(alert) = rx.recv() => {
                let payload = alert_to_mqtt_payload(&alert);
                if let Err(e) = client
                    .publish(
                        &mqtt_cfg.alert_topic,
                        QoS::AtLeastOnce,
                        false,
                        payload.as_bytes(),
                    )
                    .await
                {
                    tracing::error!("MQTT alert publish error: {}", e);
                } else {
                    tracing::info!("Published alert [{}] for {}", alert.alert_type, alert.spindle_id);
                }
            }
            notification = eventloop.poll() => {
                if let Err(e) = notification {
                    tracing::warn!("MQTT alert connection error: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
