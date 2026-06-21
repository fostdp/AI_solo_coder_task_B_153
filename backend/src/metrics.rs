use prometheus::{
    register_int_counter, register_int_counter_vec, register_int_gauge, HistogramOpts,
    HistogramVec, IntCounter, IntCounterVec, IntGauge, Registry,
};
use std::sync::Arc;

pub struct Metrics {
    pub registry: Registry,

    pub mqtt_messages_total: IntCounter,
    pub mqtt_messages_invalid_total: IntCounterVec,
    pub sensor_samples_total: IntCounterVec,

    pub vibration_analyses_total: IntCounter,
    pub whirl_instability_events_total: IntCounter,

    pub quality_predictions_total: IntCounter,
    pub lms_updates_total: IntCounter,
    pub wear_coefficient: IntGauge,

    pub alerts_total: IntCounterVec,

    pub clickhouse_write_total: IntCounterVec,

    pub api_request_duration_seconds: HistogramVec,
}

impl Metrics {
    pub fn new() -> anyhow::Result<Arc<Self>> {
        let registry = Registry::new_custom(Some("spindle".to_string()), None)?;

        let mqtt_messages_total = register_int_counter!(
            "mqtt_messages_total",
            "Total MQTT messages received from sensor topic",
        )?;
        registry.register(Box::new(mqtt_messages_total.clone()))?;

        let mqtt_messages_invalid_total = register_int_counter_vec!(
            "mqtt_messages_invalid_total",
            "Total invalid MQTT messages by validation error type",
            &["error_type"],
        )?;
        registry.register(Box::new(mqtt_messages_invalid_total.clone()))?;

        let sensor_samples_total = register_int_counter_vec!(
            "sensor_samples_total",
            "Total validated sensor samples received by spindle_id",
            &["spindle_id"],
        )?;
        registry.register(Box::new(sensor_samples_total.clone()))?;

        let vibration_analyses_total = register_int_counter!(
            "vibration_analyses_total",
            "Total vibration analyses performed",
        )?;
        registry.register(Box::new(vibration_analyses_total.clone()))?;

        let whirl_instability_events_total = register_int_counter!(
            "whirl_instability_events_total",
            "Total whirl instability events detected",
        )?;
        registry.register(Box::new(whirl_instability_events_total.clone()))?;

        let quality_predictions_total = register_int_counter!(
            "quality_predictions_total",
            "Total yarn quality predictions performed",
        )?;
        registry.register(Box::new(quality_predictions_total.clone()))?;

        let lms_updates_total = register_int_counter!(
            "lms_updates_total",
            "Total LMS online learning updates performed",
        )?;
        registry.register(Box::new(lms_updates_total.clone()))?;

        let wear_coefficient = register_int_gauge!(
            "wear_copermille_x1000",
            "Wear coefficient multiplied by 1000 for integer storage",
        )?;
        registry.register(Box::new(wear_coefficient.clone()))?;

        let alerts_total = register_int_counter_vec!(
            "alerts_total",
            "Total alerts raised by type and severity",
            &["alert_type", "severity"],
        )?;
        registry.register(Box::new(alerts_total.clone()))?;

        let clickhouse_write_total = register_int_counter_vec!(
            "clickhouse_write_total",
            "Total ClickHouse writes by target table",
            &["table", "status"],
        )?;
        registry.register(Box::new(clickhouse_write_total.clone()))?;

        let hist_opts = HistogramOpts::new(
            "http_request_duration_seconds",
            "HTTP request latency in seconds",
        )
        .buckets(vec![
            0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]);
        let api_request_duration_seconds =
            HistogramVec::new(hist_opts, &["endpoint", "method", "status_code"])?;
        registry.register(Box::new(api_request_duration_seconds.clone()))?;

        Ok(Arc::new(Self {
            registry,
            mqtt_messages_total,
            mqtt_messages_invalid_total,
            sensor_samples_total,
            vibration_analyses_total,
            whirl_instability_events_total,
            quality_predictions_total,
            lms_updates_total,
            wear_coefficient,
            alerts_total,
            clickhouse_write_total,
            api_request_duration_seconds,
        }))
    }

    pub fn encode_text(&self) -> anyhow::Result<String> {
        use prometheus::{Encoder, TextEncoder};
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&metric_families, &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }
}
