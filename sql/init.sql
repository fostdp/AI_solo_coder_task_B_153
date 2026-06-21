-- ============================================================
-- Ancient Water Spindle Monitoring System - ClickHouse Init
-- Includes: Raw tables, downsampling MVs, TTL retention policies
-- ============================================================

CREATE DATABASE IF NOT EXISTS spindle_system;

USE spindle_system;

-- ------------------------------------------------------------
-- 1. Raw tables - TTL 90 days, hot tier, MergeTree
-- ------------------------------------------------------------

CREATE TABLE IF NOT EXISTS spindle_sensor_data
(
    timestamp DateTime64(3),
    spindle_id String,
    rpm Float64,
    vibration_amplitude Float64,
    temperature Float64,
    twist_per_meter Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (spindle_id, timestamp)
TTL timestamp + INTERVAL 90 DAY
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS vibration_analysis
(
    timestamp DateTime64(3),
    spindle_id String,
    critical_rpm Float64,
    unbalance_response Float64,
    oil_film_stiffness_x Float64,
    oil_film_stiffness_y Float64,
    oil_film_damping_x Float64,
    oil_film_damping_y Float64,
    whirl_ratio Float64,
    eccentricity_ratio Float64,
    vibration_x Float64,
    vibration_y Float64,
    total_displacement Float64,
    phase_angle Float64,
    nonlinear_force_x Float64,
    nonlinear_force_y Float64,
    whirl_instability UInt8,
    nonlinear_damping_factor Float64,
    oil_film_pressure_peak Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (spindle_id, timestamp)
TTL timestamp + INTERVAL 90 DAY
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS yarn_quality
(
    timestamp DateTime64(3),
    spindle_id String,
    predicted_uniformity Float64,
    predicted_strength Float64,
    twist_variance Float64,
    vibration_impact_factor Float64,
    wear_coefficient Float64,
    calibration_error Float64,
    sample_count Int64,
    beta0 Float64,
    beta1 Float64,
    alpha0 Float64,
    alpha1 Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (spindle_id, timestamp)
TTL timestamp + INTERVAL 90 DAY
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS alerts
(
    timestamp DateTime64(3),
    spindle_id String,
    alert_type Enum8('vibration_overload' = 1, 'twist_uneven' = 2, 'critical_speed' = 3, 'temperature_high' = 4, 'oil_whirl' = 5),
    severity Enum8('warning' = 1, 'critical' = 2),
    message String,
    value Float64,
    threshold Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (spindle_id, alert_type, severity, timestamp)
TTL timestamp + INTERVAL 365 DAY
SETTINGS index_granularity = 8192;


-- ============================================================
-- 2. Downsampling Tier 1: 1-minute buckets (AggregatingMergeTree)
--    Retention: 30 days
-- ============================================================

CREATE TABLE IF NOT EXISTS spindle_sensor_data_1m
(
    bucket_start DateTime,
    spindle_id String,
    rpm_avg SimpleAggregateFunction(avg, Float64),
    rpm_min SimpleAggregateFunction(min, Float64),
    rpm_max SimpleAggregateFunction(max, Float64),
    vibration_avg SimpleAggregateFunction(avg, Float64),
    vibration_p95 SimpleAggregateFunction(max, Float64),
    temperature_avg SimpleAggregateFunction(avg, Float64),
    temperature_max SimpleAggregateFunction(max, Float64),
    twist_avg SimpleAggregateFunction(avg, Float64),
    twist_stddev SimpleAggregateFunction(max, Float64),
    samples SimpleAggregateFunction(sum, UInt64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(bucket_start)
ORDER BY (spindle_id, bucket_start)
TTL bucket_start + INTERVAL 30 DAY;

CREATE MATERIALIZED VIEW IF NOT EXISTS spindle_sensor_data_1m_mv
TO spindle_sensor_data_1m
AS
SELECT
    toStartOfMinute(timestamp) AS bucket_start,
    spindle_id,
    avg(rpm)                                        AS rpm_avg,
    min(rpm)                                        AS rpm_min,
    max(rpm)                                        AS rpm_max,
    avg(vibration_amplitude)                        AS vibration_avg,
    max(vibration_amplitude)                        AS vibration_p95,
    avg(temperature)                                AS temperature_avg,
    max(temperature)                                AS temperature_max,
    avg(twist_per_meter)                            AS twist_avg,
    max(abs(twist_per_meter - avg(twist_per_meter) OVER (PARTITION BY spindle_id, toStartOfMinute(timestamp)))) AS twist_stddev,
    count()                                         AS samples
FROM spindle_sensor_data
GROUP BY bucket_start, spindle_id;


CREATE TABLE IF NOT EXISTS vibration_analysis_1m
(
    bucket_start DateTime,
    spindle_id String,
    critical_rpm_avg SimpleAggregateFunction(avg, Float64),
    unbalance_response_avg SimpleAggregateFunction(avg, Float64),
    vibration_avg SimpleAggregateFunction(avg, Float64),
    vibration_p99 SimpleAggregateFunction(max, Float64),
    whirl_events SimpleAggregateFunction(sum, UInt64),
    max_oil_pressure_peak SimpleAggregateFunction(max, Float64),
    samples SimpleAggregateFunction(sum, UInt64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(bucket_start)
ORDER BY (spindle_id, bucket_start)
TTL bucket_start + INTERVAL 30 DAY;

CREATE MATERIALIZED VIEW IF NOT EXISTS vibration_analysis_1m_mv
TO vibration_analysis_1m
AS
SELECT
    toStartOfMinute(timestamp) AS bucket_start,
    spindle_id,
    avg(critical_rpm)                                AS critical_rpm_avg,
    avg(unbalance_response)                          AS unbalance_response_avg,
    avg(total_displacement)                          AS vibration_avg,
    max(total_displacement)                          AS vibration_p99,
    sum(whirl_instability)                           AS whirl_events,
    max(oil_film_pressure_peak)                      AS max_oil_pressure_peak,
    count()                                          AS samples
FROM vibration_analysis
GROUP BY bucket_start, spindle_id;


CREATE TABLE IF NOT EXISTS yarn_quality_1m
(
    bucket_start DateTime,
    spindle_id String,
    uniformity_avg SimpleAggregateFunction(avg, Float64),
    uniformity_min SimpleAggregateFunction(min, Float64),
    strength_avg SimpleAggregateFunction(avg, Float64),
    strength_min SimpleAggregateFunction(min, Float64),
    wear_coefficient SimpleAggregateFunction(max, Float64),
    samples SimpleAggregateFunction(sum, UInt64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(bucket_start)
ORDER BY (spindle_id, bucket_start)
TTL bucket_start + INTERVAL 30 DAY;

CREATE MATERIALIZED VIEW IF NOT EXISTS yarn_quality_1m_mv
TO yarn_quality_1m
AS
SELECT
    toStartOfMinute(timestamp) AS bucket_start,
    spindle_id,
    avg(predicted_uniformity)                         AS uniformity_avg,
    min(predicted_uniformity)                         AS uniformity_min,
    avg(predicted_strength)                           AS strength_avg,
    min(predicted_strength)                           AS strength_min,
    max(wear_coefficient)                             AS wear_coefficient,
    count()                                           AS samples
FROM yarn_quality
GROUP BY bucket_start, spindle_id;


-- ============================================================
-- 3. Downsampling Tier 2: 1-hour buckets (AggregatingMergeTree)
--    Retention: 2 years (long-term trend analysis)
-- ============================================================

CREATE TABLE IF NOT EXISTS spindle_sensor_data_1h
(
    bucket_start DateTime,
    spindle_id String,
    rpm_avg SimpleAggregateFunction(avg, Float64),
    rpm_min SimpleAggregateFunction(min, Float64),
    rpm_max SimpleAggregateFunction(max, Float64),
    vibration_avg SimpleAggregateFunction(avg, Float64),
    vibration_p95 SimpleAggregateFunction(max, Float64),
    temperature_avg SimpleAggregateFunction(avg, Float64),
    temperature_max SimpleAggregateFunction(max, Float64),
    twist_avg SimpleAggregateFunction(avg, Float64),
    twist_stddev SimpleAggregateFunction(max, Float64),
    samples SimpleAggregateFunction(sum, UInt64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(bucket_start)
ORDER BY (spindle_id, bucket_start)
TTL bucket_start + INTERVAL 2 YEAR;

CREATE MATERIALIZED VIEW IF NOT EXISTS spindle_sensor_data_1h_mv
TO spindle_sensor_data_1h
AS
SELECT
    toStartOfHour(timestamp) AS bucket_start,
    spindle_id,
    avg(rpm)                                        AS rpm_avg,
    min(rpm)                                        AS rpm_min,
    max(rpm)                                        AS rpm_max,
    avg(vibration_amplitude)                        AS vibration_avg,
    max(vibration_amplitude)                        AS vibration_p95,
    avg(temperature)                                AS temperature_avg,
    max(temperature)                                AS temperature_max,
    avg(twist_per_meter)                            AS twist_avg,
    max(abs(twist_per_meter - avg(twist_per_meter) OVER (PARTITION BY spindle_id, toStartOfHour(timestamp)))) AS twist_stddev,
    count()                                         AS samples
FROM spindle_sensor_data
GROUP BY bucket_start, spindle_id;


CREATE TABLE IF NOT EXISTS vibration_analysis_1h
(
    bucket_start DateTime,
    spindle_id String,
    vibration_avg SimpleAggregateFunction(avg, Float64),
    vibration_p99 SimpleAggregateFunction(max, Float64),
    whirl_events SimpleAggregateFunction(sum, UInt64),
    max_oil_pressure_peak SimpleAggregateFunction(max, Float64),
    samples SimpleAggregateFunction(sum, UInt64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(bucket_start)
ORDER BY (spindle_id, bucket_start)
TTL bucket_start + INTERVAL 2 YEAR;

CREATE MATERIALIZED VIEW IF NOT EXISTS vibration_analysis_1h_mv
TO vibration_analysis_1h
AS
SELECT
    toStartOfHour(timestamp) AS bucket_start,
    spindle_id,
    avg(total_displacement)                          AS vibration_avg,
    max(total_displacement)                          AS vibration_p99,
    sum(whirl_instability)                           AS whirl_events,
    max(oil_film_pressure_peak)                      AS max_oil_pressure_peak,
    count()                                          AS samples
FROM vibration_analysis
GROUP BY bucket_start, spindle_id;


CREATE TABLE IF NOT EXISTS alerts_daily
(
    day Date,
    alert_type Enum8('vibration_overload' = 1, 'twist_uneven' = 2, 'critical_speed' = 3, 'temperature_high' = 4, 'oil_whirl' = 5),
    severity Enum8('warning' = 1, 'critical' = 2),
    spindle_id String,
    total SimpleAggregateFunction(sum, UInt64),
    max_value SimpleAggregateFunction(max, Float64)
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY (day, alert_type, severity, spindle_id)
TTL day + INTERVAL 3 YEAR;

CREATE MATERIALIZED VIEW IF NOT EXISTS alerts_daily_mv
TO alerts_daily
AS
SELECT
    toDate(timestamp)      AS day,
    alert_type,
    severity,
    spindle_id,
    count()                AS total,
    max(value)             AS max_value
FROM alerts
GROUP BY day, alert_type, severity, spindle_id;


-- ============================================================
-- 4. Helpful views for dashboards
-- ============================================================

DROP VIEW IF EXISTS v_current_spindle_status;
CREATE VIEW v_current_spindle_status AS
SELECT
    s.spindle_id,
    s.timestamp,
    s.rpm,
    s.vibration_amplitude,
    s.temperature,
    s.twist_per_meter,
    v.total_displacement,
    v.whirl_instability,
    q.predicted_uniformity,
    q.predicted_strength,
    q.wear_coefficient
FROM
(
    SELECT *,
        row_number() OVER (PARTITION BY spindle_id ORDER BY timestamp DESC) AS rn
    FROM spindle_sensor_data
) s
ANY LEFT JOIN vibration_analysis v
    ON v.spindle_id = s.spindle_id AND v.timestamp = s.timestamp
ANY LEFT JOIN yarn_quality q
    ON q.spindle_id = s.spindle_id AND q.timestamp = s.timestamp
WHERE s.rn = 1;

SYSTEM FLUSH LOGS;
