let currentMaterialRpm = 500;
let currentMaterialEraId = null;

function renderMaterialComparatorPanel(container, materials, eras) {
    container.innerHTML = '';
    const panel = document.createElement('div');
    panel.className = 'comparator-panel';
    panel.innerHTML = `
        <div class="panel-header">
            <h3>材料对比</h3>
            <div class="panel-controls">
                <div class="control-group">
                    <label>转速 (RPM)</label>
                    <input type="range" id="mat-rpm-slider" min="100" max="25000" step="100" value="${currentMaterialRpm}">
                    <span id="mat-rpm-display">${currentMaterialRpm}</span>
                </div>
                <div class="control-group">
                    <label>时代工艺</label>
                    <select id="mat-era-select">
                        <option value="">基准模型</option>
                        ${eras.map(e => `<option value="${e.id}" ${currentMaterialEraId === e.id ? 'selected' : ''}>${e.name}</option>`).join('')}
                    </select>
                </div>
                <button id="mat-compare-btn" class="action-btn">开始对比</button>
            </div>
        </div>
        <div class="charts-area">
            <div class="chart-container">
                <canvas id="mat-chart-critical"></canvas>
            </div>
            <div class="chart-container">
                <canvas id="mat-chart-displacement"></canvas>
            </div>
            <div class="chart-container">
                <canvas id="mat-chart-quality"></canvas>
            </div>
        </div>
        <div id="mat-results-table" class="results-area"></div>
    `;
    container.appendChild(panel);

    const rpmSlider = panel.querySelector('#mat-rpm-slider');
    const rpmDisplay = panel.querySelector('#mat-rpm-display');
    rpmSlider.addEventListener('input', (e) => {
        updateMaterialRpm(parseFloat(e.target.value));
        rpmDisplay.textContent = currentMaterialRpm;
    });

    const eraSelect = panel.querySelector('#mat-era-select');
    eraSelect.addEventListener('change', (e) => {
        updateMaterialEra(e.target.value || null);
    });

    const compareBtn = panel.querySelector('#mat-compare-btn');
    compareBtn.addEventListener('click', runMaterialComparison);
}

function updateMaterialRpm(val) {
    currentMaterialRpm = val;
}

function updateMaterialEra(id) {
    currentMaterialEraId = id;
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

function renderMaterialComparisonResults(data) {
    const resultsArea = document.getElementById('mat-results-table');
    if (!resultsArea) return;
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

    resultsArea.innerHTML = `
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

    const labels = (data.comparisons || []).map(c => c.display_name);
    const criticalData = (data.comparisons || []).map(c => c.critical_rpm);
    const displacementData = (data.comparisons || []).map(c => c.total_displacement_mm);
    const uniformityData = (data.comparisons || []).map(c => c.estimated_uniformity || 0);
    const strengthData = (data.comparisons || []).map(c => c.estimated_strength || 0);

    const chartColors = ['#3b82f6', '#f59e0b', '#10b981', '#8b5cf6', '#ef4444'];

    const ctxCritical = document.getElementById('mat-chart-critical');
    if (ctxCritical && typeof Chart !== 'undefined') {
        if (window._matChartCritical) window._matChartCritical.destroy();
        window._matChartCritical = new Chart(ctxCritical, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [{
                    label: '临界转速 (RPM)',
                    data: criticalData,
                    backgroundColor: chartColors.slice(0, labels.length),
                    borderColor: chartColors.slice(0, labels.length),
                    borderWidth: 1
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '临界转速对比' } },
                scales: { y: { beginAtZero: true } }
            }
        });
    }

    const ctxDisp = document.getElementById('mat-chart-displacement');
    if (ctxDisp && typeof Chart !== 'undefined') {
        if (window._matChartDisp) window._matChartDisp.destroy();
        window._matChartDisp = new Chart(ctxDisp, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [{
                    label: '振动位移 (mm)',
                    data: displacementData,
                    backgroundColor: chartColors.slice(0, labels.length),
                    borderColor: chartColors.slice(0, labels.length),
                    borderWidth: 1
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '振动位移对比' } },
                scales: { y: { beginAtZero: true } }
            }
        });
    }

    const ctxQuality = document.getElementById('mat-chart-quality');
    if (ctxQuality && typeof Chart !== 'undefined') {
        if (window._matChartQuality) window._matChartQuality.destroy();
        window._matChartQuality = new Chart(ctxQuality, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [
                    {
                        label: '均匀度 (%)',
                        data: uniformityData,
                        backgroundColor: '#3b82f6',
                        borderColor: '#3b82f6',
                        borderWidth: 1
                    },
                    {
                        label: '强度 (cN/tex)',
                        data: strengthData,
                        backgroundColor: '#10b981',
                        borderColor: '#10b981',
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '成纱质量对比' } },
                scales: { y: { beginAtZero: true } }
            }
        });
    }
}

async function runMaterialComparison() {
    const rpm = currentMaterialRpm;
    const contentEl = document.getElementById('mat-results-table');
    if (contentEl) {
        contentEl.innerHTML = '<div class="compare-placeholder"><div class="placeholder-icon">⏳</div><div class="placeholder-text">计算中...</div></div>';
    }

    try {
        const resp = await fetch(`${window.API_BASE}/api/material-comparison`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ rpm, era_id: currentMaterialEraId || undefined }),
        });
        const data = await resp.json();
        renderMaterialComparisonResults(data);
    } catch (e) {
        const fallback = { rpm, era_id: currentMaterialEraId, comparisons: localMaterialCompare(rpm) };
        renderMaterialComparisonResults(fallback);
    }
}
