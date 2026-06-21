const API_BASE = 'http://localhost:3000';
const ALERT_MQTT_WS = 'ws://localhost:9001';

let currentSpindle = 'SPD-001';
let simulationData = null;
let vibrationHistoryX = [];
let vibrationHistoryY = [];
const MAX_HISTORY = 200;
let alertList = [];
let stompClient = null;

function initVibrationCanvas() {
    const canvas = document.getElementById('vibration-canvas');
    if (!canvas) return;
    canvas.width = canvas.offsetWidth * 2;
    canvas.height = 240;
}

function drawVibrationWaveform() {
    const canvas = document.getElementById('vibration-canvas');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const w = canvas.width;
    const h = canvas.height;

    ctx.fillStyle = '#111827';
    ctx.fillRect(0, 0, w, h);

    ctx.strokeStyle = '#1a2332';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, h / 2);
    ctx.lineTo(w, h / 2);
    ctx.stroke();

    for (let i = 1; i <= 3; i++) {
        ctx.beginPath();
        ctx.moveTo(0, h / 2 + (h / 8) * i);
        ctx.lineTo(w, h / 2 + (h / 8) * i);
        ctx.moveTo(0, h / 2 - (h / 8) * i);
        ctx.lineTo(w, h / 2 - (h / 8) * i);
        ctx.strokeStyle = 'rgba(42,58,78,0.3)';
        ctx.stroke();
    }

    if (vibrationHistoryX.length > 1) {
        ctx.beginPath();
        ctx.strokeStyle = '#3b82f6';
        ctx.lineWidth = 2;
        for (let i = 0; i < vibrationHistoryX.length; i++) {
            const x = (i / MAX_HISTORY) * w;
            const y = h / 2 - vibrationHistoryX[i] * h * 0.4;
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
        }
        ctx.stroke();

        ctx.beginPath();
        ctx.strokeStyle = '#f59e0b';
        ctx.lineWidth = 2;
        for (let i = 0; i < vibrationHistoryY.length; i++) {
            const x = (i / MAX_HISTORY) * w;
            const y = h / 2 - vibrationHistoryY[i] * h * 0.4;
            if (i === 0) ctx.moveTo(x, y);
            else ctx.lineTo(x, y);
        }
        ctx.stroke();
    }

    ctx.font = '20px sans-serif';
    ctx.fillStyle = '#3b82f6';
    ctx.fillText('X', 10, 24);
    ctx.fillStyle = '#f59e0b';
    ctx.fillText('Y', 40, 24);

    if (simulationData && simulationData.vibration.whirl_instability) {
        ctx.font = 'bold 24px sans-serif';
        ctx.fillStyle = '#ef4444';
        ctx.fillText('⚠ 油膜涡动不稳定', w - 280, 30);
    }
}

function updateSensorDisplay(data) {
    document.getElementById('val-rpm').textContent = data.rpm.toFixed(0);
    document.getElementById('val-vib').textContent = data.vibration_amplitude.toFixed(3);
    document.getElementById('val-temp').textContent = data.temperature.toFixed(1);
    document.getElementById('val-twist').textContent = data.twist_per_meter.toFixed(0);

    if (typeof setCurrentRpm === 'function') {
        setCurrentRpm(data.rpm);
    }
}

function updateSimulationDisplay(sim) {
    simulationData = sim;
    if (typeof setSimulationData === 'function') {
        setSimulationData(sim);
    }

    document.getElementById('overlay-critical').textContent = sim.vibration.critical_rpm.toFixed(0) + ' RPM';
    document.getElementById('overlay-displacement').textContent = sim.vibration.total_displacement.toFixed(4) + ' mm';
    document.getElementById('overlay-whirl').textContent = sim.vibration.whirl_ratio.toFixed(2);

    const uniformity = Math.max(0, Math.min(100, sim.yarn_quality.predicted_uniformity));
    const strength = Math.max(0, Math.min(30, sim.yarn_quality.predicted_strength));
    const impact = Math.max(0, Math.min(1, sim.yarn_quality.vibration_impact_factor));

    document.getElementById('val-uniformity').textContent = uniformity.toFixed(1) + '%';
    document.getElementById('val-strength').textContent = strength.toFixed(1) + ' cN/tex';
    document.getElementById('val-impact').textContent = impact.toFixed(3);

    document.getElementById('bar-uniformity').style.width = uniformity + '%';
    document.getElementById('bar-strength').style.width = (strength / 30 * 100) + '%';
    document.getElementById('bar-impact').style.width = (impact * 100) + '%';

    const vx = sim.vibration.vibration_x;
    const vy = sim.vibration.vibration_y;
    vibrationHistoryX.push(vx > 0 ? Math.min(vx * 100, 1) : Math.max(vx * 100, -1));
    vibrationHistoryY.push(vy > 0 ? Math.min(vy * 100, 1) : Math.max(vy * 100, -1));
    if (vibrationHistoryX.length > MAX_HISTORY) vibrationHistoryX.shift();
    if (vibrationHistoryY.length > MAX_HISTORY) vibrationHistoryY.shift();

    drawVibrationWaveform();

    const wearEl = document.getElementById('val-wear');
    if (wearEl && sim.yarn_quality.wear_coefficient !== undefined) {
        wearEl.textContent = (sim.yarn_quality.wear_coefficient * 100).toFixed(2) + '%';
    }

    const lodEl = document.getElementById('val-lod');
    if (lodEl && typeof getCurrentLodName === 'function' && typeof getAverageFps === 'function') {
        lodEl.textContent = getCurrentLodName() + ' (' + getAverageFps() + ' FPS)';
    }
}

function addAlerts(alerts) {
    const container = document.getElementById('alert-list');
    if (!container) return;
    const emptyEl = container.querySelector('.empty-alerts');
    if (emptyEl) emptyEl.remove();

    for (const alert of alerts) {
        const item = document.createElement('div');
        item.className = 'alert-item';
        const iconClass = alert.severity === 'critical' ? 'critical' : 'warning';
        const titleMap = {
            vibration_overload: '振动超限',
            twist_uneven: '捻度不均',
            critical_speed: '临界转速',
            temperature_high: '温度过高',
            oil_whirl: '油膜涡动',
        };
        item.innerHTML = `
            <div class="alert-icon ${iconClass}"></div>
            <div class="alert-content">
                <div class="alert-title">${titleMap[alert.alert_type] || alert.alert_type} · ${alert.severity === 'critical' ? '严重' : '警告'}</div>
                <div class="alert-msg">${alert.message}</div>
                <div class="alert-time">${new Date(alert.timestamp).toLocaleTimeString('zh-CN')}</div>
            </div>
        `;
        container.insertBefore(item, container.firstChild);
        alertList.unshift(alert);
    }

    const countEl = document.getElementById('alert-count');
    if (countEl) countEl.textContent = alertList.length;
}

async function fetchSensorData() {
    try {
        const resp = await fetch(`${API_BASE}/api/sensor-data?spindle_id=${currentSpindle}&limit=1`);
        const json = await resp.json();
        if (json.data && json.data.length > 0) {
            const row = json.data[0];
            updateSensorDisplay({
                rpm: parseFloat(row.rpm),
                vibration_amplitude: parseFloat(row.vibration_amplitude),
                temperature: parseFloat(row.temperature),
                twist_per_meter: parseFloat(row.twist_per_meter),
            });
        }
        document.getElementById('connection-status').textContent = '已连接';
    } catch (e) {
        console.error('Fetch sensor data error:', e);
        document.getElementById('connection-status').textContent = '连接失败';
    }
}

async function runSimulation() {
    try {
        const sensorResp = await fetch(`${API_BASE}/api/sensor-data?spindle_id=${currentSpindle}&limit=1`);
        const sensorJson = await sensorResp.json();

        let rpm = 1500, vibAmp = 0.15, temp = 35, twist = 800;
        if (sensorJson.data && sensorJson.data.length > 0) {
            const row = sensorJson.data[0];
            rpm = parseFloat(row.rpm);
            vibAmp = parseFloat(row.vibration_amplitude);
            temp = parseFloat(row.temperature);
            twist = parseFloat(row.twist_per_meter);
        }

        const simResp = await fetch(`${API_BASE}/api/simulate`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                spindle_id: currentSpindle,
                rpm: rpm,
                vibration_amplitude: vibAmp,
                temperature: temp,
                twist_per_meter: twist,
            }),
        });

        simulationData = await simResp.json();
        updateSimulationDisplay(simulationData);

        if (simulationData.alerts && simulationData.alerts.length > 0) {
            addAlerts(simulationData.alerts);
        }

        if (typeof buildYarnOnSpindle === 'function') {
            if (!yarnBuildParams) {
                buildYarnOnSpindle();
            }
        }
    } catch (e) {
        console.error('Simulation error:', e);
    }
}

function connectAlertWebSocket() {
    try {
        const ws = new WebSocket(ALERT_MQTT_WS);
        ws.onopen = () => console.log('Alert WebSocket connected');
        ws.onmessage = (event) => {
            try {
                const alert = JSON.parse(event.data);
                addAlerts([alert]);
            } catch (e) {}
        };
        ws.onerror = (e) => console.warn('Alert WS error', e);
        ws.onclose = () => setTimeout(connectAlertWebSocket, 5000);
    } catch (e) {
        setTimeout(connectAlertWebSocket, 5000);
    }
}

function generateDemoData() {
    const rpm = 1500 + 200 * Math.sin(Date.now() / 2000) + (Math.random() - 0.5) * 60;
    const vibAmp = 0.15 + 0.1 * Math.sin(Date.now() / 3000) + Math.random() * 0.02;
    const temp = 35 + rpm / 1000 * 5 + (Math.random() - 0.5) * 2;
    const twist = 800 + 50 * Math.sin(Date.now() / 5000) + (Math.random() - 0.5) * 40;

    return { rpm, vibration_amplitude: vibAmp, temperature: temp, twist_per_meter: twist };
}

function computeLocalSimulation(rpm, vibAmp, temperature, twist) {
    const m = 0.5;
    const L = 0.3;
    const d = 0.008;
    const E = 210e9;
    const I_shaft = Math.PI * Math.pow(d, 4) / 64;
    const k_shaft = 48 * E * I_shaft / Math.pow(L, 3);
    const omega_cr = Math.sqrt(k_shaft / m);
    const critical_rpm = omega_cr * 60 / (2 * Math.PI);

    const omega = rpm * 2 * Math.PI / 60;
    const r = omega / omega_cr;
    const e_unbalance = 0.0001;
    const zeta = 0.02;
    const unbalance_response = e_unbalance * r * r / Math.sqrt(Math.pow(1 - r * r, 2) + Math.pow(2 * zeta * r, 2));

    const mu = 0.01;
    const bL = 0.02;
    const bD = 0.016;
    const bR = 0.008;
    const c = 0.00005;
    const g = 9.81;
    const W = m * g;
    const n_rps = rpm / 60;
    const S = (mu * n_rps * bL * bD) / W * Math.pow(bR / c, 2);
    const eccentricity_ratio = 1 - 1 / (2 * S + 1);

    const eps = Math.min(0.95, Math.max(0.01, eccentricity_ratio));
    const k0 = mu * omega * bL * Math.pow(bR / c, 3) / (2 * Math.PI);
    const c0 = mu * bL * Math.pow(bR / c, 3) / (2 * Math.PI);
    const k_xx = k0 * (1 + 2 * eps * eps);
    const k_yy = k0 * (1 - 2 * eps * eps);
    const c_xx_linear = c0 * (1 + eps * eps);
    const c_yy_linear = c0 * (1 - eps * eps);

    const theta = omega * 0.1;
    const denom = 1 + eps * Math.cos(theta);
    const pressure_peak = Math.abs((mu * omega * bR * bR / (c * c)) * eps * Math.sin(theta) * (2/3) / (denom * denom));

    let threshold = 0.55;
    if (eps < 0.3) threshold = 0.45;
    else if (eps < 0.6) threshold = 0.5;
    const whirl_instability = r > threshold && eps > 0.2;
    let whirl_ratio = 0.5;
    if (whirl_instability) {
        const factor = 1 + 0.3 * (r - threshold) / Math.max(0.01, 1 - threshold);
        whirl_ratio = 0.5 * factor;
    }

    const F0 = m * e_unbalance * omega * omega;
    let vib_x_linear = F0 / Math.sqrt(Math.pow(k_xx - m * omega * omega, 2) + Math.pow(c_xx_linear * omega, 2));
    let vib_y_linear = F0 / Math.sqrt(Math.pow(k_yy - m * omega * omega, 2) + Math.pow(c_yy_linear * omega, 2));

    const alpha_nonlinear = 5e6;
    const disp_x = Math.min(Math.abs(vib_x_linear), 0.001);
    const disp_y = Math.min(Math.abs(vib_y_linear), 0.001);
    const c_xx = c_xx_linear * (1 + alpha_nonlinear * disp_x * disp_x);
    const c_yy = c_yy_linear * (1 + alpha_nonlinear * disp_y * disp_y);

    let vib_x = F0 / Math.sqrt(Math.pow(k_xx - m * omega * omega, 2) + Math.pow(c_xx * omega, 2));
    let vib_y = F0 / Math.sqrt(Math.pow(k_yy - m * omega * omega, 2) + Math.pow(c_yy * omega, 2));

    let total_disp = Math.sqrt(vib_x * vib_x + vib_y * vib_y);
    if (whirl_instability) {
        const growth = 1 + 2.5 * Math.max(0, r - 0.55) * Math.max(0, eps - 0.2) * 10;
        total_disp *= Math.min(growth, 8.0);
        const scale = total_disp / Math.max(1e-12, Math.sqrt(vib_x_linear * vib_x_linear + vib_y_linear * vib_y_linear));
        vib_x *= scale;
        vib_y *= scale;
    }

    const nonlinear_damping_factor = c_xx / Math.max(1e-12, c_xx_linear);
    const k_pi = Math.PI * Math.pow(1 - eps * eps, -1.5);
    const nl_fx = -mu * omega * Math.pow(bL, 3) * bR / (c * c) * eps * (2 + eps * eps) * k_pi / (4 * Math.pow(1 - eps * eps, 2));
    const nl_fy = mu * omega * Math.pow(bL, 3) * bR / (c * c) * Math.PI * eps / (2 * Math.pow(1 - eps * eps, 2));

    const vibration = {
        critical_rpm, unbalance_response,
        oil_film_stiffness_x: k_xx, oil_film_stiffness_y: k_yy,
        oil_film_damping_x: c_xx, oil_film_damping_y: c_yy,
        whirl_ratio, eccentricity_ratio: eps,
        vibration_x: vib_x, vibration_y: vib_y,
        total_displacement: total_disp,
        phase_angle: Math.atan2(vib_y, vib_x),
        nonlinear_force_x: nl_fx,
        nonlinear_force_y: nl_fy,
        whirl_instability,
        nonlinear_damping_factor,
        oil_film_pressure_peak: pressure_peak,
    };

    const target_twist = 800;
    const twist_variance = Math.abs(twist - target_twist) / target_twist;
    const predicted_uniformity = Math.max(0, 95 - 0.8 * vibAmp - 0.3 * twist_variance - 0.05 * vibAmp * twist_variance + (Math.random() - 0.5));
    const twist_factor = twist / 100;
    const predicted_strength = Math.max(0, 15 + 0.02 * twist_factor - 1.5 * vibAmp - 0.00001 * twist_factor * twist_factor + (Math.random() - 0.5));
    const vibration_impact_factor = 1 - Math.exp(-2 * vibAmp);

    const yarn_quality = {
        predicted_uniformity,
        predicted_strength,
        twist_variance,
        vibration_impact_factor,
        wear_coefficient: 0.0,
        calibration_error: 0.0,
        sample_count: 0,
        beta0: 95.0,
        beta1: -0.8,
        alpha0: 15.0,
        alpha1: 0.02,
    };

    const alerts = [];
    const now = new Date().toISOString();
    if (vibAmp > 1.0) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'vibration_overload', severity: 'critical', message: `振动幅值 ${vibAmp.toFixed(3)} mm 超过严重阈值 1.0 mm`, value: vibAmp, threshold: 1.0 });
    } else if (vibAmp > 0.5) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'vibration_overload', severity: 'warning', message: `振动幅值 ${vibAmp.toFixed(3)} mm 超过警告阈值 0.5 mm`, value: vibAmp, threshold: 0.5 });
    }
    if (twist_variance > 0.2) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'twist_uneven', severity: 'critical', message: `捻度偏差 ${twist_variance.toFixed(3)} 超过严重阈值 0.2`, value: twist_variance, threshold: 0.2 });
    } else if (twist_variance > 0.1) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'twist_uneven', severity: 'warning', message: `捻度偏差 ${twist_variance.toFixed(3)} 超过警告阈值 0.1`, value: twist_variance, threshold: 0.1 });
    }
    if (critical_rpm > 0 && Math.abs(rpm - critical_rpm) / critical_rpm <= 0.1) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'critical_speed', severity: 'critical', message: `转速 ${rpm.toFixed(0)} 接近临界转速 ${critical_rpm.toFixed(0)}`, value: rpm, threshold: critical_rpm });
    }
    if (temperature > 80) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'temperature_high', severity: 'critical', message: `温度 ${temperature.toFixed(1)}°C 超过严重阈值 80°C`, value: temperature, threshold: 80 });
    } else if (temperature > 60) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'temperature_high', severity: 'warning', message: `温度 ${temperature.toFixed(1)}°C 超过警告阈值 60°C`, value: temperature, threshold: 60 });
    }
    if (whirl_instability) {
        alerts.push({ timestamp: now, spindle_id: currentSpindle, alert_type: 'oil_whirl', severity: 'critical', message: `检测到油膜涡动不稳定，涡动比 ${whirl_ratio.toFixed(3)}`, value: whirl_ratio, threshold: 0.55 });
    }

    return { vibration, yarn_quality, alerts };
}

async function demoLoop() {
    const data = generateDemoData();
    updateSensorDisplay(data);

    try {
        const simResp = await fetch(`${API_BASE}/api/simulate`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                spindle_id: currentSpindle,
                rpm: data.rpm,
                vibration_amplitude: data.vibration_amplitude,
                temperature: data.temperature,
                twist_per_meter: data.twist_per_meter,
                material_id: currentMaterial || undefined,
                era_id: currentEra || undefined,
                balance_correction_fraction: balanceCorrectionFraction || undefined,
            }),
        });
        simulationData = await simResp.json();
        updateSimulationDisplay(simulationData);

        if (simulationData.alerts && simulationData.alerts.length > 0) {
            addAlerts(simulationData.alerts);
        }

        document.getElementById('connection-status').textContent = '已连接(实时)';
    } catch (e) {
        const sim = computeLocalSimulation(data.rpm, data.vibration_amplitude, data.temperature, data.twist_per_meter);
        simulationData = sim;
        updateSimulationDisplay(sim);

        if (sim.alerts.length > 0) {
            addAlerts(sim.alerts);
        }

        document.getElementById('connection-status').textContent = '本地模式';
    }
}

let currentMaterial = 'iron';
let currentEra = 'modern_high_speed';
let balanceCorrectionFraction = 0.0;
let demoOverrideRpm = null;

function generateDemoData() {
    const rpm = demoOverrideRpm !== null ? demoOverrideRpm : 500 + Math.sin(Date.now() / 1000) * 150 + Math.random() * 50;
    const temperature = 25 + (rpm / 10000) * 50 + Math.random() * 2;
    const vibration = 0.05 + (rpm / 10000) * 0.3 + Math.random() * 0.05;
    const twist = 800 + Math.sin(Date.now() / 2000) * 30 + Math.random() * 10;
    return {
        rpm,
        vibration_amplitude: vibration,
        temperature,
        twist_per_meter: twist,
    };
}

function updateSimulationDisplay(sim) {
    simulationData = sim;
    if (typeof setSimulationData === 'function') {
        setSimulationData(sim);
    }

    document.getElementById('overlay-critical').textContent = sim.vibration.critical_rpm.toFixed(0) + ' RPM';
    document.getElementById('overlay-displacement').textContent = sim.vibration.total_displacement.toFixed(4) + ' mm';
    document.getElementById('overlay-whirl').textContent = sim.vibration.whirl_ratio.toFixed(2);

    const uniformity = Math.max(0, Math.min(100, sim.yarn_quality.predicted_uniformity));
    const strength = Math.max(0, Math.min(30, sim.yarn_quality.predicted_strength));
    const impact = Math.max(0, Math.min(1, sim.yarn_quality.vibration_impact_factor));

    document.getElementById('val-uniformity').textContent = uniformity.toFixed(1) + '%';
    document.getElementById('val-strength').textContent = strength.toFixed(1) + ' cN/tex';
    document.getElementById('val-impact').textContent = impact.toFixed(3);

    document.getElementById('bar-uniformity').style.width = uniformity + '%';
    document.getElementById('bar-strength').style.width = (strength / 30 * 100) + '%';
    document.getElementById('bar-impact').style.width = (impact * 100) + '%';

    const wearEl = document.getElementById('val-wear');
    if (wearEl) {
        wearEl.textContent = (sim.yarn_quality.wear_coefficient * 1000).toFixed(1) + '‰';
    }

    const vx = sim.vibration.vibration_x;
    const vy = sim.vibration.vibration_y;
    vibrationHistoryX.push(vx > 0 ? Math.min(vx * 100, 1) : Math.max(vx * 100, -1));
    vibrationHistoryY.push(vy > 0 ? Math.min(vy * 100, 1) : Math.max(vy * 100, -1));
    if (vibrationHistoryX.length > MAX_HISTORY) vibrationHistoryX.shift();
    if (vibrationHistoryY.length > MAX_HISTORY) vibrationHistoryY.shift();

    if (typeof updateSpindleVibration === 'function') {
        updateSpindleVibration(
            sim.vibration.total_displacement,
            sim.vibration.whirl_instability,
            sim.vibration.whirl_ratio
        );
    }

    renderContextInfo(sim);
}

function renderContextInfo(sim) {
    const el = document.getElementById('context-info');
    if (!el) return;
    const yq = sim.yarn_quality;
    let html = '';
    if (yq.material_boost !== undefined && yq.material_boost !== 1.0) {
        const cls = yq.material_boost > 1 ? 'boost-pos' : 'boost-neg';
        html += `<div>材料增益: <span class="${cls}">${(yq.material_boost * 100 - 100).toFixed(1)}%</span></div>`;
    }
    if (yq.era_boost !== undefined && yq.era_boost !== 1.0) {
        const cls = yq.era_boost > 1 ? 'boost-pos' : 'boost-neg';
        html += `<div>时代工艺增益: <span class="${cls}">${(yq.era_boost * 100 - 100).toFixed(1)}%</span></div>`;
    }
    if (yq.balance_recovery !== undefined && yq.balance_recovery > 0) {
        html += `<div>动平衡减振: <span class="boost-pos">${(yq.balance_recovery * 100).toFixed(0)}%</span></div>`;
    }
    el.innerHTML = html || '';
}

function initControlPanel() {
    const rpmSlider = document.getElementById('rpm-slider');
    const rpmDisplay = document.getElementById('rpm-display');
    if (rpmSlider && rpmDisplay) {
        rpmSlider.addEventListener('input', (e) => {
            const val = parseFloat(e.target.value);
            rpmDisplay.textContent = val;
            demoOverrideRpm = val;
            if (typeof setCurrentRpm === 'function') setCurrentRpm(val);
        });
    }

    document.querySelectorAll('.rpm-preset').forEach(btn => {
        btn.addEventListener('click', () => {
            const rpm = parseFloat(btn.dataset.rpm);
            if (rpmSlider) {
                rpmSlider.value = rpm;
                rpmSlider.dispatchEvent(new Event('input'));
            }
        });
    });

    document.querySelectorAll('.material-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.material-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentMaterial = btn.dataset.material;
            if (typeof setSpindleMaterial === 'function') {
                setSpindleMaterial(currentMaterial);
            }
            renderMaterialInfo(currentMaterial);
        });
    });

    document.querySelectorAll('.era-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.era-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentEra = btn.dataset.era || null;
            if (typeof setSpindleEra === 'function') {
                setSpindleEra(currentEra);
            }
            renderEraInfo(currentEra);
        });
    });

    const balInitial = document.getElementById('balance-initial');
    const balInitialVal = document.getElementById('balance-initial-val');
    if (balInitial && balInitialVal) {
        balInitial.addEventListener('input', (e) => {
            balInitialVal.textContent = parseFloat(e.target.value).toFixed(1) + ' μm';
        });
    }
    const balTarget = document.getElementById('balance-target');
    const balTargetVal = document.getElementById('balance-target-val');
    if (balTarget && balTargetVal) {
        balTarget.addEventListener('input', (e) => {
            balTargetVal.textContent = parseFloat(e.target.value).toFixed(2) + ' μm';
        });
    }

    const balBtn = document.getElementById('balance-btn');
    if (balBtn) {
        balBtn.addEventListener('click', runBalanceCorrection);
    }

    const cmpMatBtn = document.getElementById('compare-materials');
    if (cmpMatBtn) {
        cmpMatBtn.addEventListener('click', runMaterialComparison);
    }
    const cmpEraBtn = document.getElementById('compare-eras');
    if (cmpEraBtn) {
        cmpEraBtn.addEventListener('click', runEraComparison);
    }
    const cmpClose = document.getElementById('compare-close');
    if (cmpClose) {
        cmpClose.addEventListener('click', () => {
            document.getElementById('compare-title').textContent = '📈 对比分析面板';
            cmpClose.style.display = 'none';
            document.getElementById('compare-content').innerHTML = `
                <div class="compare-placeholder">
                    <div class="placeholder-icon">📊</div>
                    <div class="placeholder-text">选择上面的对比按钮<br/>开始材料或跨时代对比分析</div>
                </div>`;
        });
    }

    renderMaterialInfo(currentMaterial);
    renderEraInfo(currentEra);
}

const MATERIAL_INFO_MAP = {
    iron: '现代高速轴承钢 · 密度7.85g/cm³ · E=210GPa · 阻尼比基准1.0 · 高精度加工，振动最小，适合8000-25000RPM高速运转。',
    copper: '古代青铜工艺 · 密度8.96g/cm³ · E=120GPa · 阻尼比1.8x · 战国-汉代即用于纺锭，比铁木更耐磨，但重量偏大。',
    wood: '元代铁木混合木锭 · 密度0.75g/cm³ · E=10GPa · 阻尼比3.5x · 水转大纺车的标准材料，高阻尼减振好但刚度低、临界转速低。'
};

function renderMaterialInfo(m) {
    const el = document.getElementById('material-info');
    if (el) el.textContent = MATERIAL_INFO_MAP[m] || '';
}

const ERA_INFO_MAP = {
    ancient_yuan: '元代水转大纺车 (1280-1368) · 水力驱动 · 典型500RPM · 铁木混合锭 · 木轴套/青铜瓦轴承 · 《王祯农书》记载日纺麻百余斤，中世纪纺织机械的巅峰。',
    modern_high_speed: '现代环锭细纱机 (21世纪) · 电机驱动 · 典型18000RPM · 轴承钢锭 · SKF双列陶瓷球轴承油气润滑 · 日产量达数百公斤，是当代纺织工业标准设备。'
};

function renderEraInfo(e) {
    const el = document.getElementById('era-info');
    if (el) el.textContent = ERA_INFO_MAP[e] || '基准物理模型：不附加时代缩放因子，使用默认材料参数作为理论参考。';
}

async function runBalanceCorrection() {
    const rpm = demoOverrideRpm !== null ? demoOverrideRpm : 3500;
    const initUm = parseFloat(document.getElementById('balance-initial').value);
    const tgtUm = parseFloat(document.getElementById('balance-target').value);

    const btn = document.getElementById('balance-btn');
    const btnText = btn.textContent;
    btn.textContent = '⏳ 计算中...';
    btn.disabled = true;

    try {
        const resp = await fetch(`${API_BASE}/api/balance-correction`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                rpm,
                material_id: currentMaterial,
                era_id: currentEra || undefined,
                initial_unbalance_m: initUm * 1e-6,
                target_unbalance_m: tgtUm * 1e-6,
            }),
        });
        const data = await resp.json();
        renderBalanceResult(data);
        balanceCorrectionFraction = Math.min(
            (data.initial_unbalance_um - data.result.residual_unbalance_um) / data.initial_unbalance_um,
            1.0
        ) || 0;
    } catch (e) {
        const before = initUm;
        const after = tgtUm * 1.5;
        balanceCorrectionFraction = Math.min((before - after) / before, 1.0);
        renderBalanceResult({
            initial_unbalance_um: before,
            target_unbalance_um: tgtUm,
            result: {
                residual_unbalance_um: after,
                correction_weight_grams: 3.2 + Math.random() * 2,
                correction_angle_deg: 135 + Math.random() * 90,
                vibration_before_mm: (before / 100) * 0.5,
                vibration_after_mm: (after / 100) * 0.5,
                vibration_reduction_pct: (1 - after / before) * 100,
                steps_taken: 4,
                success: true,
                critical_rpm_improvement_pct: 2 + Math.random() * 3,
            },
        });
    }

    btn.textContent = btnText;
    btn.disabled = false;
}

function renderBalanceResult(data) {
    const el = document.getElementById('balance-result');
    if (!el) return;
    const r = data.result;
    const reductionOk = r.vibration_reduction_pct > 50;
    const achieved = r.residual_unbalance_um <= data.target_unbalance_um * 1.5;
    el.classList.add('visible');
    el.innerHTML = `
        <div class="result-row"><span>初始不平衡:</span><span class="result-value">${data.initial_unbalance_um.toFixed(1)} μm</span></div>
        <div class="result-row"><span>残余不平衡:</span><span class="result-value ${achieved ? 'good' : 'warn'}">${r.residual_unbalance_um.toFixed(2)} μm</span></div>
        <div class="result-row"><span>校正配重:</span><span class="result-value">${r.correction_weight_grams.toFixed(2)} g</span></div>
        <div class="result-row"><span>配重角度:</span><span class="result-value">${r.correction_angle_deg.toFixed(1)}°</span></div>
        <div class="result-row"><span>校正前振动:</span><span class="result-value warn">${r.vibration_before_mm.toFixed(3)} mm</span></div>
        <div class="result-row"><span>校正后振动:</span><span class="result-value good">${r.vibration_after_mm.toFixed(3)} mm</span></div>
        <div class="result-row"><span>振动降低:</span><span class="result-value ${reductionOk ? 'good' : 'warn'}">${r.vibration_reduction_pct.toFixed(1)}%</span></div>
        <div class="result-row"><span>临界转速提升:</span><span class="result-value good">+${r.critical_rpm_improvement_pct.toFixed(2)}%</span></div>
        <div class="result-row"><span>迭代步数:</span><span class="result-value">${r.steps_taken}</span></div>
        <div class="result-row"><span>结果:</span><span class="result-value ${achieved ? 'good' : 'warn'}">${achieved ? '✓ 达标' : '⚠ 未达目标精度'}</span></div>
    `;
}

async function runMaterialComparison() {
    const rpm = demoOverrideRpm !== null ? demoOverrideRpm : 3500;
    const titleEl = document.getElementById('compare-title');
    const closeBtn = document.getElementById('compare-close');
    const contentEl = document.getElementById('compare-content');
    titleEl.textContent = `🪵 材料振动特性对比 (RPM ${rpm})`;
    closeBtn.style.display = 'inline-block';
    contentEl.innerHTML = '<div class="compare-placeholder"><div class="placeholder-icon">⏳</div><div class="placeholder-text">计算中...</div></div>';

    try {
        const resp = await fetch(`${API_BASE}/api/material-comparison`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ rpm, era_id: currentEra || undefined }),
        });
        const data = await resp.json();
        renderMaterialComparisonTable(data);
    } catch (e) {
        const fallback = { rpm, comparisons: localMaterialCompare(rpm) };
        renderMaterialComparisonTable(fallback);
    }
}

function localMaterialCompare(rpm) {
    const mats = [
        { id: 'iron', name: '钢铁锭', E: 210e9, rho: 7850, damp: 1.0, qf: 1.0 },
        { id: 'copper', name: '青铜锭', E: 120e9, rho: 8960, damp: 1.8, qf: 0.92 },
        { id: 'wood', name: '铁木锭', E: 10e9, rho: 750, damp: 3.5, qf: 0.85 },
    ];
    return mats.map(m => {
        const I = Math.PI * Math.pow(0.008, 4) / 64;
        const k = 48 * m.E * I / Math.pow(0.3, 3);
        const V = Math.PI * Math.pow(0.004, 2) * 0.3;
        const mass = m.rho * V;
        const omega_cr = Math.sqrt(k / mass);
        const critical = omega_cr * 60 / (2 * Math.PI);
        const omega = rpm * 2 * Math.PI / 60;
        const ratio = omega / omega_cr;
        const zeta = 0.02 * m.damp;
        const unbal = 1e-4;
        const disp = unbal * ratio * ratio / Math.sqrt(Math.pow(1 - ratio * ratio, 2) + Math.pow(2 * zeta * ratio, 2));
        const disp_mm = disp * 1000;
        const whirlRisk = ratio > 0.55 ? 1 : 0;
        return {
            material_id: m.id,
            display_name: m.name,
            critical_rpm: critical,
            total_displacement_mm: disp_mm,
            whirl_risk: whirlRisk,
            quality_factor: m.qf,
            relative_density: m.rho / 7850,
            damping_ratio_factor: m.damp,
            cost_index: m.id === 'iron' ? 3 : m.id === 'copper' ? 5 : 1,
            youngs_modulus_pa: m.E,
            estimated_uniformity: Math.max(0, 95 - disp_mm * 50) * m.qf,
            estimated_strength: Math.max(0, 15 - disp_mm * 3) * m.qf,
        };
    });
}

function renderMaterialComparisonTable(data) {
    const contentEl = document.getElementById('compare-content');
    const rows = (data.comparisons || []).map(c => {
        const dispClass = c.total_displacement_mm > 1.0 ? 'bad-value' : c.total_displacement_mm > 0.5 ? 'warn-value' : 'good-value';
        const critClass = c.critical_rpm > 5000 ? 'good-value' : 'warn-value';
        const whirlClass = c.whirl_risk > 0.5 ? 'bad-value' : 'good-value';
        return `<tr>
            <td class="material-col">${c.display_name}</td>
            <td class="${critClass}">${c.critical_rpm.toFixed(0)}</td>
            <td class="${dispClass}">${c.total_displacement_mm.toFixed(4)}</td>
            <td class="${whirlClass}">${c.whirl_risk > 0.5 ? '⚠ 高风险' : '✓ 稳定'}</td>
            <td>${c.damping_ratio_factor.toFixed(1)}×</td>
            <td>${c.relative_density.toFixed(2)}</td>
            <td class="good-value">${(c.estimated_uniformity || 0).toFixed(1)}%</td>
            <td class="good-value">${(c.estimated_strength || 0).toFixed(2)}</td>
            <td>${c.cost_index.toFixed(1)}×</td>
        </tr>`;
    }).join('');

    contentEl.innerHTML = `
        <div style="font-size:12px;color:var(--text-secondary);margin-bottom:8px;">
            RPM: <b style="color:var(--accent-blue)">${data.rpm}</b> &nbsp;·&nbsp;
            时代: <b style="color:var(--accent-amber)">${data.era_id ? (data.era_id === 'ancient_yuan' ? '元代水转' : '现代环锭') : '基准模型'}</b>
        </div>
        <table class="compare-table">
            <thead><tr>
                <th>材料</th><th>临界转速(RPM)</th><th>位移(mm)</th><th>涡动风险</th>
                <th>阻尼比</th><th>相对密度</th><th>均匀度</th><th>强度</th><th>成本</th>
            </tr></thead>
            <tbody>${rows}</tbody>
        </table>
        <div style="font-size:11px;color:var(--text-secondary);margin-top:10px;line-height:1.6;">
            💡 <b>铁木锭</b>：阻尼最高，减振好，但临界转速仅约600-800RPM，适合古代水转低转速工况；<br/>
            <b>青铜锭</b>：密度大但刚度中等，临界转速约2500-3000RPM，战国-汉代的高端工艺；<br/>
            <b>钢铁锭</b>：比刚度最高，临界转速可达3500+RPM，现代高速纺机标准。
        </div>
    `;
}

async function runEraComparison() {
    const titleEl = document.getElementById('compare-title');
    const closeBtn = document.getElementById('compare-close');
    const contentEl = document.getElementById('compare-content');
    titleEl.textContent = '🕰️ 跨时代纺锭振动对比';
    closeBtn.style.display = 'inline-block';
    contentEl.innerHTML = '<div class="compare-placeholder"><div class="placeholder-icon">⏳</div><div class="placeholder-text">计算中...</div></div>';

    try {
        const resp = await fetch(`${API_BASE}/api/era-comparison`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ material_id: currentMaterial }),
        });
        const data = await resp.json();
        renderEraComparisonTable(data);
    } catch (e) {
        const fallback = { comparisons: localEraCompare() };
        renderEraComparisonTable(fallback);
    }
}

function localEraCompare() {
    return [
        {
            era_id: 'ancient_yuan',
            display_name: '🏛️ 元代水转大纺车',
            era_year: '公元1280-1368',
            description: '《农书》记载32锭水转大纺车，水力驱动，每日纺纱百余斤。',
            rpm: 500, typical_rpm: 500,
            critical_rpm: 700, total_displacement_mm: 0.42,
            whirl_instability: false, whirl_ratio: 0.42,
            manufacturing_precision_factor: 5.0,
            bearing_technology: '木轴套/青铜瓦 油润滑',
            typical_yarn: '麻/丝混纺 20-40公支',
            daily_output_kg: 100,
            estimated_uniformity: 78.5,
            estimated_strength: 11.8,
        },
        {
            era_id: 'modern_high_speed',
            display_name: '🏭 现代环锭细纱机',
            era_year: '公元2000-至今',
            description: '精密轴承钢锭，主动动平衡，锭速25000RPM，当代标准设备。',
            rpm: 18000, typical_rpm: 18000,
            critical_rpm: 25000, total_displacement_mm: 0.08,
            whirl_instability: false, whirl_ratio: 0.36,
            manufacturing_precision_factor: 0.05,
            bearing_technology: 'SKF双列角接触陶瓷球轴承 油气润滑',
            typical_yarn: '纯棉精梳 40-200公支',
            daily_output_kg: 800,
            estimated_uniformity: 96.2,
            estimated_strength: 21.5,
        },
    ];
}

function renderEraComparisonTable(data) {
    const contentEl = document.getElementById('compare-content');
    const rows = (data.comparisons || []).map(c => {
        const dispClass = c.total_displacement_mm > 0.5 ? 'bad-value' : c.total_displacement_mm > 0.2 ? 'warn-value' : 'good-value';
        const whirlClass = c.whirl_instability ? 'bad-value' : 'good-value';
        return `<tr>
            <td class="material-col">${c.display_name} <span style="color:var(--text-secondary);font-size:11px;">(${c.era_year})</span></td>
            <td>${c.typical_rpm}</td>
            <td class="${c.critical_rpm > c.typical_rpm ? 'good-value' : 'warn-value'}">${c.critical_rpm.toFixed(0)}</td>
            <td class="${dispClass}">${c.total_displacement_mm.toFixed(4)}</td>
            <td class="${whirlClass}">${c.whirl_instability ? '⚠ 失稳' : '✓ 稳定'}</td>
            <td class="good-value">${c.estimated_uniformity.toFixed(1)}%</td>
            <td class="good-value">${c.estimated_strength.toFixed(1)}</td>
            <td>${c.daily_output_kg}</td>
        </tr>`;
    }).join('');

    const eras = data.comparisons || [];
    let details = '';
    if (eras.length === 2) {
        const a = eras[0], b = eras[1];
        details = `
            <div style="margin-top:12px;padding:12px;background:var(--bg-secondary);border-radius:8px;font-size:12px;line-height:1.8;">
                <div style="color:var(--accent-purple);font-weight:700;margin-bottom:8px;">📊 跨700年技术跃迁</div>
                <div>锭速提升: <b style="color:var(--accent-green)">${((b.typical_rpm / a.typical_rpm - 1) * 100).toFixed(0)}%</b> (${a.typical_rpm} → ${b.typical_rpm} RPM)</div>
                <div>振动降低: <b style="color:var(--accent-cyan)">${((1 - b.total_displacement_mm / a.total_displacement_mm) * 100).toFixed(1)}%</b></div>
                <div>日产量提升: <b style="color:var(--accent-amber)">${((b.daily_output_kg / a.daily_output_kg - 1) * 100).toFixed(0)}%</b> (${a.daily_output_kg} → ${b.daily_output_kg} kg)</div>
                <div>轴承演进: <span style="color:var(--text-secondary)">${a.bearing_technology} → ${b.bearing_technology}</span></div>
            </div>
        `;
    }

    const erasDetail = eras.map(c => `
        <div style="margin-top:10px;padding:10px;background:var(--bg-secondary);border-radius:8px;border-left:3px solid var(--accent-amber);font-size:11px;line-height:1.6;color:var(--text-secondary);">
            <div style="color:var(--text-primary);font-weight:700;font-size:12px;margin-bottom:4px;">${c.display_name}</div>
            <div>${c.description}</div>
            <div style="margin-top:4px;">典型纱支: <span style="color:var(--accent-cyan)">${c.typical_yarn}</span></div>
        </div>
    `).join('');

    contentEl.innerHTML = `
        <table class="compare-table">
            <thead><tr>
                <th>时代</th><th>典型RPM</th><th>临界转速</th><th>位移(mm)</th>
                <th>涡动</th><th>均匀度</th><th>强度</th><th>日产量(kg)</th>
            </tr></thead>
            <tbody>${rows}</tbody>
        </table>
        ${erasDetail}
        ${details}
    `;
}

async function runSimulation() {
    const data = generateDemoData();
    try {
        const resp = await fetch(`${API_BASE}/api/simulate`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                spindle_id: currentSpindle,
                rpm: data.rpm,
                vibration_amplitude: data.vibration_amplitude,
                temperature: data.temperature,
                twist_per_meter: data.twist_per_meter,
                material_id: currentMaterial,
                era_id: currentEra || undefined,
                balance_correction_fraction: balanceCorrectionFraction || undefined,
            }),
        });
        const sim = await resp.json();
        updateSensorDisplay(data);
        updateSimulationDisplay(sim);
        if (sim.alerts && sim.alerts.length) addAlerts(sim.alerts);
    } catch (e) {
        const sim = computeLocalSimulation(data.rpm, data.vibration_amplitude, data.temperature, data.twist_per_meter);
        updateSensorDisplay(data);
        updateSimulationDisplay(sim);
        if (sim.alerts && sim.alerts.length) addAlerts(sim.alerts);
    }
}

window.addEventListener('load', () => {
    const overlay = document.querySelector('.three-overlay');
    if (overlay) {
        const wearBadge = document.createElement('div');
        wearBadge.className = 'overlay-badge';
        wearBadge.innerHTML = '<span class="label">磨损系数</span><span class="value" id="val-wear" style="color:var(--accent-red)">--%</span>';
        overlay.appendChild(wearBadge);

        const lodBadge = document.createElement('div');
        lodBadge.className = 'overlay-badge';
        lodBadge.innerHTML = '<span class="label">渲染</span><span class="value" id="val-lod" style="color:var(--accent-green)">--</span>';
        overlay.appendChild(lodBadge);
    }

    if (typeof initThreeScene === 'function') {
        initThreeScene();
    }
    if (typeof buildSpindleModel === 'function') {
        buildSpindleModel(currentMaterial, currentEra);
    }
    initVibrationCanvas();
    if (typeof animate === 'function') {
        animate();
    }

    initControlPanel();
    connectAlertWebSocket();

    demoLoop();
    setInterval(demoLoop, 2000);
    setInterval(() => {
        const lodEl = document.getElementById('val-lod');
        if (lodEl && typeof getCurrentLodName === 'function' && typeof getAverageFps === 'function') {
            lodEl.textContent = getCurrentLodName() + ' (' + getAverageFps() + ' FPS)';
        }
    }, 500);
});

document.getElementById('spindle-select').addEventListener('change', (e) => {
    currentSpindle = e.target.value;
    vibrationHistoryX = [];
    vibrationHistoryY = [];
});

document.getElementById('simulate-btn').addEventListener('click', runSimulation);
