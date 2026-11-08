use serde_json::{json, Value};

use crate::roasters::setup::{Control, Kind};
use crate::roasters::types::{ModbusDataType, ModbusRegisterType, Protocol};
use crate::transports::ingest::Role;

use super::ini::Ini;
use super::machines::{machine_text, summary, unsupported};
use super::plan::parse_write;
use super::qt::qt_values;
use super::*;

#[test]
fn qt_values_read_lists_quotes_escapes_and_variants() {
    assert_eq!(qt_values("50, 35, 36"), vec!["50", "35", "36"]);
    assert_eq!(qt_values("Bean Temp, , x"), vec!["Bean Temp", "", "x"]);
    assert_eq!(
        qt_values(r#""setDBint(2,16,{})", "write(1,2,{});sleep(1)""#),
        vec!["setDBint(2,16,{})", "write(1,2,{});sleep(1)"]
    );
    assert_eq!(
        qt_values(r"Machine\n\\1, Burner\nAUTO"),
        vec!["Machine\n\\1", "Burner\nAUTO"]
    );
    assert_eq!(qt_values("plain ; a comment"), vec!["plain"]);
    assert_eq!(qt_values(""), Vec::<String>::new());
    assert_eq!(qt_values("C"), vec!["C"]);
    // A one-element list as Qt writes it: QVariantList [int 33].
    assert_eq!(
        qt_values(r"@Variant(\0\0\0\t\0\0\0\x1\0\0\0\x2\0\0\0!)"),
        vec!["33"]
    );
    assert_eq!(qt_values("@Invalid()"), Vec::<String>::new());
    // A string list with one QString: "Air"
    assert_eq!(
        qt_values(r"@Variant(\0\0\0\t\0\0\0\x1\0\0\0\n\0\0\0\x6\0\x41\0i\0r)"),
        vec!["Air"]
    );
}

#[test]
fn writes_are_parsed() {
    assert_eq!(
        parse_write("writeSingle(1,100,{})"),
        Some(("writeSingle", 1, 100))
    );
    assert_eq!(
        parse_write("writeSingle([3, 4096, {}])"),
        Some(("writeSingle", 3, 4096))
    );
    assert_eq!(
        parse_write("writeWord(1,10,{})"),
        Some(("writeWord", 1, 10))
    );
    assert_eq!(parse_write("write(1,10,-{})"), None);
    assert_eq!(parse_write("writeSingle(1,10,5)"), None);
    assert_eq!(parse_write("setDBint(2,16,{})"), None);
}

const BUS: &str = r#"
[General]
roastertype_setup=Example 5
roasterheating_setup=1
Delay=2000
[Device]
id=29
[Modbus]
type=0
baudrate=9600
parity=E
stopbits=1
bytesize=8
comport=COM4
timeout=0.4
wordorderLittle=true
input1deviceId=1
input1register=4096
input1code=3
input1div=1
input1mode=C
input2deviceId=2
input2register=4096
input2code=3
input2div=1
input2mode=F
input3deviceId=3
input3register=10
input3code=4
input3div=0
input3mode=
input4deviceId=1
input4register=4100
input4code=3
input4div=1
input4mode=C
input5deviceId=1
input5register=200
input5code=3
input5mode=
input6deviceId=1
input6register=201
input6code=3
input6mode=
input7deviceId=1
input7register=300
input7mode=C
PID_device_ID=1
PID_SV_register=4097
SVmultiplier=1
[ExtraDev]
extradevices=33, 55, 25
extraname1=IT, Gas Pressure Pa, Heat
extraname2={3}, ,
extramathexpression1=, , x*2
extramathexpression2=, ,
[Sliders]
slideractions=2, 2, 0, 2
slidercommands="writeSingle(3,20,{})", "writeSingle(1,30,{});mwrite(1,31,0,1)", , "writeSingle([3,21,{}])"
sliderfactors=1, 1, 1, 10
slideroffsets=0, 0, 0, 0
slidermin=0, 40, 0, 0
slidermax=100, 60, 100, 100
eventsliderunits=%, Hz, , %
[events]
etypes=Air, Drum, Damper, Burner, --
"#;

#[test]
fn a_bus_of_controllers_plans_a_roaster() {
    let p = plan(BUS, "Example/Five");
    assert!(p.machine.supported, "{:?}", p.machine.why);
    assert_eq!(p.machine.name, "Example 5");
    assert_eq!(p.protocol, Some(Protocol::ModbusRtu));
    let c = p.config.clone().unwrap();
    assert_eq!(c["baud"], json!(9600));
    assert_eq!(c["parity"], json!("even"));
    assert_eq!(c["unit_id"], json!(1));
    assert_eq!(c["poll_interval_ms"], json!(2000));
    assert_eq!(c["timeout_ms"], json!(400));
    assert_eq!(p.needs, vec!["port"]);

    let reg = |name: &str| p.registers.iter().find(|r| r.name == name).unwrap();
    let bt = reg("BT");
    assert_eq!(bt.role.as_deref(), Some("bt"));
    assert_eq!(bt.unit_id, None, "on the connection's slave");
    assert_eq!(bt.scale_factor, Some(0.1));
    assert_eq!(bt.address, 4096);
    let et = reg("ET");
    assert_eq!(et.unit_id, Some(2));
    assert_eq!(et.unit.as_deref(), Some("F"), "a role converts °F itself");
    // Input 3 is the first curve of device 33 (IT): a temperature channel on slave 3.
    let it = reg("it");
    assert_eq!(it.role, None);
    assert_eq!((it.address, it.unit_id), (10, Some(3)));
    assert_eq!(it.register_type, ModbusRegisterType::Input);
    // Input 4 is `{3}`: the Burner's read-back, the heater role.
    let burner = p
        .registers
        .iter()
        .find(|r| r.role.as_deref() == Some("heater") && r.writable == Some(false))
        .unwrap();
    assert_eq!((burner.address, burner.unit_id), (4100, None));
    assert_eq!(burner.scale_factor, Some(0.1));
    // Input 5 (Gas Pressure Pa) is a channel in Pa; input 6 has an empty name.
    let gas = reg("gas_pressure_pa");
    assert_eq!(gas.address, 200);
    assert!(p.registers.iter().any(|r| r.name == "input_6"));
    // Input 7 is drawn by no curve.
    assert!(p.registers.iter().all(|r| r.address != 300));
    assert!(p.notes.iter().any(|n| n.contains("input 7")));

    // Sliders: Air writes slave 3 register 20; Drum in Hz is % of 60 Hz; Burner ×10.
    let setup = p.setup.clone().unwrap();
    assert_eq!(setup.control, Control::Off);
    assert_eq!(setup.kind, Kind::DrumGas);
    let fan = setup.outputs.fan.clone().unwrap();
    assert_eq!((fan.min, fan.max), (0.0, 100.0));
    let drum = setup.outputs.drum.clone().unwrap();
    assert_eq!(drum.label, "Drum (% of 60 Hz)");
    assert!((drum.min - 66.7).abs() < 0.01);
    let drum_reg = p
        .registers
        .iter()
        .find(|r| r.role.as_deref() == Some("drum"))
        .unwrap();
    // 100 % → 60 Hz: raw = (100 − 0) / scale
    let raw = 100.0 / drum_reg.scale_factor.unwrap();
    assert!((raw - 60.0).abs() < 1e-9);
    assert!(p.notes.iter().any(|n| n.contains("mwrite")));
    let heater = p
        .registers
        .iter()
        .find(|r| r.role.as_deref() == Some("heater") && r.writable == Some(true))
        .unwrap();
    assert_eq!(heater.unit_id, Some(3));
    assert_eq!(heater.address, 21);
    assert!((50.0 / heater.scale_factor.unwrap() - 500.0).abs() < 1e-9);
    // The PID's setpoint, ×10.
    let sv = p
        .registers
        .iter()
        .find(|r| r.role.as_deref() == Some("setpoint"))
        .unwrap();
    assert_eq!((sv.address, sv.scale_factor), (4097, Some(0.1)));
    // The formula curve is named in the notes.
    assert!(p.notes.iter().any(|n| n.contains("formula")));
    // Channels: IT (°C), the gas pressure (Pa) and input 6.
    let keys: Vec<&str> = setup.channels.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, vec!["it", "gas_pressure_pa", "input_6"]);
    assert_eq!(setup.channels[0].unit, "C");
    assert_eq!(setup.channels[1].unit, "Pa");
}

/// The plan's config with what the owner gives filled in (a host, a serial port, a topic).
fn filled(p: &Plan) -> Value {
    let mut config = p.config.clone().unwrap();
    for need in &p.needs {
        match need.as_str() {
            "host" if p.protocol == Some(Protocol::WebSocket) => {
                let url = config["url"]
                    .as_str()
                    .unwrap()
                    .replace("127.0.0.1", "10.0.0.5");
                config["url"] = json!(url);
            }
            "host" => config["host"] = json!("10.0.0.5"),
            "port" => config["port"] = json!("/dev/ttyUSB0"),
            "telemetry_topic" => config["telemetry_topic"] = json!("roest/telemetry"),
            n => panic!("{}: an unknown need {n}", p.machine.key),
        }
    }
    config
}

/// Every machine in the catalogue reads, and every plan Roastwarden makes passes the checks
/// the real calls apply: the connection (its ingest mapping and templates too), the register
/// map, the outputs and the channels; and every channel has a source.
#[test]
fn every_catalogue_machine_plans_a_roaster_the_server_takes() {
    let all = machines();
    assert!(all.len() > 250, "{} machines", all.len());
    let supported: Vec<&Machine> = all.iter().filter(|m| m.supported).collect();
    assert!(supported.len() >= 170, "{} supported", supported.len());
    for m in &all {
        assert!(!m.name.is_empty(), "{}", m.key);
        if !m.supported {
            assert!(m.why.is_some(), "{}", m.key);
        }
    }
    for m in supported {
        let (key, text) = machine_text(&m.key).unwrap();
        let p = plan(text, key);
        let protocol = p
            .protocol
            .clone()
            .unwrap_or_else(|| panic!("{key}: no protocol"));
        let config = filled(&p);
        crate::transports::pollers::check_connection_config(&protocol, &config)
            .unwrap_or_else(|e| panic!("{key}: connection: {e}"));
        crate::transports::pollers::check_register_map(&p.registers)
            .unwrap_or_else(|e| panic!("{key}: register map: {e}"));
        let fields: Vec<String> = config["ingest"]["fields"]
            .as_object()
            .map(|f| f.keys().cloned().collect())
            .unwrap_or_default();
        let modbus = matches!(protocol, Protocol::ModbusRtu | Protocol::ModbusTcp);
        assert!(
            p.registers
                .iter()
                .any(|r| matches!(r.role.as_deref(), Some("bt" | "et")))
                || fields.iter().any(|f| f == "bt" || f == "et")
                || protocol == Protocol::SerialTc4,
            "{key}: no BT or ET"
        );
        assert_eq!(modbus, !p.registers.is_empty(), "{key}: registers");
        let mut setup = p.setup.clone().unwrap();
        crate::roasters::setup::validate(&setup.outputs, setup.fan_floor_pct)
            .unwrap_or_else(|e| panic!("{key}: outputs: {e}"));
        crate::roasters::channels::validate(&mut setup.channels)
            .unwrap_or_else(|e| panic!("{key}: channels: {e}"));
        assert_eq!(setup.control, Control::Off);
        // Each channel has its register or field, and each one with no role is a channel.
        let sources: Vec<String> = p
            .registers
            .iter()
            .filter(|r| r.role.is_none())
            .map(|r| r.name.clone())
            .chain(fields.iter().filter(|f| Role::parse(f).is_none()).cloned())
            .collect();
        for s in &sources {
            assert!(
                setup.channels.iter().any(|c| &c.key == s),
                "{key}: {s} is no channel"
            );
        }
        for c in &setup.channels {
            assert!(
                sources.contains(&c.key)
                    || (protocol == Protocol::SerialTc4
                        && config[format!("{}_channel", c.key)].is_number()),
                "{key}: channel {} has no source",
                c.key
            );
        }
        // An output declared has its way out: a writable register or a template, else
        // a TC4's own line (the heater) or its fan output.
        for (role, out) in [
            ("heater", &setup.outputs.heater),
            ("fan", &setup.outputs.fan),
            ("drum", &setup.outputs.drum),
            ("damper", &setup.outputs.damper),
        ] {
            if out.is_some() {
                assert!(
                    p.registers
                        .iter()
                        .any(|r| r.role.as_deref() == Some(role) && r.writable == Some(true))
                        || config["commands"][role].is_object(),
                    "{key}: {role} declared with no way to send it"
                );
            }
        }
    }
}

#[test]
fn phidgets_machines_plan_a_phidget22_connection() {
    let phidgets: Vec<Machine> = machines()
        .into_iter()
        .filter(|m| m.artisan_device_name == "Phidgets")
        .collect();
    assert_eq!(phidgets.len(), 13);
    assert!(phidgets.iter().all(|m| m.supported), "{phidgets:?}");

    let p = plan_machine("Phidget/USB_1048_Databridge_2xTC").unwrap();
    assert_eq!(p.protocol, Some(Protocol::Phidget22));
    assert_eq!(p.needs, vec!["host"]);
    let c = p.config.clone().unwrap();
    assert_eq!(c["ingest"]["fields"], json!({ "bt": "ch0", "et": "ch1" }));
    assert_eq!(c["thermocouple_type"], "K");
    assert_eq!(c["data_interval_ms"], 32);
    assert_eq!(c["poll_interval_ms"], 2000, "Artisan's Delay");
    let setup = p.setup.unwrap();
    assert_eq!(setup.kind, Kind::Logger, "no outputs");
    assert!(p.notes[0].contains("a 1048") && p.notes[0].contains("phidget22networkserver"));

    // A 1048's inputs 2 and 3 and its own sensor as channels; the 1018's voltages left out.
    let p = plan_machine("Diedrich/6_Sensor_Data_Logging").unwrap();
    let c = p.config.clone().unwrap();
    let f = &c["ingest"]["fields"];
    assert_eq!(f["bt"], "ch0");
    assert_eq!(f["total_exhaust"]["path"], "ch2");
    assert_eq!(f["inlet_temp"]["path"], "ch3");
    assert_eq!(f["board_temp"]["path"], "ch4");
    assert_eq!(c["data_interval_ms"], 256);
    let keys: Vec<&str> = p
        .setup
        .as_ref()
        .unwrap()
        .channels
        .iter()
        .map(|c| c.key.as_str())
        .collect();
    assert_eq!(keys, vec!["total_exhaust", "inlet_temp", "board_temp"]);
    let notes = p.notes.join(" | ");
    assert!(
        notes.contains("left out, not read through this connection"),
        "{notes}"
    );
    assert!(notes.contains("Phidgets (Power (0-100%))"), "{notes}");

    // A 1051: one thermocouple, the board's sensor a channel and no ET.
    let p = plan_machine("Hive Roaster/Data_Dome").unwrap();
    let c = p.config.clone().unwrap();
    assert_eq!(
        c["ingest"]["fields"],
        json!({ "bt": "ch0", "board": "ch1" })
    );
    assert!(p
        .notes
        .iter()
        .any(|n| n.contains("draws the board's own sensor")));

    // A TMP1200: its RTD as the file sets it; the second board's curve left out.
    let p = plan_machine("Phidget/VINT_TMP1200_2xRTD").unwrap();
    let c = p.config.clone().unwrap();
    assert_eq!(c["ingest"]["fields"], json!({ "bt": "ch0" }));
    assert_eq!(
        (c["rtd_type"].clone(), c["rtd_wires"].clone()),
        (json!("pt1000_3850"), json!(4))
    );
    assert_eq!(c["data_interval_ms"], 250);

    // Mill City's Delta controls are read over Modbus: left out, the Phidget's IT kept.
    let p = plan_machine("Mill City Roasters/Phidget_&_Delta_controls_(port_on_the_back)").unwrap();
    let c = p.config.clone().unwrap();
    assert_eq!(c["ingest"]["fields"]["it"]["path"], "ch2");
    assert!(
        p.notes.iter().any(|n| n.contains("Modbus")),
        "{:?}",
        p.notes
    );

    // An owner's file with a remote server keeps it; types per input when they differ.
    let text = "[Device]\nid=58\nphidget1048_types=1, 2, 1, 1\nphidgetRemoteFlag=true\n\
                    phidgetServerID=sbc.local\nphidgetPort=5662\n";
    let p = plan(text, "mine.aset");
    let c = p.config.unwrap();
    assert_eq!(p.needs, [] as [std::string::String; 0]);
    assert_eq!(
        (c["host"].clone(), c["port"].clone()),
        (json!("sbc.local"), json!(5662))
    );
    assert_eq!(c["thermocouple_types"], json!({ "ch1": "J" }));
    assert!(p.notes[0].contains("TMP1101"));
}

#[test]
fn fuji_pids_plan_a_register_map() {
    let p = plan_machine("Sedona Elite/Roaster_PXF").unwrap();
    assert!(p.machine.supported, "{:?}", p.machine.why);
    assert_eq!(p.protocol, Some(Protocol::ModbusRtu));
    let c = p.config.clone().unwrap();
    assert_eq!(
        (c["baud"].clone(), c["parity"].clone()),
        (json!(9600), json!("odd"))
    );
    assert_eq!(
        c["unit_id"],
        json!(1),
        "BT's controller is the connection's"
    );
    let reg = |name: &str| p.registers.iter().find(|r| r.name == name).unwrap();
    let bt = reg("BT");
    assert_eq!(
        (bt.address, bt.unit_id, bt.scale_factor),
        (2000, None, Some(0.1))
    );
    assert_eq!(bt.register_type, ModbusRegisterType::Input);
    let et = reg("ET");
    assert_eq!((et.address, et.unit_id), (2000, Some(2)));
    assert_eq!(reg("ET PID output").role.as_deref(), Some("heater"));
    let sv = reg("ET SV write");
    assert_eq!(
        (sv.address, sv.writable, sv.role.as_deref()),
        (2240, Some(true), Some("setpoint"))
    );
    // A PXR's registers are its own.
    let pxr = plan_machine("Santoker/2xPXR").unwrap();
    let sv = pxr
        .registers
        .iter()
        .find(|r| r.role.as_deref() == Some("setpoint"))
        .unwrap();
    assert_eq!(sv.address, 1002);
    let setup = p.setup.unwrap();
    assert_eq!(setup.channels[0].key, "et_sv");
}

#[test]
fn a_tc4_machine_plans_its_serial_connection_and_commands() {
    let p = plan_machine("Kaleido/Legacy").unwrap();
    assert!(p.machine.supported, "{:?}", p.machine.why);
    assert_eq!(p.protocol, Some(Protocol::SerialTc4));
    assert_eq!(p.needs, vec!["port"]);
    let c = p.config.clone().unwrap();
    assert_eq!(c["baud"], json!(57600));
    assert_eq!(c["reply_format"], json!("active"));
    assert_eq!(c["fan_output"], json!("ot2"));
    assert_eq!(c["commands"]["heater"]["payload"], json!("OT1,{value}"));
    assert_eq!(c["commands"]["drum"]["payload"], json!("IO3,{value}"));
    let setup = p.setup.unwrap();
    assert_eq!(setup.kind, Kind::DrumElectric);
    assert_eq!(setup.outputs.drum.unwrap().min, 10.0);
    let hottop = plan_machine("Hottop/TC4").unwrap();
    assert_eq!(hottop.config.unwrap()["fan_output"], json!("dcfan"));
    assert!(hottop.notes.iter().any(|n| n.contains("Extra 1")));
}

#[test]
fn a_websocket_machine_plans_its_poll_mapping_and_commands() {
    let p = plan_machine("Probat/P_Series_III").unwrap();
    assert!(p.machine.supported, "{:?}", p.machine.why);
    assert_eq!(p.protocol, Some(Protocol::WebSocket));
    assert_eq!(p.needs, vec!["host"]);
    let c = p.config.clone().unwrap();
    assert_eq!(c["url"], json!("ws://127.0.0.1:80/WebSocket"));
    assert_eq!(
        c["poll"]["message"],
        json!("{\"command\":\"getCurrentRoastingStep\",\"id\":{id},\"roasterID\":0}")
    );
    let f = &c["ingest"]["fields"];
    assert_eq!(f["bt"]["path"], json!("data.beanTemp"));
    assert_eq!(f["et"]["path"], json!("data.ambientTemp"));
    assert_eq!(f["heater"]["path"], json!("data.burner"));
    assert_eq!(f["fan"]["path"], json!("data.exhaustFan"));
    assert_eq!(f["drum"]["path"], json!("data.drum"));
    // Pa is Artisan's x*1000; the inlet a temperature; the RoR a rate.
    assert_eq!(f["pa"]["scale"], json!(1000.0));
    assert_eq!(f["inlet"]["unit"], json!("C"));
    assert_eq!(
            c["commands"]["heater"]["payload"],
            json!("{\"command\":\"setControlParams\",\"id\":{id},\"params\":{\"burner\":{value}},\"roasterID\":0}")
        );
    let setup = p.setup.unwrap();
    let ror = setup.channels.iter().find(|c| c.label == "RoR").unwrap();
    assert_eq!(ror.unit, "C/min");
    assert!(setup.outputs.heater.is_some() && setup.outputs.drum.is_some());
}

#[test]
fn an_mqtt_machine_plans_its_mapping_and_needs_its_topic() {
    let p = plan_machine("ROEST/P3000").unwrap();
    assert!(p.machine.supported, "{:?}", p.machine.why);
    assert_eq!(p.protocol, Some(Protocol::Mqtt));
    assert_eq!(p.needs, vec!["telemetry_topic"]);
    let f = &p.config.as_ref().unwrap()["ingest"]["fields"];
    assert_eq!(f["bt"]["path"], json!("data.bt"));
    assert_eq!(f["heater"]["path"], json!("data.heat"));
    assert_eq!(f["dt"]["unit"], json!("C"));
    // rpm is a channel in rpm, not the drum's %; Artisan's JMESPath and [-1] are left out.
    let setup = p.setup.unwrap();
    assert!(setup.channels.iter().any(|c| c.unit == "rpm"));
    assert!(p.notes.iter().any(|n| n.contains("events[-1]")));
    assert!(p.notes.iter().any(|n| n.contains("coap.roestcoffee.com")));
}

#[test]
fn bcd_and_ascii_machines_plan() {
    let loring = plan_machine("Loring/Smart_Roast").unwrap();
    assert!(loring.machine.supported, "{:?}", loring.machine.why);
    let bt = loring.registers.iter().find(|r| r.name == "BT").unwrap();
    assert_eq!(bt.data_type, ModbusDataType::Bcd32);
    assert_eq!(
        (bt.unit.as_deref(), bt.byte_order.as_deref()),
        (Some("F"), Some("CDAB"))
    );
    assert!(loring
        .registers
        .iter()
        .any(|r| r.data_type == ModbusDataType::Bcd16));
    let delta = plan_machine("Nordic/DeltaDTA").unwrap();
    assert!(delta.machine.supported, "{:?}", delta.machine.why);
    let c = delta.config.unwrap();
    assert_eq!(c["framing"], json!("ascii"));
    assert_eq!(
        (c["data_bits"].clone(), c["parity"].clone()),
        (json!(7), json!("even"))
    );
}

#[test]
fn machines_are_found_by_key_and_unsupported_ones_say_why() {
    let (key, _) = machine_text("giesen/w6a").unwrap();
    assert_eq!(key, "Giesen/W6A");
    let giesen = machines()
        .into_iter()
        .find(|m| m.key == "Giesen/W6A")
        .unwrap();
    assert!(!giesen.supported);
    assert_eq!(giesen.artisan_device_name, "Siemens S7");
    assert!(giesen.why.unwrap().contains("Siemens S7"));
    assert!(catalogue_source().contains("artisan"));
}

#[test]
fn qt_escapes_hex_octal_and_quoted_tails() {
    assert_eq!(
        qt_values(r"a\ab\bc\fd\re\vf"),
        vec!["a\u{7}b\u{8}c\u{c}d\re\u{b}f"]
    );
    assert_eq!(qt_values(r"\x41\x42 \101\102"), vec!["AB AB"]);
    // A quoted item keeps its trailing space, an unquoted one is trimmed, and an empty quoted
    // value alone is no item.
    assert_eq!(qt_values(r#""a b " , x   , "c""#), vec!["a b ", "x", "c"]);
    assert_eq!(qt_values(r#""""#), Vec::<String>::new());
    assert_eq!(qt_values("a,\t b"), vec!["a", "b"]);
}

/// `@Variant(…)` with every byte as `\xHH`, as the INI escapes it.
fn variant(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("@Variant(");
    for b in bytes {
        let _ = write!(s, "\\x{b:02x}");
    }
    s.push(')');
    s
}

#[test]
fn qvariant_scalars_and_lists_read_as_text() {
    let one = |ty: u32, body: &[u8]| {
        let mut b = ty.to_be_bytes().to_vec();
        b.extend_from_slice(body);
        qt_values(&variant(&b))
    };
    assert_eq!(one(1, &[1]), vec!["true"]);
    assert_eq!(one(1, &[0]), vec!["false"]);
    assert_eq!(one(2, &(-7i32).to_be_bytes()), vec!["-7"]);
    assert_eq!(one(3, &256u32.to_be_bytes()), vec!["256"]);
    assert_eq!(one(4, &(-1i64).to_be_bytes()), vec!["-1"]);
    assert_eq!(one(5, &7u64.to_be_bytes()), vec!["7"]);
    assert_eq!(one(6, &1.5f64.to_be_bytes()), vec!["1.5"]);
    assert_eq!(one(99, &[]), vec![""]);
    // A list of two ints alone is the list; beside another item it is one joined item.
    let mut l = 9u32.to_be_bytes().to_vec();
    l.extend_from_slice(&2u32.to_be_bytes());
    for v in [1u32, 2] {
        l.extend_from_slice(&2u32.to_be_bytes());
        l.extend_from_slice(&v.to_be_bytes());
    }
    assert_eq!(qt_values(&variant(&l)), vec!["1", "2"]);
    assert_eq!(qt_values(&format!("{}, x", variant(&l))), vec!["1, 2", "x"]);
}

#[test]
fn an_ini_skips_comments_and_keeps_brackets_in_values() {
    let ini = Ini::parse("; a=b\n# x=y\n\n[S]\nk=x]\n[T\nj=1\n");
    assert_eq!(ini.text("S", "k").as_deref(), Some("x]"));
    // `[T` is no section header: `j` is still in S.
    assert_eq!(ini.int("S", "j"), Some(1));
    assert!(ini.text("S", "; a").is_none() && ini.text("S", "# x").is_none());
    assert!(ini.text("", "; a").is_none() && ini.text("", "# x").is_none());
}

#[test]
fn artisan_devices_have_their_names() {
    for (id, name) in [
        (0, "Fuji PID (PXG/PXR)"),
        (1, "Omega handheld meter"),
        (3, "Center thermometer"),
        (11, "Voltcraft thermometer"),
        (17, "Omega HH309"),
        (18, "none"),
        (19, "Arduino TC4"),
        (22, "PID duty cycle"),
        (25, "virtual (a formula)"),
        (26, "Delta DTA PID"),
        (27, "a program Artisan calls"),
        (29, "Modbus"),
        (34, "Phidgets"),
        (45, "Yocto"),
        (53, "Hottop"),
        (79, "Siemens S7"),
        (83, "Aillio Bullet R1"),
        (176, "Aillio Bullet R2"),
        (101, "Behmor"),
        (111, "WebSocket"),
        (115, "HB"),
        (126, "ARC"),
        (134, "Santoker"),
        (138, "Kaleido"),
        (142, "IKAWA"),
        (164, "Mugma"),
        (196, "Orbiter"),
        (201, "MQTT"),
        (209, "SR900"),
        (999, "Artisan device 999"),
    ] {
        assert_eq!(super::machines::device_name(id), name, "device {id}");
    }
}

#[test]
fn unsupported_setups_say_why() {
    let why = |text: &str| unsupported(&Ini::parse(text)).unwrap_err();
    let ok = |text: &str| unsupported(&Ini::parse(text)).is_ok();
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=2\n").contains("serial binary"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=4\n").contains("UDP"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=9\n").contains("type 9"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=0\nbytesize=7\n").contains("needs 8"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=1\nbytesize=6\n").contains("7 or 8"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=1\nbytesize=7\n").contains("neither BT nor ET"));
    assert!(why("[Device]\nid=29\n[Modbus]\ntype=3\n").contains("neither BT nor ET"));
    assert!(ok("[Device]\nid=29\n[Modbus]\ntype=3\ninput2deviceId=1\n"));
    assert!(ok(
        "[Device]\nid=29\n[Modbus]\ntype=0\nbytesize=8\ninput1deviceId=1\n"
    ));
    assert!(why("[Device]\nid=111\n").contains("no BT or ET node"));
    assert!(
        why("[Device]\nid=112\n[WebSocket]\nchannel_nodes=bt, et\n").contains("no BT or ET node")
    );
    assert!(ok("[Device]\nid=111\n[WebSocket]\nchannel_nodes=, et\n"));
    assert!(ok("[Device]\nid=201\n[MQTT]\nchannel_nodes=data.bt\n"));
    assert!(why("[Device]\nid=201\n[MQTT]\nchannel_nodes=, \n").contains("no BT or ET field"));
    assert!(why("[Device]\nid=202\n[MQTT]\nchannel_nodes=data.bt\n").contains("no BT or ET field"));
    assert!(why("[Device]\nid=35\n").contains("not a temperature board"));
    let hottop = why("[Device]\nid=53\n");
    assert!(hottop.contains("Hottop") && hottop.contains("no transport"));
    assert!(ok("[Device]\nid=19\n") && ok("[Device]\nid=34\n"));
    // Fuji PIDs: PXG, PXR and PXF on ET; a Delta DTA on either side is not Modbus.
    assert!(ok("[Device]\nid=0\ncontrolETpid=0, 1\n"));
    assert!(ok("[Device]\nid=0\ncontrolETpid=1, 1\n"));
    assert!(ok("[Device]\nid=0\ncontrolETpid=4, 2\nreadBTpid=1, 3\n"));
    assert!(why("[Device]\nid=0\ncontrolETpid=3, 1\n").contains("ET controller is a Delta"));
    assert!(why("[Device]\nid=0\ncontrolETpid=2, 1\n").contains("names no Fuji"));
    assert!(why("[Device]\nid=0\n").contains("names no Fuji"));
    assert!(why("[Device]\nid=0\ncontrolETpid=0, 1\nreadBTpid=3, 2\n")
        .contains("BT controller is a Delta"));
    assert!(
        why("[Device]\nid=0\ncontrolETpid=0, 1\n[SerialPort]\nbytesize=7\n").contains("needs 8")
    );
}

#[test]
fn a_setup_without_a_device_is_device_minus_one() {
    let m = summary("Brand/Model_X", &Ini::parse(""));
    assert_eq!(
        (m.artisan_device, m.artisan_device_name.as_str()),
        (-1, "Artisan device -1")
    );
    assert_eq!(
        (m.brand.as_str(), m.model.as_str(), m.name.as_str()),
        ("Brand", "Model X", "Brand Model X")
    );
    assert!(!m.supported);
}
