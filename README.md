# 古代水转大纺车锭子振动仿真与纱线质量预测系统

> 一套面向纺织史研究的全栈系统：模拟元代水转大纺车的 32 枚锭子，
> 基于转子动力学（Jeffcott + Reynolds 短轴承）计算油膜轴承临界转速与涡动失稳，
> 用在线 LMS 回归模型预测纱线均匀度与强度，MQTT 传输传感器数据和告警，
> ClickHouse 做持久化与降采样，Three.js + Canvas 可视化锭子三维振动与纱线 LOD。

---

## 目录

- [架构总览](#架构总览)
- [服务清单](#服务清单)
- [快速部署（Docker Compose）](#快速部署docker-compose)
- [后端模块（Rust）](#后端模块rust)
- [前端（Three.js + Canvas）](#前端threejs--canvas)
- [锭子传感器模拟器](#锭子传感器模拟器)
- [监控与指标](#监控与指标)
- [ClickHouse 保留与降采样策略](#clickhouse-保留与降采样策略)
- [常见问题](#常见问题)

---

## 架构总览

```
                         ┌──────────────────────────────┐
                         │   Spindle Sensor Simulator   │
                         │  8 锭子 / RPM 扫频 / 油膜老化 │
                         └─────────────┬────────────────┘
                                       │  MQTT (1883)
                                       │  topic: spindle/sensor_data
                                       ▼
                    ┌─────────────────────────────────────┐
                    │        Mosquitto  Broker  2.0        │
                    └────┬────────────────────┬───────────┘
                         │                    │
          订阅 sensor    │                    │  发布 alerts
                         ▼                    │
       ┌─────────────────────────────┐       │  topic: spindle/alerts
       │   Rust Backend (tokio)      │       │
       │                             │       │
       │  ┌─────────────────────┐    │       │
       │  │  mqtt_receiver      │───┼───┐   │
       │  │  数据采集 + 校验    │    │   │   │
       │  └──────────┬──────────┘    │   │   │
       │  tokio mpsc │ channel       │   │   │
       │  ┌──────────▼──────────┐    │   │   │
       │  │  dispatch_loop      │◄───┘   │   │
       │  │  路由 & 时序聚合    │        │   │
       │  └──────┬───────┬──────┘        │   │
       │         │       │               │   │
       │   ┌─────▼──┐  ┌─▼────────────┐  │   │
       │   │vibration│  │quality_      │  │   │
       │   │simulator│  │predictor     │  │   │
       │   │转子+油膜│  │LMS回归+磨损  │  │   │
       │   └────┬───┘  └───┬──────────┘  │   │
       │        │          │             │   │
       │     ┌──▼──────────▼───────────┐  │   │
       │     │    dispatch_loop (回)   │  │   │
       │     └─────────────┬───────────┘  │   │
       │                   │              │   │
       │        ┌──────────▼─────────┐    │   │
       │        │   alarm_mqtt       ├────┘   │
       │        │   5类告警 + MQTT   │        │
       │        └──────────┬─────────┘        │
       │                   │                  │
       │        ┌──────────▼──────────┐       │
       │        │   ch_writer         │       │
       │        │   ClickHouse Writer │       │
       │        └──────────┬──────────┘       │
       │                   │                  │
       │    /metrics (prometheus)   /api/*   │
       └─────────────────┬──────────────────────┘
                         │ HTTP (8123 native)
                         ▼
              ┌───────────────────────┐
              │  ClickHouse 24.3      │──► 聚合MV: 1m / 1h / daily
              │  TTL 90d 热 / 2年冷   │
              └───────────┬───────────┘
                          │
           ┌──────────────▼──────────────┐
           │     Prometheus v2.52        │
           │  抓取 /metrics  保留 30 天  │
           └──────────────┬──────────────┘
                          │
                          ▼ 反向代理 + 静态 gzip
                ┌──────────────────────┐
                │   Nginx  1.27 alpine │
                │   /    ->  index.html │
                │   /api -> backend:3000│
                └──────────────────────┘
```

---

## 服务清单

| Service        | Port(s)                       | 默认协议       | 说明                                  |
| -------------- | ----------------------------- | -------------- | ------------------------------------- |
| `mqtt`         | `127.0.0.1:1883`, `9001(WS)`  | MQTT v3.1.1    | Mosquitto，传感器/告警消息总线         |
| `clickhouse`   | `127.0.0.1:8123`, `9000`      | HTTP / Native  | 存储与降采样 (MergeTree+AggregatingMT)|
| `prometheus`   | `127.0.0.1:9090`              | HTTP           | 指标采集器 (抓取 backend:3000/metrics)|
| `backend`      | `127.0.0.1:3000`              | HTTP           | Rust 多模块后端 (静态二进制 distroless)|
| `frontend`     | `127.0.0.1:8080`              | HTTP           | Nginx 静态前端 + gzip + API 反代       |
| `simulator`    | —                             | MQTT client    | 锭子传感器模拟器（`--profile simulator`）|

---

## 快速部署（Docker Compose）

### 0. 前置依赖

- Docker ≥ 24 ＋ Docker Compose V2
- 至少 6 GB 可用内存（ClickHouse + Rust 构建）

### 1. 启动核心服务（不含模拟器）

```bash
docker compose up -d --build
```

等待所有服务 `healthy`：

```bash
watch docker compose ps
```

- 浏览器打开 http://localhost:8080 → 前端三维可视化
- http://localhost:3000/metrics → Prometheus 指标
- http://localhost:9090 → Prometheus 查询界面
- http://localhost:8123/?query=SELECT+1 → ClickHouse 健康检查

### 2. 启动模拟器（可选 profile）

```bash
# 一次 8 锭子 RPM 扫频模式，每 60 秒一批，油膜轻微老化+污染
docker compose --profile simulator up -d --build simulator

# 或者单独运行自定义参数：
docker compose run --rm simulator \
  --host mqtt --port 1883 --interval 10 --spindles 16 \
  --mode sweep --rpm-sweep-min 300 --rpm-sweep-max 4500 --rpm-sweep-minutes 5 \
  --oil-aging 0.6 --oil-contamination 0.25 --inject-worn-idx 3,7-9
```

### 3. 停止 & 清理

```bash
docker compose down               # 保留数据卷
docker compose down -v --rmi all  # 同时删除所有数据卷和镜像
```

---

## 后端模块（Rust）

后端基于 tokio 异步运行时，通过 `tokio::sync::mpsc` channel 把 4 个业务模块完全解耦。
入口：`backend/src/main.rs`。

### 模块清单

| 模块 | 文件 | 职责 |
|-----|------|-----|
| `metrics`          | `backend/src/metrics.rs`          | Prometheus 指标注册 & `/metrics` 导出（12 大类指标） |
| `mqtt_receiver`    | `backend/src/mqtt_receiver.rs`    | MQTT 订阅、JSON 反序列化、6 项数据范围校验            |
| `vibration_simulator` | `backend/src/vibration_simulator.rs` | Jeffcott 临界转速、Reynolds 短轴承、非线性阻尼、半频涡动检测 |
| `quality_predictor`  | `backend/src/quality_predictor.rs` | LMS 增量学习（η=0.01，滑动窗口 50）、磨损双维度建模 |
| `alarm_mqtt`       | `backend/src/alarm_mqtt.rs`       | 5 类告警 2 级严重度 → ClickHouse 持久化 + MQTT 推送 |
| `ch_writer`        | `backend/src/ch_writer.rs`        | 单协程串行通道写入 4 大表，带 ok/err 埋点            |
| `api`              | `backend/src/api.rs`              | axum Router + `/metrics` + TraceLayer + CORS        |
| `config`           | `backend/src/config.rs`           | 从 `APP_CONFIG_PATH` 加载 JSON 配置（多层嵌套 Deserialize） |

### 配置文件

- 路径：`backend/config/app_config.json`
- 可通过环境变量 `APP_CONFIG_PATH` 覆盖
- 配置 8 大块：`mqtt`, `clickhouse`, `api`, `rotor_dynamics`, `oil_film_bearing`, `regression_model`, `validation`, `alert_thresholds`

### 指标清单

Prometheus namespace: `spindle_*`

| Metric 名 | 类型 | Labels | 说明 |
|-----------|------|--------|------|
| `mqtt_messages_total`                 | Counter | —            | 所有接收消息数 |
| `mqtt_messages_invalid_total`         | CounterVec | `error_type` | 校验失败原因（6种+deserialize_error） |
| `sensor_samples_total`                | CounterVec | `spindle_id` | 合法样本按锭子计数 |
| `vibration_analyses_total`            | Counter | —            | 油膜计算次数 |
| `whirl_instability_events_total`      | Counter | —            | 涡动失稳触发次数 |
| `quality_predictions_total`           | Counter | —            | 质量预测次数 |
| `lms_updates_total`                   | Counter | —            | LMS 在线学习参数更新次数 |
| `wear_copermille_x1000`               | Gauge   | —            | 最新磨损系数 ×1000 |
| `alerts_total`                        | CounterVec | `alert_type`, `severity` | 告警总量 |
| `clickhouse_write_total`              | CounterVec | `table`, `status` | 写入量及成败 |
| `http_request_duration_seconds`       | HistogramVec | `endpoint` `method` `status_code` | API 延迟分布 |

### 构建：多阶段 → 静态二进制

`Dockerfile.backend` 2 阶段：

1. **Builder**：`rust:1.78-bookworm` 安装 musl-tools，`x86_64-unknown-linux-musl` 目标 → 产出纯静态二进制
2. **Runtime**：`gcr.io/distroless/static:nonroot` 运行，`nonroot` 用户，镜像约 30 MB

本地验证：

```bash
cd backend
cargo check --release
cargo run --release
```

### 日志与 Tracing

- 环境变量 `RUST_LOG`（默认 `info`）：`error/warn/info/debug/trace`
- 环境变量 `LOG_FORMAT=json`：结构化 JSON 日志（ELK/Loki 友好）

```bash
docker run ... -e RUST_LOG=debug -e LOG_FORMAT=json spindle-backend
```

---

## 前端（Three.js + Canvas）

文件结构：

```
frontend/
├── index.html            # 主 HTML：spindle_3d.js 先于 vibration_panel.js
├── style.css
├── spindle_3d.js         # Three.js 场景、锭子三维建模、纱线 LOD、animate
└── vibration_panel.js    # Canvas 振动波形、API 调用、Demo Loop、告警列表、WS 连接
```

### 跨文件约定接口（spindle_3d.js 暴露 → vibration_panel.js 调用）

| 函数 | 说明 |
|------|------|
| `initThreeScene()`      | 初始化场景/相机/OrbitControls，挂载 DOM               |
| `animate()`             | Three.js 主循环，60 fps 目标 + FPS 统计              |
| `setSimulationData(d)`  | 传入 VibrationResult，振动动画 + 涡动发光             |
| `setCurrentRpm(rpm)`    | 更新纱线 LOD 级别（转速 3 级阈值）                    |
| `getCurrentLodName()`   | 返回当前 LOD：high/medium/low                         |
| `getAverageFps()`       | 最近 60 帧平均 FPS                                    |

### 前端性能优化

| 机制 | 实现 |
|------|------|
| **静态 Gzip 预压缩**    | 构建阶段 `alpine gzip -k -9` 为每个 html/css/js/svg/json 预生成 `.gz` |
| **Gzip 动态压缩**       | Nginx `gzip on`（comp_level 6，min_length 1024）+ `gzip_static on` |
| **强缓存**              | CSS/JS/字体/图片：7 天 `Cache-Control: public, immutable`      |
| **Three.js LOD**        | 锭子纱线三级细节：TubeGeometry（低速）→ LineBasicMaterial → 外壳圆筒（高速） |
| **运动模糊**            | 高速时叠加半透明历史轨迹圆柱                            |

访问入口：http://localhost:8080

---

## 锭子传感器模拟器

`simulator/spindle_simulator.py` —— Python 版全功能模拟器，复刻了 Rust 后端的
Jeffcott 临界转速 + Reynolds 短轴承 Sommerfeld + 涡动增长模型，支持 RPM 模式、
油膜条件、磨损注入。

### 模式一览 (`--mode`)

| mode | 说明 | 相关参数 |
|------|------|----------|
| `random`   | 正弦叠加高斯白噪声（默认），适合稳态测试 | `--rpm-base`, `--rpm-variance` |
| `constant` | 恒定 RPM ± 5% 噪声 | `--rpm-base` |
| `sweep`    | 正弦扫频：完整周期跨越 `[rpm_sweep_min, rpm_sweep_max]`，用来复现临界转速共振 | `--rpm-sweep-min/max/minutes` |
| `step`     | 5 段阶跃（min → max 等分），适合测瞬态油膜响应 | `--rpm-sweep-min/max/minutes` |

### 油膜条件参数

| 参数 | 单位 | 说明 |
|------|------|------|
| `--oil-viscosity`       | Pa·s | 基础润滑油动力粘度，默认 0.01（对应 46# 机械油 40°C）|
| `--oil-clearance`       | m    | 轴承径向间隙，默认 5e-5（50 μm）                     |
| `--oil-aging`           | [0, 2] | 油品老化：0 = 新油，1 = 明显劣化，2 = 严重失效   |
| `--oil-contamination`   | [0, 1] | 固体颗粒污染比例 → 实际粘度上升 + 间隙减小       |

有效粘度和间隙由 `OilFilmCondition.effective_viscosity()/clearance_ratio()` 实时计算；
模拟器随 `temperature` 上升还会叠加温度惩罚。

### 故障注入

```bash
# 指定 3 号、7-9 号锭子为严重磨损（额外 0.6 mm 振动 + 10°C 温度偏移）
python simulator/spindle_simulator.py \
  --host localhost --port 1883 \
  --mode sweep --rpm-sweep-min 500 --rpm-sweep-max 4000 --rpm-sweep-minutes 8 \
  --inject-worn-idx 3,7-9
```

### 常用用法

```bash
# 本地开发：每 10 秒发布一批 8 锭子（不构建 docker）
pip install paho-mqtt
python simulator/spindle_simulator.py --interval 10

# 稳态高压测试：32 锭，恒定 3500 RPM + 污染油
python simulator/spindle_simulator.py \
  --spindles 32 --mode constant --rpm-base 3500 --rpm-variance 100 \
  --oil-viscosity 0.006 --oil-clearance 8e-5 --oil-contamination 0.3

# 只打一轮然后退出（CI 冒烟测试）
python simulator/spindle_simulator.py --once --spindles 4
```

---

## 监控与指标

### Prometheus

打开 http://localhost:9090 ，常用查询：

```promql
# 每秒告警速率（按类型+严重度）
rate(spindle_alerts_total[5m])

# 平均振动位移 vs 时间
avg by (spindle_id) (
  rate(spindle_sensor_samples_total[5m])
)

# P95 API 延迟（30 秒）
histogram_quantile(0.95, sum(rate(spindle_http_request_duration_seconds_bucket[30s])) by (le, endpoint))

# 每锭 LMS 更新频率
rate(spindle_lms_updates_total[5m])
```

### Rust Tracing

所有模块调用 `tracing::{info, warn, error, debug}`，可配置：

- `RUST_LOG=spindle_backend=debug,mqtt_receiver=trace` 精确到模块
- `LOG_FORMAT=json` 打开结构化日志（Loki 或 ELK 直接消费）

---

## ClickHouse 保留与降采样策略

初始化脚本：`sql/init.sql`，Docker 首次启动自动执行。

### 表层分级

| 层级 | 粒度  | 引擎 | 保留期 | 说明 |
|------|------|------|--------|------|
| 原始表（4张）  | 原始事件 | MergeTree         | 90 天    | `spindle_sensor_data`, `vibration_analysis`, `yarn_quality`（alerts 1 年） |
| 1 分钟聚合     | 1 分钟   | AggregatingMergeTree | 30 天 | `*_1m`：avg/min/max/p95/whirl_events/samples |
| 1 小时聚合     | 1 小时   | AggregatingMergeTree | 2 年  | `*_1h`：长期趋势分析 |
| 告警日聚合     | 1 天     | AggregatingMergeTree | 3 年  | `alerts_daily` |

### 物化视图（Materialized Views）

- `spindle_sensor_data_1m_mv` / `..._1h_mv` — 实时聚合传感器原始值
- `vibration_analysis_1m_mv` / `..._1h_mv` — 统计涡动事件峰值
- `yarn_quality_1m_mv` — 跟踪强度/均匀度均值 & 磨损
- `alerts_daily_mv` — 告警 KPI 日报

### 便捷 Dashboard 视图

- `v_current_spindle_status`：每枚锭子的最新一行综合状态（sensor + vibration + quality），可以直接 Grafana 挂接。

### 运维常用查询

```sql
-- 近 10 分钟告警趋势
SELECT
    toStartOfMinute(timestamp) AS t,
    alert_type, severity,
    count() AS c
FROM spindle_system.alerts
WHERE timestamp > now() - INTERVAL 10 MINUTE
GROUP BY t, alert_type, severity ORDER BY t;

-- 1 小时聚合磨损 TOP 10
SELECT spindle_id, maxMerge(wear_coefficient) AS wear
FROM spindle_system.yarn_quality_1h
WHERE bucket_start >= now() - INTERVAL 1 DAY
GROUP BY spindle_id
ORDER BY wear DESC LIMIT 10;
```

---

## 常见问题

**Q1：`cargo build` 时 musl 链接失败？**
> Docker build 中已安装 `musl-tools`。本地直接跑时用默认 GNU target 即可 (`cargo run --release`)；
> 如果本地也要静态，`rustup target add x86_64-unknown-linux-musl` + 安装系统 musl-dev。

**Q2：Prometheus 显示 `connection refused`？**
> backend 需要先通过 healthcheck；`docker compose logs backend` 查看启动日志，确认 `Server listening on 0.0.0.0:3000`。

**Q3：模拟器运行但前端无数据？**
1. `docker compose logs mqtt` 查看连接是否正常；
2. `mosquitto_sub -h localhost -t 'spindle/#'` 验证 MQTT 流；
3. 确认 backend 和 simulator 的 `--topic` 配置相同。

**Q4：ClickHouse 数据量太大，想手动触发 merge / TTL？**
```sql
OPTIMIZE TABLE spindle_system.spindle_sensor_data FINAL;
SYSTEM RELOAD TTL;
```

**Q5：如何把日志推到 Loki？**
Docker Compose 里加 logging driver，或在 backend 上 `LOG_FORMAT=json` 直接让 promtail 采集。
