const LOD = { HIGH: 0, MEDIUM: 1, LOW: 2 };
const LOD_NAMES = ['HIGH', 'MEDIUM', 'LOW'];

const MATERIAL_PRESETS = {
    wood: {
        shaftColor: 0x6b4423,
        shaftMetalness: 0.1,
        shaftRoughness: 0.75,
        whorlColor: 0x4a3728,
        whorlMetalness: 0.05,
        whorlRoughness: 0.85,
        baseColor: 0x3a2a1c,
        pillarColor: 0x8b6f47,
        pillarMetalness: 0.05,
    },
    copper: {
        shaftColor: 0xb87333,
        shaftMetalness: 0.85,
        shaftRoughness: 0.35,
        whorlColor: 0xa0622d,
        whorlMetalness: 0.7,
        whorlRoughness: 0.4,
        baseColor: 0x4a3728,
        pillarColor: 0x9c7b4f,
        pillarMetalness: 0.6,
    },
    iron: {
        shaftColor: 0x8899aa,
        shaftMetalness: 0.9,
        shaftRoughness: 0.25,
        whorlColor: 0x7a8a9a,
        whorlMetalness: 0.8,
        whorlRoughness: 0.3,
        baseColor: 0x3a3a44,
        pillarColor: 0x8a99aa,
        pillarMetalness: 0.85,
    }
};

const ERA_PRESETS = {
    ancient_yuan: {
        shaftLenScale: 1.2,
        shaftDiaScale: 1.5,
        whorlScale: 1.3,
        baseScale: 1.15,
        label: '元代水转大纺车',
        labelColor: 0xf59e0b,
    },
    modern_high_speed: {
        shaftLenScale: 0.85,
        shaftDiaScale: 0.75,
        whorlScale: 0.8,
        baseScale: 0.9,
        label: '现代环锭细纱机',
        labelColor: 0x06b6d4,
    }
};

const spindleGroup = new THREE.Group();
let spindleMesh = null;
let bearingMesh = null;
let yarnGroup = null;
let yarnLodObjects = [];
let currentLod = LOD.HIGH;
let motionBlurGroup = null;
let whirlGlow = null;
let whorlMesh = null;
let baseMesh = null;
let pillarMesh = null;
let topBearingMesh = null;
let topCapMesh = null;
let eraLabel = null;
let scene, camera, renderer, controls;
let vibX = 0, vibY = 0;
let currentRpm = 0;
let lastFrameTime = performance.now();
let frameTimeHistory = [];
let yarnBuildParams = null;
let simulationData = null;
let currentMaterialId = 'iron';
let currentEraId = null;

let updateDisplayCallback = null;
let addAlertsCallback = null;

function setCallbacks(onUpdateDisplay, onAddAlerts) {
    updateDisplayCallback = onUpdateDisplay;
    addAlertsCallback = onAddAlerts;
}

function initThreeScene() {
    const container = document.getElementById('three-container');
    const w = container.clientWidth;
    const h = container.clientHeight;

    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0e17);
    scene.fog = new THREE.FogExp2(0x0a0e17, 0.02);

    camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 1000);
    camera.position.set(3, 4, 6);
    camera.lookAt(0, 2, 0);

    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    container.appendChild(renderer.domElement);

    controls = new THREE.OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.target.set(0, 2, 0);
    controls.update();

    const ambientLight = new THREE.AmbientLight(0x404060, 0.6);
    scene.add(ambientLight);

    const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
    dirLight.position.set(5, 10, 5);
    dirLight.castShadow = true;
    scene.add(dirLight);

    const pointLight1 = new THREE.PointLight(0x3b82f6, 0.5, 20);
    pointLight1.position.set(-3, 5, -3);
    scene.add(pointLight1);

    const pointLight2 = new THREE.PointLight(0x06b6d4, 0.4, 20);
    pointLight2.position.set(3, 3, 3);
    scene.add(pointLight2);

    const floorGeo = new THREE.PlaneGeometry(20, 20);
    const floorMat = new THREE.MeshStandardMaterial({ color: 0x111827, roughness: 0.9 });
    const floor = new THREE.Mesh(floorGeo, floorMat);
    floor.rotation.x = -Math.PI / 2;
    floor.receiveShadow = true;
    scene.add(floor);

    const gridHelper = new THREE.GridHelper(20, 40, 0x1a2332, 0x1a2332);
    scene.add(gridHelper);

    buildSpindleModel();
    scene.add(spindleGroup);

    window.addEventListener('resize', () => {
        const w2 = container.clientWidth;
        const h2 = container.clientHeight;
        camera.aspect = w2 / h2;
        camera.updateProjectionMatrix();
        renderer.setSize(w2, h2);
    });
}

function buildSpindleModel(materialId = 'iron', eraId = null) {
    while (spindleGroup.children.length) {
        spindleGroup.remove(spindleGroup.children[0]);
    }
    currentMaterialId = materialId;
    currentEraId = eraId;

    const mat = MATERIAL_PRESETS[materialId] || MATERIAL_PRESETS.iron;
    const era = eraId ? (ERA_PRESETS[eraId] || null) : null;

    const shaftLenScale = era ? era.shaftLenScale : 1.0;
    const shaftDiaScale = era ? era.shaftDiaScale : 1.0;
    const whorlScale = era ? era.whorlScale : 1.0;
    const baseScale = era ? era.baseScale : 1.0;

    const baseMat = new THREE.MeshStandardMaterial({ color: mat.baseColor, roughness: 0.7, metalness: 0.3 });
    const metalMat = new THREE.MeshStandardMaterial({
        color: mat.shaftColor,
        roughness: mat.shaftRoughness,
        metalness: mat.shaftMetalness
    });
    const pillarMat = new THREE.MeshStandardMaterial({
        color: mat.pillarColor,
        roughness: 0.4,
        metalness: mat.pillarMetalness
    });
    const bearingMat = new THREE.MeshStandardMaterial({ color: 0xb8860b, roughness: 0.4, metalness: 0.6 });
    const whorlMat = new THREE.MeshStandardMaterial({
        color: mat.whorlColor,
        roughness: mat.whorlRoughness,
        metalness: mat.whorlMetalness
    });

    const baseGeo = new THREE.CylinderGeometry(0.6 * baseScale, 0.7 * baseScale, 0.15 * baseScale, 32);
    baseMesh = new THREE.Mesh(baseGeo, baseMat);
    baseMesh.position.y = 0.075 * baseScale;
    baseMesh.castShadow = true;
    spindleGroup.add(baseMesh);

    const pillarH = 0.4 * baseScale;
    const basePillarGeo = new THREE.CylinderGeometry(0.08 * baseScale, 0.1 * baseScale, pillarH, 16);
    pillarMesh = new THREE.Mesh(basePillarGeo, pillarMat);
    pillarMesh.position.y = 0.075 * baseScale + pillarH / 2;
    pillarMesh.castShadow = true;
    spindleGroup.add(pillarMesh);

    const bearingY = 0.075 * baseScale + pillarH + 0.12 * baseScale;
    const bearingGeo = new THREE.TorusGeometry(0.12 * baseScale, 0.04, 16, 32);
    bearingMesh = new THREE.Mesh(bearingGeo, bearingMat);
    bearingMesh.position.y = bearingY;
    bearingMesh.rotation.x = Math.PI / 2;
    bearingMesh.castShadow = true;
    spindleGroup.add(bearingMesh);

    const shaftLen = 3.0 * shaftLenScale;
    const shaftTopR = 0.02 * shaftDiaScale;
    const shaftBotR = 0.025 * shaftDiaScale;
    const shaftGeo = new THREE.CylinderGeometry(shaftTopR, shaftBotR, shaftLen, 16);
    const shaftY = bearingY + shaftLen / 2;
    spindleMesh = new THREE.Mesh(shaftGeo, metalMat);
    spindleMesh.position.y = shaftY;
    spindleMesh.castShadow = true;
    spindleGroup.add(spindleMesh);

    const whorlR1 = 0.25 * whorlScale;
    const whorlR2 = 0.15 * whorlScale;
    const whorlH = 0.12 * whorlScale;
    const whorlGeo = new THREE.CylinderGeometry(whorlR1, whorlR2, whorlH, 32);
    const whorlY = bearingY + shaftLen * 0.3;
    whorlMesh = new THREE.Mesh(whorlGeo, whorlMat);
    whorlMesh.position.y = whorlY;
    whorlMesh.castShadow = true;
    spindleGroup.add(whorlMesh);

    const topBearingY = bearingY + shaftLen + 0.05 * shaftLenScale;
    const topBearingGeo = new THREE.TorusGeometry(0.06 * shaftDiaScale, 0.02, 12, 24);
    topBearingMesh = new THREE.Mesh(topBearingGeo, bearingMat);
    topBearingMesh.position.y = topBearingY;
    topBearingMesh.rotation.x = Math.PI / 2;
    spindleGroup.add(topBearingMesh);

    const topCapGeo = new THREE.ConeGeometry(0.04 * shaftDiaScale, 0.1 * shaftLenScale, 16);
    topCapMesh = new THREE.Mesh(topCapGeo, metalMat);
    topCapMesh.position.y = topBearingY + 0.05 * shaftLenScale;
    spindleGroup.add(topCapMesh);

    const glowGeo = new THREE.RingGeometry(0.08 * shaftDiaScale, 0.12 * shaftDiaScale, 32);
    const glowMat = new THREE.MeshBasicMaterial({
        color: 0xef4444,
        transparent: true,
        opacity: 0.0,
        side: THREE.DoubleSide,
    });
    whirlGlow = new THREE.Mesh(glowGeo, glowMat);
    whirlGlow.position.y = shaftY;
    whirlGlow.rotation.x = Math.PI / 2;
    spindleGroup.add(whirlGlow);

    if (era) {
        const canvas = document.createElement('canvas');
        canvas.width = 512;
        canvas.height = 128;
        const ctx = canvas.getContext('2d');
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.clearRect(0, 0, 512, 128);
        const col = '#' + era.labelColor.toString(16).padStart(6, '0');
        ctx.font = 'bold 48px "Noto Serif SC", serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.strokeStyle = 'rgba(0,0,0,0.8)';
        ctx.lineWidth = 6;
        ctx.strokeText(era.label, 256, 64);
        ctx.fillStyle = col;
        ctx.fillText(era.label, 256, 64);
        const tex = new THREE.CanvasTexture(canvas);
        tex.anisotropy = 8;
        const spriteMat = new THREE.SpriteMaterial({ map: tex, transparent: true, depthTest: false });
        eraLabel = new THREE.Sprite(spriteMat);
        eraLabel.position.set(0, topBearingY + 0.5 * shaftLenScale, 0);
        eraLabel.scale.set(2.5, 0.625, 1);
        spindleGroup.add(eraLabel);
    }

    buildYarnOnSpindle();
}

function setSpindleMaterial(materialId) {
    buildSpindleModel(materialId, currentEraId);
}

function setSpindleEra(eraId) {
    buildSpindleModel(currentMaterialId, eraId);
}

function setCurrentRpm(rpm) {
    currentRpm = rpm;
}

function buildYarnGeometry(params, lodLevel) {
    const yarnMat = new THREE.MeshStandardMaterial({
        color: 0xf5f0e0,
        roughness: 0.8,
        metalness: 0.0,
        emissive: 0x222211,
        emissiveIntensity: 0.1,
    });

    const { helixRadius, helixHeight, helixTurns } = params;

    if (lodLevel === LOD.HIGH) {
        const helixPoints = helixTurns * 16;
        const points = [];
        for (let i = 0; i <= helixPoints; i++) {
            const t = i / helixPoints;
            const angle = t * helixTurns * Math.PI * 2;
            const y = 1.1 + t * helixHeight;
            const r = helixRadius + 0.005 * Math.sin(t * 50);
            points.push(new THREE.Vector3(r * Math.cos(angle), y, r * Math.sin(angle)));
        }
        const curve = new THREE.CatmullRomCurve3(points);
        const tubeGeo = new THREE.TubeGeometry(curve, helixPoints * 2, 0.004, 6, false);
        return new THREE.Mesh(tubeGeo, yarnMat);
    } else if (lodLevel === LOD.MEDIUM) {
        const helixPoints = helixTurns * 4;
        const points = [];
        for (let i = 0; i <= helixPoints; i++) {
            const t = i / helixPoints;
            const angle = t * helixTurns * Math.PI * 2;
            const y = 1.1 + t * helixHeight;
            const r = helixRadius + 0.005 * Math.sin(t * 20);
            points.push(new THREE.Vector3(r * Math.cos(angle), y, r * Math.sin(angle)));
        }
        const lineGeo = new THREE.BufferGeometry().setFromPoints(points);
        const lineMat = new THREE.LineBasicMaterial({
            color: 0xf5f0e0,
            transparent: true,
            opacity: 0.85,
        });
        return new THREE.Line(lineGeo, lineMat);
    } else {
        const shellGeo = new THREE.CylinderGeometry(
            helixRadius + 0.01,
            helixRadius + 0.01,
            helixHeight,
            16, 1, true
        );
        const shellMat = new THREE.MeshStandardMaterial({
            color: 0xf0ead6,
            roughness: 0.95,
            side: THREE.DoubleSide,
            transparent: true,
            opacity: 0.4,
        });
        const shell = new THREE.Mesh(shellGeo, shellMat);
        shell.position.y = 1.1 + helixHeight / 2;
        return shell;
    }
}

function buildMotionBlurLines(params) {
    const group = new THREE.Group();
    const { helixRadius, helixHeight, helixTurns } = params;
    const blurLayers = 4;
    const maxAngleOffset = 0.15;

    for (let layer = 1; layer <= blurLayers; layer++) {
        const opacity = (1 - layer / (blurLayers + 1)) * 0.3;
        const helixPoints = Math.min(helixTurns * 3, 40);
        const points = [];
        const angleOffset = (layer / blurLayers) * maxAngleOffset;

        for (let i = 0; i <= helixPoints; i++) {
            const t = i / helixPoints;
            const angle = t * helixTurns * Math.PI * 2 + angleOffset;
            const y = 1.1 + t * helixHeight;
            const r = helixRadius;
            points.push(new THREE.Vector3(r * Math.cos(angle), y, r * Math.sin(angle)));
        }

        const lineGeo = new THREE.BufferGeometry().setFromPoints(points);
        const lineMat = new THREE.LineBasicMaterial({
            color: 0xf5f0e0,
            transparent: true,
            opacity: opacity,
        });
        const line = new THREE.Line(lineGeo, lineMat);
        group.add(line);

        const points2 = [];
        for (let i = 0; i <= helixPoints; i++) {
            const t = i / helixPoints;
            const angle = t * helixTurns * Math.PI * 2 - angleOffset;
            const y = 1.1 + t * helixHeight;
            const r = helixRadius;
            points2.push(new THREE.Vector3(r * Math.cos(angle), y, r * Math.sin(angle)));
        }
        const lineGeo2 = new THREE.BufferGeometry().setFromPoints(points2);
        const line2 = new THREE.Line(lineGeo2, lineMat.clone());
        group.add(line2);
    }

    return group;
}

function buildYarnOnSpindle() {
    if (yarnGroup) {
        spindleGroup.remove(yarnGroup);
    }
    yarnGroup = new THREE.Group();
    yarnLodObjects = [];
    motionBlurGroup = null;

    const twistPerMeter = simulationData ? simulationData.yarn_quality.twist_variance : 0;
    const helixRadius = 0.04 + twistPerMeter * 0.02;
    const helixHeight = 1.8;
    const helixTurns = 20 + twistPerMeter * 30;

    yarnBuildParams = { helixRadius, helixHeight, helixTurns };

    for (let lod = 0; lod < 3; lod++) {
        const obj = buildYarnGeometry(yarnBuildParams, lod);
        obj.visible = lod === LOD.HIGH;
        yarnLodObjects.push(obj);
        yarnGroup.add(obj);
    }

    const copGeo = new THREE.CylinderGeometry(helixRadius + 0.015, helixRadius + 0.015, helixHeight, 16, 1, true);
    const copMat = new THREE.MeshStandardMaterial({
        color: 0xf0ead6,
        roughness: 0.9,
        side: THREE.DoubleSide,
        transparent: true,
        opacity: 0.25,
    });
    const cop = new THREE.Mesh(copGeo, copMat);
    cop.position.y = 1.1 + helixHeight / 2;
    yarnGroup.add(cop);

    motionBlurGroup = buildMotionBlurLines(yarnBuildParams);
    motionBlurGroup.visible = false;
    yarnGroup.add(motionBlurGroup);

    currentLod = LOD.HIGH;
    updateLodVisibility();
    spindleGroup.add(yarnGroup);
}

function getCameraDistanceToSpindle() {
    const spindleCenter = new THREE.Vector3(0, 2.05, 0);
    return camera.position.distanceTo(spindleCenter);
}

function determineLod() {
    const distance = getCameraDistanceToSpindle();
    const rpm = currentRpm;

    let distanceLod = LOD.HIGH;
    if (distance > 8) distanceLod = LOD.LOW;
    else if (distance > 4) distanceLod = LOD.MEDIUM;

    let rpmLod = LOD.HIGH;
    if (rpm > 3500) rpmLod = LOD.LOW;
    else if (rpm > 2000) rpmLod = LOD.MEDIUM;

    let performanceLod = LOD.HIGH;
    if (frameTimeHistory.length > 10) {
        const avgFrameTime = frameTimeHistory.reduce((a, b) => a + b, 0) / frameTimeHistory.length;
        if (avgFrameTime > 33) performanceLod = LOD.LOW;
        else if (avgFrameTime > 20) performanceLod = LOD.MEDIUM;
    }

    return Math.max(distanceLod, rpmLod, performanceLod);
}

function updateLodVisibility() {
    const newLod = determineLod();

    if (newLod !== currentLod && yarnLodObjects.length === 3) {
        yarnLodObjects[currentLod].visible = false;
        yarnLodObjects[newLod].visible = true;
        currentLod = newLod;

        if (motionBlurGroup) {
            motionBlurGroup.visible = currentLod !== LOD.HIGH && currentRpm > 1500;
        }
    }
}

function updateSpindleVibration(time) {
    if (!spindleMesh || !simulationData) return;

    const vib = simulationData.vibration;
    const freq = currentRpm / 60.0;
    const omega = freq * Math.PI * 2;
    const scale = Math.min(vib.total_displacement * 50, 0.5);

    const whirlMod = vib.whirl_instability ? 1.5 : 1.0;
    const whirlOmega = omega * vib.whirl_ratio * whirlMod;

    vibX = scale * Math.cos(omega * time) + scale * 0.3 * Math.cos(whirlOmega * time);
    vibY = scale * Math.sin(omega * time) + scale * 0.3 * Math.sin(whirlOmega * time);

    spindleMesh.position.x = vibX;
    spindleMesh.position.z = vibY;

    if (yarnGroup) {
        yarnGroup.position.x = vibX * 0.5;
        yarnGroup.position.z = vibY * 0.5;
    }

    if (whirlGlow) {
        const targetOpacity = vib.whirl_instability ? 0.6 : 0.0;
        whirlGlow.material.opacity += (targetOpacity - whirlGlow.material.opacity) * 0.1;
        whirlGlow.rotation.z += omega * 0.016;
    }

    const rotationSpeed = currentRpm / 60.0 * Math.PI * 2 * 0.016;
    spindleMesh.rotation.y += rotationSpeed;
}

function animate() {
    requestAnimationFrame(animate);

    const now = performance.now();
    const frameTime = now - lastFrameTime;
    lastFrameTime = now;

    frameTimeHistory.push(frameTime);
    if (frameTimeHistory.length > 20) frameTimeHistory.shift();

    const time = now / 1000;
    updateSpindleVibration(time);
    updateLodVisibility();
    controls.update();
    renderer.render(scene, camera);
}

function setSimulationData(data) {
    simulationData = data;
    if (data && data.vibration) {
        if (!yarnBuildParams ||
            Math.abs(yarnBuildParams.helixRadius - (0.04 + data.yarn_quality.twist_variance * 0.02)) > 0.001) {
            buildYarnOnSpindle();
        }
    }
}

function setCurrentRpm(rpm) {
    currentRpm = rpm;
}

function getCurrentLodName() {
    return LOD_NAMES[currentLod];
}

function getAverageFps() {
    if (frameTimeHistory.length === 0) return 60;
    const avg = frameTimeHistory.reduce((a, b) => a + b, 0) / frameTimeHistory.length;
    return Math.round(1000 / avg);
}

function getCurrentVibXY() {
    return { vibX, vibY };
}
