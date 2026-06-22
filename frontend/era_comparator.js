let currentEraMaterialId = null;
let currentEraOverrideRpm = null;

function renderEraComparatorPanel(container, materials, eras) {
    container.innerHTML = '';
    const panel = document.createElement('div');
    panel.className = 'comparator-panel';
    panel.innerHTML = `
        <div class="panel-header">
            <h3>跨时代对比</h3>
            <div class="panel-controls">
                <div class="control-group">
                    <label>材料</label>
                    <select id="era-material-select">
                        <option value="">全部材料</option>
                        ${materials.map(m => `<option value="${m.id}" ${currentEraMaterialId === m.id ? 'selected' : ''}>${m.name}</option>`).join('')}
                    </select>
                </div>
                <div class="control-group">
                    <label>强制转速 (RPM)</label>
                    <input type="range" id="era-rpm-slider" min="0" max="25000" step="100" value="${currentEraOverrideRpm || 0}">
                    <span id="era-rpm-display">${currentEraOverrideRpm || '各时代典型值'}</span>
                </div>
                <button id="era-compare-btn" class="action-btn">开始对比</button>
            </div>
        </div>
        <div class="charts-area">
            <div class="chart-container">
                <canvas id="era-chart-rpm"></canvas>
            </div>
            <div class="chart-container">
                <canvas id="era-chart-disp"></canvas>
            </div>
            <div class="chart-container">
                <canvas id="era-chart-output"></canvas>
            </div>
        </div>
        <div id="era-results-table" class="results-area"></div>
    `;
    container.appendChild(panel);

    const materialSelect = panel.querySelector('#era-material-select');
    materialSelect.addEventListener('change', (e) => {
        updateEraMaterial(e.target.value || null);
    });

    const rpmSlider = panel.querySelector('#era-rpm-slider');
    const rpmDisplay = panel.querySelector('#era-rpm-display');
    rpmSlider.addEventListener('input', (e) => {
        const val = parseFloat(e.target.value);
        updateEraRpm(val === 0 ? null : val);
        rpmDisplay.textContent = currentEraOverrideRpm || '各时代典型值';
    });

    const compareBtn = panel.querySelector('#era-compare-btn');
    compareBtn.addEventListener('click', runEraComparison);
}

function updateEraMaterial(id) {
    currentEraMaterialId = id;
}

function updateEraRpm(val) {
    currentEraOverrideRpm = val;
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

function renderEraComparisonResults(data) {
    const resultsArea = document.getElementById('era-results-table');
    if (!resultsArea) return;
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

    resultsArea.innerHTML = `
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

    const labels = (data.comparisons || []).map(c => c.display_name.split(' ')[1] || c.display_name);
    const typicalRpmData = (data.comparisons || []).map(c => c.typical_rpm);
    const criticalRpmData = (data.comparisons || []).map(c => c.critical_rpm);
    const dispData = (data.comparisons || []).map(c => c.total_displacement_mm);
    const outputData = (data.comparisons || []).map(c => c.daily_output_kg);
    const uniformityData = (data.comparisons || []).map(c => c.estimated_uniformity);
    const strengthData = (data.comparisons || []).map(c => c.estimated_strength);

    const chartColors = ['#8b5cf6', '#f59e0b', '#10b981', '#3b82f6', '#ef4444'];

    const ctxRpm = document.getElementById('era-chart-rpm');
    if (ctxRpm && typeof Chart !== 'undefined') {
        if (window._eraChartRpm) window._eraChartRpm.destroy();
        window._eraChartRpm = new Chart(ctxRpm, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [
                    {
                        label: '典型转速 (RPM)',
                        data: typicalRpmData,
                        backgroundColor: '#3b82f6',
                        borderColor: '#3b82f6',
                        borderWidth: 1
                    },
                    {
                        label: '临界转速 (RPM)',
                        data: criticalRpmData,
                        backgroundColor: '#ef4444',
                        borderColor: '#ef4444',
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '转速对比 (典型 vs 临界)' } },
                scales: { y: { beginAtZero: true } }
            }
        });
    }

    const ctxDisp = document.getElementById('era-chart-disp');
    if (ctxDisp && typeof Chart !== 'undefined') {
        if (window._eraChartDisp) window._eraChartDisp.destroy();
        window._eraChartDisp = new Chart(ctxDisp, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [
                    {
                        label: '振动位移 (mm)',
                        data: dispData,
                        backgroundColor: chartColors.slice(0, labels.length),
                        borderColor: chartColors.slice(0, labels.length),
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '振动位移对比' } },
                scales: { y: { beginAtZero: true } }
            }
        });
    }

    const ctxOutput = document.getElementById('era-chart-output');
    if (ctxOutput && typeof Chart !== 'undefined') {
        if (window._eraChartOutput) window._eraChartOutput.destroy();
        window._eraChartOutput = new Chart(ctxOutput, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [
                    {
                        label: '日产量 (kg)',
                        data: outputData,
                        backgroundColor: '#f59e0b',
                        borderColor: '#f59e0b',
                        yAxisID: 'y'
                    },
                    {
                        label: '均匀度 (%)',
                        data: uniformityData,
                        backgroundColor: '#10b981',
                        borderColor: '#10b981',
                        yAxisID: 'y1'
                    },
                    {
                        label: '强度 (cN/tex)',
                        data: strengthData,
                        backgroundColor: '#8b5cf6',
                        borderColor: '#8b5cf6',
                        yAxisID: 'y1'
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { title: { display: true, text: '产量与质量对比' } },
                scales: {
                    y: { beginAtZero: true, position: 'left' },
                    y1: { beginAtZero: true, position: 'right', grid: { drawOnChartArea: false } }
                }
            }
        });
    }
}

async function runEraComparison() {
    const contentEl = document.getElementById('era-results-table');
    if (contentEl) {
        contentEl.innerHTML = '<div class="compare-placeholder"><div class="placeholder-icon">⏳</div><div class="placeholder-text">计算中...</div></div>';
    }

    try {
        const resp = await fetch(`${window.API_BASE}/api/era-comparison`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                material_id: currentEraMaterialId || window.currentMaterial || undefined,
                override_rpm: currentEraOverrideRpm || undefined,
            }),
        });
        const data = await resp.json();
        renderEraComparisonResults(data);
    } catch (e) {
        const fallback = { comparisons: localEraCompare() };
        renderEraComparisonResults(fallback);
    }
}
