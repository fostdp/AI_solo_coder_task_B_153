import json
import time
import math
import random
import argparse
from dataclasses import dataclass, field
from typing import Optional
import paho.mqtt.client as mqtt

MQTT_TOPIC = "spindle/sensor_data"

DEFAULT_SPINDLE_COUNT = 8
DEFAULT_BASE_RPM = 1500
DEFAULT_RPM_VARIANCE = 200
DEFAULT_BASE_VIBRATION = 0.15
DEFAULT_BASE_TEMPERATURE = 35.0
DEFAULT_BASE_TWIST = 800


@dataclass
class OilFilmCondition:
    viscosity_pa_s: float = 0.01
    radial_clearance_m: float = 5e-5
    aging_factor: float = 1.0
    contamination: float = 0.0
    temperature_penalty: float = 0.0

    def effective_viscosity(self) -> float:
        contam_visc = 1.0 + 0.8 * self.contamination
        aged_visc = 1.0 - 0.4 * min(self.aging_factor, 2.0)
        temp_factor = 1.0 + self.temperature_penalty
        return self.viscosity_pa_s * max(0.1, aged_visc * contam_visc * temp_factor)

    def clearance_ratio(self) -> float:
        aged_ratio = 1.0 + 0.6 * min(self.aging_factor, 2.0)
        contam_ratio = 1.0 - 0.3 * self.contamination
        return self.radial_clearance_m * aged_ratio * contam_ratio


@dataclass
class SpindleProfile:
    idx: int
    rpm_base: float
    rpm_variance: float
    rpm_sweep_min: float
    rpm_sweep_max: float
    rpm_sweep_minutes: float
    vibration_offset: float
    temperature_offset: float
    twist_offset: float
    wear_severity: float = 0.0
    oil: OilFilmCondition = field(default_factory=OilFilmCondition)
    mode: str = "random"

    def current_rpm(self, tick: int, interval_sec: int) -> float:
        t_min = tick * interval_sec / 60.0
        period = max(self.rpm_sweep_minutes, 0.1)
        if self.mode == "constant":
            rpm = self.rpm_base + random.gauss(0, self.rpm_variance * 0.05)
        elif self.mode == "sweep":
            span = self.rpm_sweep_max - self.rpm_sweep_min
            phase = 2 * math.pi * (t_min % period) / period
            rpm = self.rpm_sweep_min + span * (0.5 + 0.5 * math.sin(phase - math.pi / 2))
            rpm += random.gauss(0, self.rpm_variance * 0.05)
        elif self.mode == "step":
            steps = [(self.rpm_sweep_min + (self.rpm_sweep_max - self.rpm_sweep_min) * i / 4.0) for i in range(5)]
            idx = int((t_min // max(period / 5.0, 0.1)) % 5)
            rpm = steps[int(idx)]
            rpm += random.gauss(0, self.rpm_variance * 0.03)
        else:
            phase = tick * 0.05 + self.idx
            rpm = self.rpm_base + self.rpm_variance * math.sin(phase) + random.gauss(0, 30)
        return max(100.0, min(10000.0, rpm))


def build_profile(
    idx: int,
    args: argparse.Namespace,
    oil_defaults: OilFilmCondition,
) -> SpindleProfile:
    variance = args.rpm_variance
    sweep_min = args.rpm_sweep_min
    sweep_max = args.rpm_sweep_max
    sweep_minutes = args.rpm_sweep_minutes
    mode = args.mode

    wear = 0.0
    if args.inject_worn_idx and (idx in args.inject_worn_idx):
        wear = 0.3

    oil = OilFilmCondition(
        viscosity_pa_s=args.oil_viscosity or oil_defaults.viscosity_pa_s,
        radial_clearance_m=args.oil_clearance or oil_defaults.radial_clearance_m,
        aging_factor=args.oil_aging,
        contamination=args.oil_contamination,
        temperature_penalty=0.0,
    )

    return SpindleProfile(
        idx=idx,
        rpm_base=args.rpm_base + idx * args.rpm_spread_per_spindle,
        rpm_variance=variance,
        rpm_sweep_min=sweep_min,
        rpm_sweep_max=sweep_max,
        rpm_sweep_minutes=sweep_minutes,
        vibration_offset=0.0 + wear * 0.6,
        temperature_offset=0.0 + wear * 10.0,
        twist_offset=0.0,
        wear_severity=wear,
        oil=oil,
        mode=mode,
    )


def jeffcott_unbalance_response(
    rpm: float,
    k_shaft: float,
    m_kg: float,
    eccentricity_m: float,
    damping_ratio: float,
) -> float:
    omega_cr = math.sqrt(k_shaft / m_kg)
    omega = rpm * 2 * math.pi / 60.0
    r = omega / omega_cr
    denom = math.sqrt((1.0 - r * r) ** 2 + (2 * damping_ratio * r) ** 2)
    if denom < 1e-12:
        return 0.0
    return eccentricity_m * (r * r) / denom


def short_bearing_eccentricity_ratio(
    rpm: float,
    viscosity_pa_s: float,
    l_bear_m: float,
    d_bear_m: float,
    r_bear_m: float,
    c_clear_m: float,
    w_load_n: float,
) -> float:
    if w_load_n < 1e-6:
        return 0.01
    n_rps = rpm / 60.0
    som = (
        viscosity_pa_s * n_rps * l_bear_m * d_bear_m / w_load_n
        * (r_bear_m / c_clear_m) ** 2
    )
    som = max(som, 0.01)
    return 1.0 - 1.0 / (2 * som + 1.0)


def generate_spindle_data(
    profile: SpindleProfile,
    tick: int,
    interval_sec: int,
    physics: bool = True,
) -> dict:
    rpm = profile.current_rpm(tick, interval_sec)

    if physics:
        m_kg = 0.5
        l_shaft = 0.3
        d_shaft = 0.008
        youngs = 210e9
        i_shaft = math.pi * d_shaft ** 4 / 64.0
        k_shaft = 48 * youngs * i_shaft / l_shaft ** 3
        eccentricity_m = 0.0001
        damping = 0.02

        linear_amp_m = jeffcott_unbalance_response(
            rpm, k_shaft, m_kg, eccentricity_m, damping
        )

        b_l = 0.02
        b_d = 0.016
        b_r = 0.008
        mu_eff = profile.oil.effective_viscosity()
        c_eff = profile.oil.clearance_ratio()
        w = m_kg * 9.81
        epsilon = short_bearing_eccentricity_ratio(
            rpm, mu_eff, b_l, b_d, b_r, c_eff, w
        )
        epsilon = max(0.01, min(0.95, epsilon))
        omega_cr = math.sqrt(k_shaft / m_kg)
        omega = rpm * 2 * math.pi / 60.0
        r = omega / max(omega_cr, 1e-6)
        threshold = 0.55 if epsilon >= 0.6 else 0.50 if epsilon >= 0.3 else 0.45
        whirl = r > threshold and epsilon > 0.2
        if whirl:
            growth = 1.0 + 2.5 * max(r - threshold, 0.0) * max(epsilon - 0.2, 0.0) * 10.0
            growth = min(growth, 8.0)
        else:
            growth = 1.0
        disp_m = linear_amp_m * growth
        if profile.wear_severity > 0:
            disp_m *= 1.0 + profile.wear_severity * 4.0
        alpha_nl = 5e6
        disp_for_nonlin = min(disp_m, 0.001)
        nl_factor = 1.0 + alpha_nl * disp_for_nonlin * disp_for_nonlin
        disp_m *= max(0.5, min(5.0, nl_factor))
        vib_mm = disp_m * 1000.0 + profile.vibration_offset
        vib_mm += random.gauss(0, 0.02)
        vib_mm = max(0.01, min(10.0, vib_mm))
    else:
        vib_mm = (
            DEFAULT_BASE_VIBRATION
            + 0.1 * math.sin(tick * 0.1 + profile.idx * 0.7)
            + random.gauss(0, 0.02)
        )
        if rpm > 2500:
            vib_mm += (rpm - 2500) * 0.0003
        vib_mm = max(0.01, min(2.0, vib_mm))

    temperature = (
        DEFAULT_BASE_TEMPERATURE
        + (rpm / 1000.0) * 5.0
        + profile.temperature_offset
        + random.gauss(0, 1.0)
    )
    profile.oil.temperature_penalty = max(0.0, (temperature - 50.0) / 80.0)
    temperature = max(20.0, min(200.0, temperature))

    twist = (
        DEFAULT_BASE_TWIST
        + 50 * math.sin(tick * 0.03 + profile.idx * 1.3)
        + profile.twist_offset
        + random.gauss(0, 20)
    )
    twist = max(0.0, min(10000.0, twist))

    return {
        "spindle_id": f"SPD-{profile.idx:03d}",
        "rpm": round(rpm, 2),
        "vibration_amplitude": round(vib_mm, 4),
        "temperature": round(temperature, 2),
        "twist_per_meter": round(twist, 1),
    }


def on_connect(client, userdata, flags, rc):
    host = userdata.get("host")
    port = userdata.get("port")
    if rc == 0:
        print(f"[mqtt] connected to {host}:{port}")
    else:
        print(f"[mqtt] connection failed code={rc}")


def parse_idx_list(s: Optional[str]):
    if not s:
        return []
    out = []
    for part in s.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            out.extend(range(int(a), int(b) + 1))
        else:
            out.append(int(part))
    return out


def main():
    parser = argparse.ArgumentParser(description="Ancient Water Spindle Sensor Simulator")
    parser.add_argument("--host", default="localhost", help="MQTT broker host")
    parser.add_argument("--port", type=int, default=1883, help="MQTT broker port")
    parser.add_argument("--interval", type=int, default=60, help="Publish interval in seconds")
    parser.add_argument("--spindles", type=int, default=DEFAULT_SPINDLE_COUNT,
                        help="Number of spindles to simulate")
    parser.add_argument("--once", action="store_true",
                        help="Publish one batch per spindle and exit")

    parser.add_argument("--mode", choices=["random", "constant", "sweep", "step"],
                        default="random", help="RPM time-series mode")
    parser.add_argument("--rpm-base", type=float, default=DEFAULT_BASE_RPM,
                        help="Base RPM for random/constant mode")
    parser.add_argument("--rpm-variance", type=float, default=DEFAULT_RPM_VARIANCE,
                        help="RPM sinusoidal amplitude +/- (random) or noise stddev ratio")
    parser.add_argument("--rpm-sweep-min", type=float, default=600.0,
                        help="RPM sweep/step lower bound")
    parser.add_argument("--rpm-sweep-max", type=float, default=3600.0,
                        help="RPM sweep/step upper bound")
    parser.add_argument("--rpm-sweep-minutes", type=float, default=10.0,
                        help="RPM sweep full-cycle or step-hold duration in minutes")
    parser.add_argument("--rpm-spread-per-spindle", type=float, default=0.0,
                        help="Per-spindle RPM base offset (to simulate slightly differing setups)")

    parser.add_argument("--oil-viscosity", type=float, default=None,
                        help="Override oil viscosity in Pa*s (default 0.01)")
    parser.add_argument("--oil-clearance", type=float, default=None,
                        help="Override bearing radial clearance in meters (default 5e-5)")
    parser.add_argument("--oil-aging", type=float, default=0.0,
                        help="Oil aging factor 0.0=new, 1.0=degraded, 2.0=severe")
    parser.add_argument("--oil-contamination", type=float, default=0.0,
                        help="Oil particulate contamination fraction 0..1")

    parser.add_argument("--inject-worn-idx", type=str, default=None,
                        help="Comma/ dash separated list of spindle indices (1-based) that have extra wear")
    parser.add_argument("--no-physics", action="store_true",
                        help="Disable rotor dynamics in simulator (pure empirical model)")
    parser.add_argument("--topic", default=MQTT_TOPIC,
                        help="MQTT publish topic")
    parser.add_argument("--qos", type=int, default=1, choices=[0, 1, 2],
                        help="MQTT QoS level")

    args = parser.parse_args()

    oil_defaults = OilFilmCondition()
    worn_idx = parse_idx_list(args.inject_worn_idx)
    args.inject_worn_idx = worn_idx
    profiles = [
        build_profile(i, args, oil_defaults)
        for i in range(1, args.spindles + 1)
    ]

    client = mqtt.Client(userdata={"host": args.host, "port": args.port})
    client.on_connect = on_connect

    try:
        client.connect(args.host, args.port, 60)
    except Exception as exc:
        print(f"[mqtt] failed to connect: {exc}")
        raise
    client.loop_start()

    tick = 0
    topic = args.topic
    qos = args.qos
    try:
        while True:
            total_success = 0
            lines = []
            for prof in profiles:
                data = generate_spindle_data(prof, tick, args.interval, not args.no_physics)
                payload = json.dumps(data)
                result = client.publish(topic, payload, qos=qos)
                if result.rc == mqtt.MQTT_ERR_SUCCESS:
                    total_success += 1
                lines.append(
                    f"[{time.strftime('%H:%M:%S')}] {data['spindle_id']}: "
                    f"RPM={data['rpm']:.0f} VIB={data['vibration_amplitude']:.3f}mm "
                    f"TEMP={data['temperature']:.1f}C TWIST={data['twist_per_meter']:.0f}/m "
                    f"qos={qos}{' WEARED' if prof.wear_severity>0 else ''}"
                )
            for line in lines:
                print(line)
            print(f"[batch-{tick}] {total_success}/{len(profiles)} published on '{topic}'")
            tick += 1
            if args.once:
                break
            time.sleep(args.interval)
    except KeyboardInterrupt:
        print("\nSimulator stopped.")
    finally:
        client.loop_stop()
        client.disconnect()


if __name__ == "__main__":
    main()
