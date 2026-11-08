"""Generate the drum seed profiles' SQL (docs/profiles.md, "Drum seeds").

    python3 scripts/gen_drum_profiles.py >> crates/server/migrations/0NN_name.sql

Migration 051 holds the first output. The SQL upserts like the fluid-bed generator's: each
profile row is inserted or updated and its points and events replaced, so a later migration
generated from this script changes the curves in deployed databases. Applied migrations are
never edited.

The curves are written from published drum practice (sources in docs/profiles.md): a preheated
drum charged near 200 °C, the turning point near 1:30 and 100 °C, drying to 150 °C by about
4:30, first crack at 196 °C, the drop 1:30 (light) to 2:15 (medium) later at 15-21 % of the
roast. BT falls from the charge to the turning point (a parabola), then follows a rate of rise
that declines linearly in time, fitted through first crack and the drop:

    BT(s) = T_tp + r0·s − k·s²/2,  s = t − t_tp

Burner and air are step changes, as drum roasters make them: burner down through Maillard
and before first crack, air up at the dry end and before first crack. The steps were picked
so the simulated drum (`virtual_drum`) follows the curve; `virtual_drum.rs`'s test
`the_drum_seeds_run_open_loop_on_the_simulated_drum` replays them and checks it.
"""
import sys

DRY = 150.0
FC = 196.0
KIND = "drum_gas"
SOURCES = "Written from published drum roasting practice (Rao, Green Coffee Collective) and checked on the simulated drum."

PROFILES = [
    dict(
        id="drum-light-filter", name="Drum — Light (filter)", level="light", purpose="filter",
        charge=200.0, tp=(85, 97.0), fc=495, drop=(585, 205.0),
        burner=[(0, 65), (375, 60), (465, 55)], air=[(0, 40), ("dry", 50), (465, 60)],
        desc="Light filter roast for a small drum, preheated. Charge ~200 °C, turning point ~1:25 at ~97 °C, "
        "dry end ~4:15 at 150 °C, FC ~8:15 at ~196 °C, drop 9:45 at 205 °C (development 1:30, ~15 %). "
        "Burner 65 % stepping to 55 % before first crack; air 40 % rising to 60 %. ",
    ),
    dict(
        id="drum-medium-omni", name="Drum — Medium omni", level="medium", purpose="omni",
        charge=205.0, tp=(90, 100.0), fc=525, drop=(660, 211.0),
        burner=[(0, 65), (300, 60)], air=[(0, 30), ("dry", 40), (495, 50)],
        desc="Medium roast for a small drum that works as filter and espresso, preheated. Charge ~205 °C, "
        "turning point ~1:30 at ~100 °C, dry end ~4:35 at 150 °C, FC ~8:45 at ~196 °C, drop 11:00 at 211 °C "
        "(development 2:15, ~20 %). Burner 65 % stepping to 60 % at 5:00; air 30 % rising to 50 %. ",
    ),
]


def curve(p):
    t_tp, T_tp = p["tp"]
    t_drop, T_drop = p["drop"]
    s2, s3 = p["fc"] - t_tp, t_drop - t_tp
    k = ((T_drop - T_tp) - s3 * (FC - T_tp) / s2) / (s3 * s2 / 2 - s3 * s3 / 2)
    r0 = (FC - T_tp + k * s2 * s2 / 2) / s2

    def bt(t):
        if t <= t_tp:
            return T_tp + (p["charge"] - T_tp) * (1 - t / t_tp) ** 2
        s = t - t_tp
        return T_tp + r0 * s - k * s * s / 2

    ror = lambda t: (r0 - k * (t - t_tp)) * 60
    return bt, ror


def step(steps, t):
    v = steps[0][1]
    for t0, x in steps:
        if t >= t0:
            v = x
    return v


sql = []
for p in PROFILES:
    pid = p["id"]
    bt, ror = curve(p)
    t_tp, t_drop, T_drop = p["tp"][0], p["drop"][0], p["drop"][1]
    t_dry = next(t for t in range(t_tp, t_drop) if bt(t) >= DRY)
    air = [(t_dry if t == "dry" else t, v) for t, v in p["air"]]
    burner = p["burner"]
    assert ror(t_drop) > 2 and ror(t_tp) > ror(t_drop), "RoR must decline and stay positive"
    print(f"{pid}: dry {t_dry // 60}:{t_dry % 60:02d}, RoR TP {ror(t_tp):.1f} dry {ror(t_dry):.1f} "
          f"FC {ror(p['fc']):.1f} drop {ror(t_drop):.1f} °C/min, DTR {100 * (t_drop - p['fc']) / t_drop:.1f} %",
          file=sys.stderr)
    desc = p["desc"] + SOURCES
    sql.append(
        "INSERT INTO roast_profiles (id, name, description, is_public, target_total_time, target_first_crack, "
        "target_end_temp, charge_temp, roast_level, roast_purpose, roaster_kind) VALUES "
        f"('{pid}', '{p['name']}', '{desc}', 1, {t_drop}, {p['fc']}, {T_drop}, {p['charge']}, "
        f"'{p['level']}', '{p['purpose']}', '{KIND}') "
        "ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, "
        "target_total_time = excluded.target_total_time, target_first_crack = excluded.target_first_crack, "
        "target_end_temp = excluded.target_end_temp, charge_temp = excluded.charge_temp, "
        "roast_level = excluded.roast_level, roast_purpose = excluded.roast_purpose, "
        "roaster_kind = excluded.roaster_kind;")
    sql.append(f"DELETE FROM profile_points WHERE profile_id = '{pid}';")
    sql.append(f"DELETE FROM profile_events WHERE profile_id = '{pid}';")
    # Every 30 s, the turning point, first crack, and each step of the burner and air.
    times = sorted(set(range(0, t_drop, 30)) | {t_tp, p['fc'], t_drop} | {t for t, _ in burner + air})
    rows = [f"('{pid}-p{i:02d}', '{pid}', {t}, {bt(t):.1f}, {step(air, t)}, {step(burner, t)})"
            for i, t in enumerate(times)]
    sql.append("INSERT INTO profile_points (id, profile_id, time_seconds, target_temp, fan_speed, heater_pct) VALUES\n"
               + ",\n".join(rows) + ";")
    sql.append(
        "INSERT INTO profile_events (id, profile_id, trigger_kind, at_seconds, at_bt, action, value, label) VALUES\n"
        f"('{pid}-charge', '{pid}', 'time', 0, NULL, 'landmark', NULL, 'charge'),\n"
        f"('{pid}-tp', '{pid}', 'time', {t_tp}, NULL, 'landmark', NULL, 'turning_point'),\n"
        f"('{pid}-dry', '{pid}', 'bt', {t_dry}, {DRY}, 'landmark', NULL, 'drying_end'),\n"
        f"('{pid}-fc', '{pid}', 'bt', {p['fc']}, {FC}, 'landmark', NULL, 'first_crack_start'),\n"
        f"('{pid}-drop', '{pid}', 'time', {t_drop}, NULL, 'landmark', NULL, 'drop');")
    sql.append("")
print("\n".join(sql))
