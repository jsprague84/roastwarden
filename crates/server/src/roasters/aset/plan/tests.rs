use serde_json::json;

use crate::roasters::aset::ini::Ini;
use crate::roasters::channels::{Channel, MAX_CHANNELS};
use crate::roasters::setup::{Kind, Output, Outputs};
use crate::roasters::types::ModbusDataType;

use super::inputs::{
    extra_curves, formula_scale, rate_name, Etypes, Input, Reads, EXTRAS_MODBUS, EXTRAS_WEBSOCKET,
};
use super::names::{key_of, output_role, setpoint_name, temp_name, unit_in};
use super::phidget::thermocouple_of;
use super::sliders::sliders;
use super::{channel_fields, finish, parse_write, plan, poll_ms, unique};

fn ini(text: &str) -> Ini {
    Ini::parse(text)
}

#[test]
fn names_say_what_a_curve_is() {
    for (n, r) in [
        ("Burner", "heater"),
        ("Gas", "heater"),
        ("Flame", "heater"),
        ("Brenner", "heater"),
        ("Power", "heater"),
        ("Heat", "heater"),
        ("Damper", "damper"),
        ("Flap", "damper"),
        ("Klappe", "damper"),
        ("Drum", "drum"),
        ("Trommel", "drum"),
        ("Air", "fan"),
        ("Fan", "fan"),
        ("Blower", "fan"),
        ("Luft", "fan"),
        ("Lüfter", "fan"),
    ] {
        assert_eq!(output_role(n), Some(r), "{n}");
    }
    assert_eq!(output_role("Inlet"), None);
    for n in ["SP-Air", "Burner SV", "Set Air", "Setpoint", "Target BT"] {
        assert!(setpoint_name(n), "{n}");
    }
    assert!(!setpoint_name("Spare") && !setpoint_name("Burner"));
    for n in [
        "IT", "DT", "CT", "XT", "ET", "BT", "MET", "AT", "ST", "Inlet", "Exhaust", "Stack", "Env",
        "Ambient", "25°",
    ] {
        assert!(temp_name(n), "{n}");
    }
    for n in ["Gas Pressure Pa", "Drum RPM", "Heat", "Bitter"] {
        assert!(!temp_name(n), "{n}");
    }
    for (n, u) in [
        ("Gas Pressure Pa", "Pa"),
        ("Drum RPM", "rpm"),
        ("daPa", "daPa"),
        ("Gas kPa", "kPa"),
        ("mbar", "mbar"),
        ("Fan Hz", "Hz"),
        ("kWh", "kWh"),
        ("Heater kW", "kW"),
        ("Humidity", "%"),
        ("Fan %", "%"),
    ] {
        assert_eq!(unit_in(n), Some(u), "{n}");
    }
    assert_eq!(unit_in("Inlet"), None);
    assert_eq!(key_of("Inlet Temp"), "inlet_temp");
    assert_eq!(key_of("  Gas-Pressure (Pa) "), "gas_pressure_pa");
    assert_eq!(key_of("3rd probe"), "ch_3rd_probe");
    assert_eq!(key_of("-Inlet"), "inlet");
    assert_eq!(key_of(&"x".repeat(40)).len(), 32);
    for n in ["RoR", "ET RoR", "Rate of Rise", "°C/min"] {
        assert!(rate_name(n), "{n}");
    }
    assert!(!rate_name("Rotor"));
}

#[test]
fn a_formula_that_is_only_a_scale() {
    assert_eq!(formula_scale("x*1000"), Some(1000.0));
    assert_eq!(formula_scale("x/10"), Some(0.1));
    assert_eq!(formula_scale("0.1*x"), Some(0.1));
    assert_eq!(formula_scale(" X * 2 "), Some(2.0));
    for f in ["x*0", "x+1", "x*abc", "", "x/0"] {
        assert_eq!(formula_scale(f), None, "{f}");
    }
}

#[test]
fn the_poll_thermocouples_unique_names_and_writes() {
    assert_eq!(poll_ms(&ini(""), 250), 1000);
    assert_eq!(poll_ms(&ini("[General]\nDelay=0\n"), 250), 1000);
    assert_eq!(poll_ms(&ini("[General]\nDelay=50\n"), 250), 250);
    assert_eq!(poll_ms(&ini("[General]\nDelay=99999\n"), 250), 10_000);
    assert_eq!(poll_ms(&ini("[General]\nDelay=1500\n"), 250), 1500);
    for (c, t) in [
        ("1", "K"),
        ("2", "J"),
        ("3", "E"),
        ("4", "T"),
        (" 3 ", "E"),
        ("9", "K"),
        ("", "K"),
    ] {
        assert_eq!(thermocouple_of(c), t, "{c:?}");
    }
    let mut used = Vec::new();
    assert_eq!(unique(&mut used, "BT"), "BT");
    assert_eq!(unique(&mut used, "bt"), "bt 2");
    assert_eq!(unique(&mut used, "BT"), "BT 3");
    assert_eq!(unique(&mut used, &"n".repeat(70)).len(), 60);
    assert_eq!(parse_write("writeLong(1,5,{})"), Some(("writeLong", 1, 5)));
    assert_eq!(parse_write("wcoil(2,7,{})"), Some(("wcoil", 2, 7)));
    assert_eq!(parse_write("writem(1,2,{})"), Some(("writeSingle", 1, 2)));
    assert_eq!(parse_write("writeSingle(0,2,{})"), None);
    assert_eq!(parse_write("writeSingle(248,2,{})"), None);
    assert_eq!(parse_write("writeSingle(1,65536,{})"), None);
}

#[test]
fn event_types_fill_defaults_and_find_templates() {
    let et = Etypes::read(&ini("[events]\netypes=Luft, , Klappe\n"));
    assert_eq!(et.get(0), "Luft");
    assert_eq!(et.get(1), "Drum");
    assert_eq!(et.get(2), "Klappe");
    assert_eq!(et.get(3), "Burner");
    assert_eq!(et.get(4), "--");
    assert_eq!(et.get(5), "");
    assert_eq!(et.named(" {0} Gas "), "Luft Gas");
    assert_eq!(et.template("{3}"), Some(3));
    assert_eq!(et.template("{3} x"), None);
    assert_eq!(et.mentioned("Burner {3}"), Some(3));
    assert_eq!(et.mentioned("{1}{0}"), Some(0));
    assert_eq!(et.mentioned("x"), None);
    assert_eq!(Etypes::read(&ini("")).get(3), "Burner");
}

#[test]
fn an_inputs_data_type_follows_artisans_flags() {
    let input = |float, int32, bcd32, bcd16, signed| Input {
        n: 1,
        unit: 1,
        register: 0,
        code: 3,
        div: 0,
        mode: "C".into(),
        float,
        int32,
        bcd32,
        bcd16,
        signed,
    };
    assert_eq!(
        input(true, true, true, true, true).data_type(),
        ModbusDataType::Float32
    );
    assert_eq!(
        input(false, true, false, false, true).data_type(),
        ModbusDataType::Int32
    );
    assert_eq!(
        input(false, true, false, false, false).data_type(),
        ModbusDataType::Uint32
    );
    assert_eq!(
        input(false, false, true, false, false).data_type(),
        ModbusDataType::Bcd32
    );
    assert_eq!(
        input(false, false, false, true, false).data_type(),
        ModbusDataType::Bcd16
    );
    assert_eq!(
        input(false, false, false, false, true).data_type(),
        ModbusDataType::Int16
    );
    assert_eq!(
        input(false, false, false, false, false).data_type(),
        ModbusDataType::Uint16
    );
}

#[test]
fn extra_curves_note_what_is_not_read_here() {
    let et = Etypes::read(&ini(""));
    let text = "[ExtraDev]\nextradevices=33, 25, 25, 50, 92, 29\n\
                extraname1=IT, Heat, , Fifty, Ninety, Other\n\
                extraname2=Exhaust, {3} x, , , , \n";
    let mut notes = Vec::new();
    let shown = extra_curves(&ini(text), &et, &EXTRAS_MODBUS, &[], &mut notes);
    assert_eq!(shown.get(&3).map(|c| c.0.as_str()), Some("IT"));
    assert_eq!(shown.get(&4).map(|c| c.0.as_str()), Some("Exhaust"));
    assert_eq!(shown.len(), 2);
    assert_eq!(
        notes,
        vec![
            "left out, not read through this connection: a formula curve (Heat, Burner x); \
              Modbus (Other)"
        ]
    );
    let mut notes = Vec::new();
    let text = "[ExtraDev]\nextradevices=29, 50\nextraname1=Same, Fifty\n";
    extra_curves(&ini(text), &et, &EXTRAS_MODBUS, &[29], &mut notes);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_curve_becomes_a_role_a_temperature_or_a_channel() {
    let i = ini("[Sliders]\neventsliderunits=%, Hz, , %\n");
    let et = Etypes::read(&i);
    let mut r = Reads::default();
    let mut notes = Vec::new();
    let unit = |r: &Reads| r.channels.last().map(|c| c.unit.clone());
    // An event type's read-back is that output, even where the device marks it a temperature.
    let c = r.curve(&i, &et, "{3}", "", true, "", 3, "Input", &mut notes);
    assert_eq!(
        (c.name.as_str(), c.role, c.temp, c.key.as_deref()),
        ("Burner", Some("heater"), false, None)
    );
    // The same role twice: the second is a channel, in its slider's unit.
    let c = r.curve(&i, &et, "Heat {3}", "", false, "", 4, "Input", &mut notes);
    assert_eq!((c.role, c.key.as_deref()), (None, Some("heat_burner")));
    assert_eq!(unit(&r).as_deref(), Some("%"));
    // A temperature by its name, by the device's mode, and a unit in the name.
    let c = r.curve(&i, &et, "Inlet", "", false, "", 5, "Input", &mut notes);
    assert_eq!((c.temp, c.key.as_deref()), (true, Some("inlet")));
    assert_eq!(unit(&r).as_deref(), Some("C"));
    let c = r.curve(&i, &et, "Air", "", true, "", 6, "Input", &mut notes);
    assert_eq!(
        (c.role, c.temp, c.key.as_deref()),
        (None, true, Some("air"))
    );
    let c = r.curve(
        &i,
        &et,
        "Gas Pressure Pa",
        "x*10",
        false,
        "",
        7,
        "Input",
        &mut notes,
    );
    assert_eq!(
        (c.temp, c.scale, c.key.as_deref()),
        (false, Some(10.0), Some("gas_pressure_pa"))
    );
    assert_eq!(unit(&r).as_deref(), Some("Pa"));
    assert!(notes.last().unwrap().contains("is a scale"));
    // A formula that is not a scale is noted and the value kept raw; a temperature's is never
    // applied.
    let c = r.curve(&i, &et, "Flow", "x+1", false, "", 8, "Input", &mut notes);
    assert_eq!(c.scale, None);
    assert!(notes.last().unwrap().contains("keeps the raw value"));
    let c = r.curve(&i, &et, "Exhaust", "x*2", true, "", 9, "Input", &mut notes);
    assert_eq!((c.temp, c.scale), (true, None));
    // A setpoint, a rate, a unit in the hint, and the ambient temperature.
    let c = r.curve(&i, &et, "Burner SV", "", false, "", 10, "Input", &mut notes);
    assert_eq!(
        (c.role, c.temp, c.key.as_deref()),
        (None, false, Some("burner_sv"))
    );
    assert_eq!(unit(&r).as_deref(), Some("#"));
    let c = r.curve(&i, &et, "ET RoR", "", true, "", 11, "Input", &mut notes);
    assert_eq!((c.temp, unit(&r).as_deref()), (false, Some("C/min")));
    let c = r.curve(
        &i,
        &et,
        "Drum",
        "",
        false,
        "sensors/drum_rpm",
        12,
        "Input",
        &mut notes,
    );
    assert_eq!((c.role, unit(&r).as_deref()), (None, Some("rpm")));
    let c = r.curve(&i, &et, "Ambient", "", true, "", 13, "Input", &mut notes);
    assert_eq!((c.role, c.temp, c.key), (Some("ambient"), true, None));
    // An unnamed curve, and a key already taken.
    let c = r.curve(&i, &et, "--", "", true, "", 14, "Input", &mut notes);
    assert_eq!(c.key.as_deref(), Some("input_14"));
    let c = r.curve(&i, &et, "INLET", "", true, "", 15, "Input", &mut notes);
    assert_eq!(c.key.as_deref(), Some("inlet_15"));
    assert!(!r.key_free("bt") && !r.key_free("inlet") && r.key_free("stack"));
}

#[test]
fn channels_are_capped_at_the_most_a_roaster_declares() {
    let ch = |k: &str| Channel {
        key: k.into(),
        label: k.to_uppercase(),
        unit: "#".into(),
        max: None,
    };
    let mut r = Reads::default();
    for i in 0..MAX_CHANNELS {
        r.channels.push(ch(&format!("c{i}")));
    }
    let mut notes = Vec::new();
    assert_eq!(r.cap(&mut notes).len(), MAX_CHANNELS);
    assert_eq!(notes, [] as [String; 0]);
    r.channels.push(ch("x"));
    r.channels.push(ch("y"));
    let keep = r.cap(&mut notes);
    assert_eq!((keep.len(), r.channels.len()), (MAX_CHANNELS, MAX_CHANNELS));
    assert_eq!(
        notes,
        vec![format!(
            "{} extra curves: the first {MAX_CHANNELS} are channels, the rest left out (X, Y)",
            MAX_CHANNELS + 2
        )]
    );
}

#[test]
fn sliders_become_outputs_or_are_noted() {
    let et = Etypes::read(&ini(""));
    let run = |sl: &str| {
        let mut notes = Vec::new();
        let out = sliders(
            &ini(&format!("[Sliders]\n{sl}")),
            &et,
            "2",
            "Modbus write",
            &mut notes,
        );
        (out, notes)
    };
    // Action 0, no action and no command are skipped without a note; another action is noted.
    let (out, notes) = run("slideractions=0, , 2, 1\n\
         slidercommands=\"writeSingle(1,1,{})\", \"writeSingle(1,2,{})\", , \"serial\"\n");
    assert!(out.is_empty());
    assert_eq!(
        notes,
        vec!["slider Burner: its action is not a Modbus write (serial); left out"]
    );
    // A slider in % up to 100 is that %; the bottom value is min × factor + offset.
    let (out, notes) = run(
        "slideractions=2\nslidercommands=\"writeSingle(1,1,{})\"\nsliderfactors=10\n\
         slideroffsets=5\nslidermin=40\nslidermax=100\neventsliderunits=%\n",
    );
    assert_eq!(out.len(), 1);
    let s = &out[0];
    assert_eq!(
        (
            s.role,
            s.k,
            s.o,
            s.low,
            s.output.min,
            s.output.max,
            s.output.label.as_str()
        ),
        ("fan", 10.0, 5.0, 405.0, 40.0, 100.0, "Air")
    );
    assert!(notes.is_empty(), "{notes:?}");
    // A slider in Hz is % of its top.
    let (out, notes) = run(
        "slideractions=2\nslidercommands=\"writeSingle(1,1,{})\"\nsliderfactors=2\n\
         slidermin=40\nslidermax=60\neventsliderunits=Hz\n",
    );
    let s = &out[0];
    assert_eq!(
        (s.k, s.output.min, s.output.max, s.output.label.as_str()),
        (1.2, 66.7, 100.0, "Air (% of 60 Hz)")
    );
    assert_eq!(
        notes,
        vec!["slider Air goes 40–60 Hz: its output is % of 60 Hz"]
    );
    // No unit but a top above 100 is % of the top too.
    let (out, _) = run("slideractions=2\nslidercommands=\"writeSingle(1,1,{})\"\nslidermax=200\n");
    assert_eq!(
        (out[0].output.max, out[0].k, out[0].output.label.as_str()),
        (100.0, 2.0, "Air (% of 200)")
    );
    // Left out: a zero factor, a range the wrong way round, a negative bottom, a temperature.
    for (sl, what) in [
        ("sliderfactors=0", "cannot be an output"),
        ("slidermin=60\nslidermax=40", "cannot be an output"),
        ("slidermin=-1", "cannot be an output"),
        ("eventsliderunits=C", "sets a temperature"),
        ("eventsliderunits=f", "sets a temperature"),
    ] {
        let (out, notes) = run(&format!(
            "slideractions=2\nslidercommands=\"writeSingle(1,1,{{}})\"\n{sl}\n"
        ));
        assert!(out.is_empty(), "{sl}");
        assert!(notes[0].contains(what), "{sl}: {notes:?}");
    }
    // A slider whose name is no output.
    let lights = Etypes::read(&ini("[events]\netypes=Lights\n"));
    let mut notes = Vec::new();
    let out = sliders(
        &ini("[Sliders]\nslideractions=2\nslidercommands=\"writeSingle(1,1,{})\"\n"),
        &lights,
        "2",
        "Modbus write",
        &mut notes,
    );
    assert!(out.is_empty());
    assert_eq!(
        notes,
        vec!["slider Lights (`writeSingle(1,1,{})`): not a heater, fan, drum or damper; left out"]
    );
}

#[test]
fn the_kind_comes_from_the_heating_else_from_the_outputs() {
    let heater = Output {
        label: "Heater".into(),
        min: 0.0,
        max: 100.0,
        step: 0.0,
    };
    let kind = |text: &str, outputs: Outputs| {
        let mut p = plan("", "x");
        p.notes.clear();
        finish(&ini(text), &mut p, outputs, vec![], Kind::DrumElectric);
        (p.setup.unwrap().kind, p.notes)
    };
    let (k, notes) = kind("[General]\nroasterheating_setup=1\n", Outputs::default());
    assert_eq!(k, Kind::DrumGas);
    assert!(notes[0].contains("(LPG)"), "{notes:?}");
    let (k, notes) = kind("[General]\nroasterheating_setup=2\n", Outputs::default());
    assert_eq!(k, Kind::DrumGas);
    assert!(notes[0].contains("(natural gas)"));
    let (k, notes) = kind("[General]\nroasterheating_setup=3\n", Outputs::default());
    assert_eq!(k, Kind::DrumElectric);
    assert!(notes[0].contains("(electric)"));
    let (k, notes) = kind("", Outputs::default());
    assert_eq!(k, Kind::Logger);
    assert!(notes[0].contains("(not given: a guess)"));
    assert_eq!(notes[1], "no outputs: Roastwarden only records it");
    let (k, notes) = kind(
        "",
        Outputs {
            heater: Some(heater.clone()),
            ..Default::default()
        },
    );
    assert_eq!(k, Kind::DrumElectric);
    assert!(notes[1].starts_with("outputs heater from"), "{notes:?}");
    let (k, _) = kind(
        "",
        Outputs {
            fan: Some(heater),
            ..Default::default()
        },
    );
    assert_eq!(k, Kind::DrumElectric);
}

#[test]
fn channel_fields_follow_the_paths_and_note_what_is_missing() {
    let i = ini(
        "[ExtraDev]\nextradevices=112, 113\nextraname1=Inlet, Pressure kPa\n\
         extraname2=Exhaust, {0}\n",
    );
    let et = Etypes::read(&i);
    let path = |n: usize| -> Option<String> {
        match n {
            1 => Some("data.bt".into()),
            2 => Some("data.et".into()),
            3 => Some("data.inlet".into()),
            5 => Some("data.p".into()),
            6 => Some("data.air".into()),
            7 => Some("data.seven".into()),
            _ => None,
        }
    };
    let unit = |n: usize| -> Option<&'static str> {
        match n {
            1 => Some("F"),
            3 => Some("C"),
            _ => None,
        }
    };
    let mut notes = Vec::new();
    let (fields, channels) =
        channel_fields(&i, &et, &EXTRAS_WEBSOCKET, &path, &unit, "C", &mut notes);
    assert_eq!(fields["bt"], json!({ "path": "data.bt", "channel": 1 }));
    assert_eq!(
        fields["inlet"],
        json!({ "path": "data.inlet", "channel": 3, "unit": "C" })
    );
    assert_eq!(
        fields["pressure_kpa"],
        json!({ "path": "data.p", "channel": 5 })
    );
    assert_eq!(fields["fan"], json!({ "path": "data.air", "channel": 6 }));
    assert_eq!(
        channels.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
        vec!["inlet", "pressure_kpa"]
    );
    for note in [
        "BT is in °F, the device's other temperatures in °C: check it",
        "Exhaust (channel 4): Artisan reads no field for it; left out",
        "channel 7 (data.seven) is read by Artisan but drawn as no curve; left out",
    ] {
        assert!(notes.iter().any(|n| n == note), "{note}: {notes:?}");
    }
}
