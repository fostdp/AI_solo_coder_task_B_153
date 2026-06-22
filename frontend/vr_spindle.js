let currentVrMaterial = 'iron';
let currentVrEra = 'ancient_yuan';
let currentVrRpm = 500;
let currentCriticalRpm = 0;
let forceFeedbackStrength = 0;

function renderVrSpindlePanel(container, materials, eras, baseRotor) {
    container.innerHTML = '';
    const panel = document.createElement('div');
    panel.className = 'vr-panel';
    panel.innerHTML = `
        <div class="panel-header">
            <h3>虚拟体验</h3>
            <div class="panel-controls">
                <div class="control-group">
                    <label>材料</label>
                    <select id="vr-material-select">
                        ${materials.map(m => `<option value="${m.id}" ${currentVrMaterial === m.id ? 'selected' : ''}>${m.name}</option>`).join('')}
                    </select>
                </div>
                <div class="control-group">
                    <label>时代工艺</label>
                    <select id="vr-era-select">
                        ${eras.map(e => `<option value="${e.id}" ${currentVrEra === e.id ? 'selected' : ''}>${e.name}</option>`).join('')}
                    </select>
                </div>
            </div>
        </div>
        <div class="vr-controls">
            <div class="vr-slider-section">
                <div class="slider-label-row">
                    <label>转速 (RPM)</label>
                    <span id="vr-rpm-display" class="rpm-display">${currentVrRpm}</span>
                </div>
                <div class="slider-wrapper">
                    <input type="range" id="vr-rpm-slider" min="100" max="25000" step="50" value="${currentVrRpm}">
                    <div id="ff-critical-marker" class="critical-marker"></div>
                </div>
                <div class="rpm-presets">
                    <button class="rpm-preset" data-rpm="500">500</button>
                    <button class="rpm-preset" data-rpm="2000">2000</button>
                    <button class="rpm-preset" data-rpm="8000">8000</button>
                    <button class="rpm-preset" data-rpm="15000">15000</button>
                    <button class="rpm-preset" data-rpm="22000">22000</button>
                </div>
            </div>
            <div class="force-feedback-section">
                <div class="ff-header">
                    <span>力反馈强度</span>
                    <span id="ff-value">0%</span>
                </div>
                <div class="ff-bar-container">
                    <div id="ff-bar" class="ff-bar"></div>
                </div>
                <div id="ff-hint" class="ff-hint">轻调转速，感受临界转速区的"阻力"</div>
            </div>
            <div class="critical-info">
                <div class="info-item">
                    <span class="info-label">当前临界转速</span>
                    <span id="vr-critical-rpm" class="info-value">-- RPM</span>
                </div>
                <div class="info-item">
                    <span class="info-label">转速/临界比</span>
                    <span id="vr-ratio" class="info-value">--</span>
                </div>
                <button id="vr-sim-btn" class="action-btn">运行VR模拟</button>
            </div>
        </div>
        <div id="vr-sim-result" class="vr-result-area"></div>
    `;
    container.appendChild(panel);

    const rpmSlider = panel.querySelector('#vr-rpm-slider');
    const rpmDisplay = panel.querySelector('#vr-rpm-display');
    if (rpmSlider && rpmDisplay) {
        rpmSlider.addEventListener('input', (e) => {
            const val = parseFloat(e.target.value);
            updateVrRpm(val, materials, eras, baseRotor);
            rpmDisplay.textContent = currentVrRpm;
        });
    }

    panel.querySelectorAll('.rpm-preset').forEach(btn => {
        btn.addEventListener('click', () => {
            const rpm = parseFloat(btn.dataset.rpm);
            if (rpmSlider) {
                rpmSlider.value = rpm;
                rpmSlider.dispatchEvent(new Event('input'));
            }
        });
    });

    const materialSelect = panel.querySelector('#vr-material-select');
    if (materialSelect) {
        materialSelect.addEventListener('change', (e) => {
            updateVrMaterial(e.target.value, materials, eras, baseRotor);
        });
    }

    const eraSelect = panel.querySelector('#vr-era-select');
    if (eraSelect) {
        eraSelect.addEventListener('change', (e) => {
            updateVrEra(e.target.value, materials, eras, baseRotor);
        });
    }

    const simBtn = panel.querySelector('#vr-sim-btn');
    if (simBtn) {
        simBtn.addEventListener('click', runVrSimulation);
    }

    const initialCr = computeCriticalRpm(currentVrMaterial, currentVrEra, materials, eras, baseRotor);
    currentCriticalRpm = initialCr;
    updateForceFeedback(currentVrRpm, initialCr);
    const crEl = document.getElementById('vr-critical-rpm');
    if (crEl) crEl.textContent = initialCr.toFixed(0) + ' RPM';
    const ratioEl = document.getElementById('vr-ratio');
    if (ratioEl) ratioEl.textContent = (currentVrRpm / Math.max(1, initialCr)).toFixed(3);
}

function updateVrRpm(val, materials, eras, baseRotor) {
    currentVrRpm = val;
    if (typeof setCurrentRpm === 'function') setCurrentRpm(val);
    if (typeof window !== 'undefined') {
        window.demoOverrideRpm = val;
    }
    updateForceFeedback(val, null);
    const ratioEl = document.getElementById('vr-ratio');
    if (ratioEl && currentCriticalRpm > 0) {
        ratioEl.textContent = (val / currentCriticalRpm).toFixed(3);
    }
}

function updateVrMaterial(id, materials, eras, baseRotor) {
    currentVrMaterial = id;
    if (typeof setSpindleMaterial === 'function') {
        setSpindleMaterial(currentVrMaterial);
    }
    if (typeof window !== 'undefined') {
        window.currentMaterial = currentVrMaterial;
    }
    if (typeof renderMaterialInfo === 'function') {
        renderMaterialInfo(currentVrMaterial);
    }
    const cr = computeCriticalRpm(currentVrMaterial, currentVrEra, materials, eras, baseRotor);
    currentCriticalRpm = cr;
    updateForceFeedback(currentVrRpm, cr);
    const crEl = document.getElementById('vr-critical-rpm');
    if (crEl) crEl.textContent = cr.toFixed(0) + ' RPM';
    const ratioEl = document.getElementById('vr-ratio');
    if (ratioEl) ratioEl.textContent = (currentVrRpm / Math.max(1, cr)).toFixed(3);
}

function updateVrEra(id, materials, eras, baseRotor) {
    currentVrEra = id;
    if (typeof setSpindleEra === 'function') {
        setSpindleEra(currentVrEra);
    }
    if (typeof window !== 'undefined') {
        window.currentEra = currentVrEra;
    }
    if (typeof renderEraInfo === 'function') {
        renderEraInfo(currentVrEra);
    }
    const cr = computeCriticalRpm(currentVrMaterial, currentVrEra, materials, eras, baseRotor);
    currentCriticalRpm = cr;
    updateForceFeedback(currentVrRpm, cr);
    const crEl = document.getElementById('vr-critical-rpm');
    if (crEl) crEl.textContent = cr.toFixed(0) + ' RPM';
    const ratioEl = document.getElementById('vr-ratio');
    if (ratioEl) ratioEl.textContent = (currentVrRpm / Math.max(1, cr)).toFixed(3);
}

function computeCriticalRpm(material, era, materials, eras, baseRotor) {
    const base = baseRotor || { length: 0.3, diameter: 0.008, E: 210e9, rho: 7850 };
    const baseLength = base.length;
    const baseDiameter = base.diameter;
    const baseE = base.E;
    const baseRho = base.rho;

    const matMap = {};
    if (materials && materials.length) {
        for (const m of materials) {
            if (m.E !== undefined && m.rho !== undefined) {
                matMap[m.id] = { E: m.E, rho: m.rho };
            }
        }
    }
    matMap.iron = matMap.iron || { E: 208e9, rho: 7850 };
    matMap.copper = matMap.copper || { E: 113e9, rho: 8800 };
    matMap.wood = matMap.wood || { E: 12e9, rho: 780 };

    const eraMap = {};
    if (eras && eras.length) {
        for (const e of eras) {
            if (e.length_scale !== undefined || e.diameter_scale !== undefined) {
                eraMap[e.id] = {
                    len: e.length_scale || 1.0,
                    dia: e.diameter_scale || 1.0
                };
            }
        }
    }
    eraMap.ancient_yuan = eraMap.ancient_yuan || { len: 1.2, dia: 1.5 };
    eraMap.modern_high_speed = eraMap.modern_high_speed || { len: 0.8, dia: 0.7 };
    eraMap[''] = eraMap[''] || { len: 1.0, dia: 1.0 };

    const mat = matMap[material] || matMap.iron;
    const e = eraMap[era] || eraMap[''];

    const L = baseLength * e.len;
    const d = baseDiameter * e.dia;
    const I = Math.PI * Math.pow(d, 4) / 64;
    const k_shaft = 48 * mat.E * I / Math.pow(L, 3);
    const volume = Math.PI * Math.pow(d / 2, 2) * L;
    const mass = mat.rho * volume;

    const omega_cr = Math.sqrt(k_shaft / mass);
    const rpm_cr = omega_cr * 60 / (2 * Math.PI);
    return rpm_cr;
}

function updateForceFeedback(rpm, criticalRpm) {
    const rpmSlider = document.getElementById('vr-rpm-slider') || document.getElementById('rpm-slider');
    const ffBar = document.getElementById('ff-bar');
    const ffValue = document.getElementById('ff-value');
    const ffHint = document.getElementById('ff-hint');
    const ffMarker = document.getElementById('ff-critical-marker');

    if (!rpmSlider || !ffBar || !ffValue) return;

    const minRpm = parseFloat(rpmSlider.min);
    const maxRpm = parseFloat(rpmSlider.max);

    if (criticalRpm && criticalRpm > 0) {
        currentCriticalRpm = criticalRpm;
    }

    if (ffMarker && currentCriticalRpm > 0) {
        const markerPct = ((currentCriticalRpm - minRpm) / (maxRpm - minRpm)) * 100;
        ffMarker.style.left = Math.max(0, Math.min(100, markerPct)) + '%';
    }

    const ratio = rpm / Math.max(1, currentCriticalRpm);
    let strength = 0;

    if (ratio > 0.6 && ratio < 1.4) {
        const dist = Math.abs(ratio - 1.0);
        strength = (1.0 - dist / 0.4) * 100;
        strength = Math.max(0, Math.min(100, strength));
    }

    forceFeedbackStrength = strength;
    ffBar.style.width = strength.toFixed(1) + '%';
    ffValue.textContent = strength.toFixed(0) + '%';

    if (strength > 70) {
        rpmSlider.classList.add('near-critical');
        if (ffHint) ffHint.textContent = '⚠ 接近临界转速！感受到明显振动阻力';
    } else if (strength > 30) {
        rpmSlider.classList.remove('near-critical');
        if (ffHint) ffHint.textContent = '振动逐渐增强，注意操作力度';
    } else {
        rpmSlider.classList.remove('near-critical');
        if (ffHint) ffHint.textContent = '轻调转速，感受临界转速区的"阻力"';
    }
}

async function runVrSimulation() {
    const resultEl = document.getElementById('vr-sim-result');
    if (resultEl) {
        resultEl.innerHTML = '<div class="compare-placeholder"><div class="placeholder-icon">⏳</div><div class="placeholder-text">VR模拟运行中...</div></div>';
    }

    const vibration = 0.05 + (currentVrRpm / 10000) * 0.3 + Math.random() * 0.05;
    const temperature = 25 + (currentVrRpm / 10000) * 50 + Math.random() * 2;
    const twist = 800 + Math.sin(Date.now() / 2000) * 30 + Math.random() * 10;
    const ratio = currentVrRpm / Math.max(1, currentCriticalRpm);
    const nearCritical = ratio > 0.85 && ratio < 1.15;

    const summary = {
        rpm: currentVrRpm,
        critical_rpm: currentCriticalRpm,
        ratio: ratio,
        vibration_amplitude: vibration,
        temperature: temperature,
        twist_per_meter: twist,
        force_feedback: forceFeedbackStrength,
        near_critical: nearCritical,
        material: currentVrMaterial,
        era: currentVrEra,
        timestamp: new Date().toISOString()
    };

    try {
        const resp = await fetch(`${window.API_BASE}/api/simulate`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                spindle_id: window.currentSpindle || 'SPD-001',
                rpm: currentVrRpm,
                vibration_amplitude: vibration,
                temperature: temperature,
                twist_per_meter: twist,
                material_id: currentVrMaterial,
                era_id: currentVrEra || undefined,
                balance_correction_fraction: window.balanceCorrectionFraction || undefined,
            }),
        });
        const sim = await resp.json();
        if (typeof updateSimulationDisplay === 'function') {
            updateSimulationDisplay(sim);
        }
        if (typeof updateSensorDisplay === 'function') {
            updateSensorDisplay({
                rpm: currentVrRpm,
                vibration_amplitude: vibration,
                temperature: temperature,
                twist_per_meter: twist,
            });
        }
        if (sim.alerts && sim.alerts.length && typeof addAlerts === 'function') {
            addAlerts(sim.alerts);
        }
        if (sim.vibration && sim.vibration.critical_rpm) {
            currentCriticalRpm = sim.vibration.critical_rpm;
        }
    } catch (e) {
        if (typeof computeLocalSimulation === 'function') {
            const sim = computeLocalSimulation(currentVrRpm, vibration, temperature, twist);
            if (typeof updateSimulationDisplay === 'function') {
                updateSimulationDisplay(sim);
            }
            if (typeof updateSensorDisplay === 'function') {
                updateSensorDisplay({
                    rpm: currentVrRpm,
                    vibration_amplitude: vibration,
                    temperature: temperature,
                    twist_per_meter: twist,
                });
            }
            if (sim.alerts && sim.alerts.length && typeof addAlerts === 'function') {
                addAlerts(sim.alerts);
            }
            if (sim.vibration && sim.vibration.critical_rpm) {
                currentCriticalRpm = sim.vibration.critical_rpm;
            }
        }
    }

    if (resultEl) {
        const ratioClass = nearCritical ? 'warn-value' : (ratio > 1.15 ? 'bad-value' : 'good-value');
        resultEl.innerHTML = `
            <div style="padding:16px;background:var(--bg-secondary);border-radius:12px;">
                <div style="color:var(--accent-cyan);font-weight:700;font-size:14px;margin-bottom:12px;">🎮 VR体验摘要</div>
                <div class="result-row"><span>材料:</span><span class="result-value">${summary.material === 'iron' ? '钢铁锭' : summary.material === 'copper' ? '青铜锭' : '铁木锭'}</span></div>
                <div class="result-row"><span>时代工艺:</span><span class="result-value">${summary.era === 'ancient_yuan' ? '元代水转大纺车' : summary.era === 'modern_high_speed' ? '现代环锭细纱机' : '基准模型'}</span></div>
                <div class="result-row"><span>当前转速:</span><span class="result-value">${summary.rpm.toFixed(0)} RPM</span></div>
                <div class="result-row"><span>临界转速:</span><span class="result-value">${currentCriticalRpm.toFixed(0)} RPM</span></div>
                <div class="result-row"><span>转速/临界比:</span><span class="result-value ${ratioClass}">${summary.ratio.toFixed(3)}</span></div>
                <div class="result-row"><span>力反馈强度:</span><span class="result-value ${forceFeedbackStrength > 70 ? 'warn' : 'good'}">${forceFeedbackStrength.toFixed(0)}%</span></div>
                <div class="result-row"><span>状态:</span><span class="result-value ${nearCritical ? 'warn' : 'good'}">${nearCritical ? '⚠ 接近临界转速' : '✓ 运行稳定'}</span></div>
                <div style="margin-top:12px;padding:10px;background:var(--bg-primary);border-radius:8px;font-size:12px;color:var(--text-secondary);line-height:1.6;">
                    💡 拖动上方转速滑块，当转速接近临界转速时，滑块会产生"阻力"效果（力反馈强度升高），直观体验共振区的物理现象。
                </div>
            </div>
        `;
    }
}
