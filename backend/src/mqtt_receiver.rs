use crate::config::ValidationConfig;
use crate::metrics::Metrics;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SensorData {
    pub spindle_id: String,
    pub rpm: f64,
    pub vibration_amplitude: f64,
    pub temperature: f64,
    pub twist_per_meter: f64,
}

#[derive(Debug, Clone)]
pub struct ValidatedSensorData {
    pub data: SensorData,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    RpmOutOfRange { value: f64, min: f64, max: f64 },
    VibrationOutOfRange { value: f64, min: f64, max: f64 },
    TemperatureOutOfRange { value: f64, min: f64, max: f64 },
    TwistOutOfRange { value: f64, min: f64, max: f64 },
    EmptySpindleId,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::RpmOutOfRange { value, min, max } => {
                write!(f, "RPM {} out of range [{}, {}]", value, min, max)
            }
            ValidationError::VibrationOutOfRange { value, min, max } => {
                write!(f, "Vibration {} out of range [{}, {}]", value, min, max)
            }
            ValidationError::TemperatureOutOfRange { value, min, max } => {
                write!(f, "Temperature {} out of range [{}, {}]", value, min, max)
            }
            ValidationError::TwistOutOfRange { value, min, max } => {
                write!(f, "Twist {} out of range [{}, {}]", value, min, max)
            }
            ValidationError::EmptySpindleId => write!(f, "Empty spindle_id"),
        }
    }
}

pub fn validate_sensor_data(
    data: SensorData,
    cfg: &ValidationConfig,
) -> Result<ValidatedSensorData, ValidationError> {
    if data.spindle_id.trim().is_empty() {
        return Err(ValidationError::EmptySpindleId);
    }
    if data.rpm < cfg.rpm_min || data.rpm > cfg.rpm_max {
        return Err(ValidationError::RpmOutOfRange {
            value: data.rpm,
            min: cfg.rpm_min,
            max: cfg.rpm_max,
        });
    }
    if data.vibration_amplitude < cfg.vibration_min_mm
        || data.vibration_amplitude > cfg.vibration_max_mm
    {
        return Err(ValidationError::VibrationOutOfRange {
            value: data.vibration_amplitude,
            min: cfg.vibration_min_mm,
            max: cfg.vibration_max_mm,
        });
    }
    if data.temperature < cfg.temperature_min_c || data.temperature > cfg.temperature_max_c {
        return Err(ValidationError::TemperatureOutOfRange {
            value: data.temperature,
            min: cfg.temperature_min_c,
            max: cfg.temperature_max_c,
        });
    }
    if data.twist_per_meter < cfg.twist_min_per_meter
        || data.twist_per_meter > cfg.twist_max_per_meter
    {
        return Err(ValidationError::TwistOutOfRange {
            value: data.twist_per_meter,
            min: cfg.twist_min_per_meter,
            max: cfg.twist_max_per_meter,
        });
    }
    Ok(ValidatedSensorData {
        data,
        received_at: chrono::Utc::now(),
    })
}

pub async fn start_mqtt_receiver<F>(
    broker_host: &str,
    broker_port: u16,
    client_id: &str,
    topic: &str,
    keep_alive_sec: u64,
    validation_cfg: ValidationConfig,
    metrics: Arc<Metrics>,
    mut on_valid: F,
) -> anyhow::Result<()>
where
    F: FnMut(ValidatedSensorData) + Send + 'static,
{
    let mut mqttoptions = MqttOptions::new(client_id, broker_host, broker_port);
    mqttoptions.set_keep_alive(Duration::from_secs(keep_alive_sec));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe(topic, QoS::AtLeastOnce).await?;

    loop {
        match eventloop.poll().await {
            Ok(notification) => {
                if let rumqttc::Event::Incoming(rumqttc::Incoming::Publish(publish)) = notification
                {
                    metrics.mqtt_messages_total.inc();
                    match serde_json::from_slice::<SensorData>(&publish.payload) {
                        Ok(raw) => match validate_sensor_data(raw, &validation_cfg) {
                            Ok(valid) => on_valid(valid),
                            Err(e) => {
                                let label = match &e {
                                    ValidationError::RpmOutOfRange { .. } => "rpm_out_of_range",
                                    ValidationError::VibrationOutOfRange { .. } => "vibration_out_of_range",
                                    ValidationError::TemperatureOutOfRange { .. } => "temperature_out_of_range",
                                    ValidationError::TwistOutOfRange { .. } => "twist_out_of_range",
                                    ValidationError::EmptySpindleId => "empty_spindle_id",
                                };
                                metrics
                                    .mqtt_messages_invalid_total
                                    .with_label_values(&[label])
                                    .inc();
                                tracing::warn!("Sensor data validation failed: {}", e);
                            }
                        },
                        Err(e) => {
                            metrics
                                .mqtt_messages_invalid_total
                                .with_label_values(&["deserialize_error"])
                                .inc();
                            tracing::warn!("Failed to deserialize sensor data: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("MQTT receiver error: {:?}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub async fn run_receiver_service(
    cfg: crate::config::AppConfig,
    tx: mpsc::UnboundedSender<ValidatedSensorData>,
    metrics: Arc<Metrics>,
) -> anyhow::Result<()> {
    let mqtt_cfg = cfg.mqtt.clone();
    let val_cfg = cfg.validation.clone();
    start_mqtt_receiver(
        &mqtt_cfg.broker_host,
        mqtt_cfg.broker_port,
        &mqtt_cfg.subscriber_client_id,
        &mqtt_cfg.sensor_topic,
        mqtt_cfg.keep_alive_seconds,
        val_cfg,
        metrics,
        move |valid| {
            if let Err(e) = tx.send(valid) {
                tracing::error!("Failed to send validated data to channel: {}", e);
            }
        },
    )
    .await
}
