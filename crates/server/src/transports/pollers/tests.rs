use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio_modbus::prelude::*;

use crate::roasters::types::{
    CreateRegisterMapEntry, ModbusDataType, ModbusRegisterMap, ModbusRegisterType, Protocol,
};
use crate::transports::transport::{is_writable, role_of, ModbusWrite};
use crate::transports::types::{ModbusRtuConnectionConfig, SerialParity};

use super::modbus::*;
use super::validate::*;
use super::websocket::*;

fn blocks(regs: &[ModbusRegisterMap]) -> Vec<(u16, u16)> {
    read_blocks(&regs.iter().collect::<Vec<_>>(), MAX_REGISTER_READ)
}

#[test]
fn reads_start_at_the_first_register_not_at_zero() {
    // A PID controller's PV and SV high in its map: one read of three words at 0x1000.
    let regs = [
        reg(
            ModbusRegisterType::Holding,
            0x1000,
            "bt",
            ModbusDataType::Int16,
            None,
            false,
        ),
        reg(
            ModbusRegisterType::Holding,
            0x1001,
            "setpoint",
            ModbusDataType::Float32,
            None,
            true,
        ),
    ];
    assert_eq!(blocks(&regs), vec![(0x1000, 3)]);
    // A gap makes two reads; a long run splits at the protocol's 125.
    let regs = [
        reg(
            ModbusRegisterType::Input,
            300,
            "a",
            ModbusDataType::Uint16,
            None,
            false,
        ),
        reg(
            ModbusRegisterType::Input,
            10,
            "b",
            ModbusDataType::Uint16,
            None,
            false,
        ),
    ];
    assert_eq!(blocks(&regs), vec![(10, 1), (300, 1)]);
    let run: Vec<_> = (0..130)
        .map(|a| {
            reg(
                ModbusRegisterType::Input,
                a,
                "r",
                ModbusDataType::Uint16,
                None,
                false,
            )
        })
        .collect();
    assert_eq!(blocks(&run), vec![(0, 125), (125, 5)]);
}

#[test]
fn the_standard_map_still_reads_from_zero_in_one_block_per_kind() {
    let map = default_register_map();
    let of = |t: ModbusRegisterType| {
        let regs: Vec<&ModbusRegisterMap> = map.iter().filter(|r| r.register_type == t).collect();
        let old_span = regs
            .iter()
            .map(|r| {
                r.address
                    + if matches!(
                        r.data_type,
                        ModbusDataType::Float32 | ModbusDataType::Uint32 | ModbusDataType::Int32
                    ) {
                        2
                    } else {
                        1
                    }
            })
            .max()
            .unwrap();
        (read_blocks(&regs, MAX_REGISTER_READ), old_span as u16)
    };
    // Contiguous from 0, as the presets are: the same single read as before.
    let (input, span) = of(ModbusRegisterType::Input);
    assert_eq!(input, vec![(0, span)]);
}

#[test]
fn values_decode_at_their_own_address() {
    let regs = [
        reg(
            ModbusRegisterType::Holding,
            0x1000,
            "pv",
            ModbusDataType::Int16,
            None,
            false,
        ),
        reg(
            ModbusRegisterType::Holding,
            0x1001,
            "sv",
            ModbusDataType::Float32,
            None,
            true,
        ),
    ];
    let refs: Vec<&ModbusRegisterMap> = regs.iter().collect();
    let bits = 201.5f32.to_bits();
    let words: HashMap<usize, u16> = (0x1000usize..)
        .zip([(-12i16) as u16, (bits >> 16) as u16, bits as u16])
        .collect();
    let mut payload = serde_json::Map::new();
    decode_registers(&refs, &words, &mut payload);
    assert_eq!(payload["pv"], json!(-12));
    assert_eq!(payload["sv"], json!(201.5));
}

/// BCD as a Loring's PLC sends it (Artisan's `bcd`, `BCDsAsInt`): BT 0x0402 0x0000 in CDAB
/// is 4020 × 0.1 °F; a bcd16 is four digits; a digit above 9 is no value. A write goes back
/// out as the same digits.
#[test]
fn bcd_registers_read_and_write_their_digits() {
    let mut bt = scaled(
        ModbusRegisterType::Holding,
        768,
        "BT",
        ModbusDataType::Bcd32,
        0.1,
        false,
    );
    bt.byte_order = Some("CDAB".into());
    let n = reg(
        ModbusRegisterType::Holding,
        778,
        "n",
        ModbusDataType::Bcd16,
        None,
        false,
    );
    let bad = reg(
        ModbusRegisterType::Holding,
        779,
        "bad",
        ModbusDataType::Bcd16,
        None,
        false,
    );
    let words: HashMap<usize, u16> =
        [(768, 0x4020), (769, 0x0000), (778, 0x1234), (779, 0x00A1)].into();
    let mut payload = serde_json::Map::new();
    decode_registers(&[&bt, &n, &bad], &words, &mut payload);
    assert_eq!(payload["BT"], json!(402.0));
    assert_eq!(payload["n"], json!(1234));
    assert!(!payload.contains_key("bad"));
    assert_eq!(from_bcd(0x99_999_999), Some(99_999_999));

    let mut sv = bt.clone();
    sv.writable = true;
    assert_eq!(
        crate::transports::transport::encode_register(&sv, 402.0),
        Ok(ModbusWrite::Holding {
            address: 768,
            words: vec![0x4020, 0x0000]
        })
    );
    assert_eq!(
        crate::transports::transport::encode_register(&n, 1234.0),
        Ok(ModbusWrite::Holding {
            address: 778,
            words: vec![0x1234]
        })
    );
    assert!(crate::transports::transport::encode_register(&n, 10_000.0).is_err());
}

fn scaled(
    register_type: ModbusRegisterType,
    address: i32,
    name: &str,
    data_type: ModbusDataType,
    scale: f64,
    writable: bool,
) -> ModbusRegisterMap {
    let mut r = reg(register_type, address, name, data_type, None, writable);
    r.scale_factor = Some(scale);
    r
}

#[test]
fn the_read_plan_covers_coils_and_discrete_inputs_in_bit_runs() {
    use ModbusDataType::*;
    use ModbusRegisterType::*;
    let map = [
        reg(Discrete, 40, "door", Bool, None, false),
        reg(Coil, 3, "e_stop", Bool, None, true),
        reg(Coil, 1, "heater_on", Bool, None, true),
        reg(Coil, 0, "fan_on", Bool, None, true),
        reg(Holding, 0x1001, "sv", Int16, None, true),
        reg(Input, 2, "et", Float32, None, false),
        reg(Input, 0, "bt", Float32, None, false),
    ];
    assert_eq!(
        plan_reads(&map),
        vec![
            ReadRun {
                unit: None,
                kind: Input,
                start: 0,
                count: 4
            },
            ReadRun {
                unit: None,
                kind: Holding,
                start: 0x1001,
                count: 1
            },
            // Coils 0 and 1 together; 3 after the gap at 2 on its own.
            ReadRun {
                unit: None,
                kind: Coil,
                start: 0,
                count: 2
            },
            ReadRun {
                unit: None,
                kind: Coil,
                start: 3,
                count: 1
            },
            ReadRun {
                unit: None,
                kind: Discrete,
                start: 40,
                count: 1
            },
        ]
    );
    // Bits split at 2000 per read, registers at 125.
    let coils: Vec<_> = (0..2100)
        .map(|a| reg(Coil, a, "c", Bool, None, false))
        .collect();
    let plan = plan_reads(&coils);
    assert_eq!(
        plan.iter().map(|r| (r.start, r.count)).collect::<Vec<_>>(),
        vec![(0, 2000), (2000, 100)]
    );
    // A coil marked float32 by mistake is still one bit.
    let odd = [reg(Coil, 7, "x", Float32, None, false)];
    assert_eq!(plan_reads(&odd)[0].count, 1);
}

// ---- Modbus RTU: the master over a serial line, with a device that answers frames ----

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn with_crc(mut body: Vec<u8>) -> Vec<u8> {
    let c = crc16(&body);
    body.extend_from_slice(&c.to_le_bytes());
    body
}

/// A PID controller on an RS-485 line, slave `unit`: holding registers in `regs`,
/// function codes 3 (read) and 6 (write one); an address it does not have is exception 2.
/// Every request it sees is recorded.
async fn rtu_controller(
    line: tokio::io::DuplexStream,
    unit: u8,
    regs: Arc<Mutex<HashMap<u16, u16>>>,
    seen: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    rtu_bus(line, HashMap::from([(unit, regs)]), seen).await;
}

/// Several controllers on one RS-485 line, each answering at its own slave address.
async fn rtu_bus(
    mut line: tokio::io::DuplexStream,
    slaves: HashMap<u8, Arc<Mutex<HashMap<u16, u16>>>>,
    seen: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        let mut req = [0u8; 8];
        if line.read_exact(&mut req).await.is_err() {
            return;
        }
        assert_eq!(
            crc16(&req[..6]).to_le_bytes(),
            [req[6], req[7]],
            "the master's CRC"
        );
        seen.lock().unwrap().push(req.to_vec());
        let unit = req[0];
        let Some(regs) = slaves.get(&unit) else {
            continue; // no such slave on the line: silence
        };
        let addr = u16::from_be_bytes([req[2], req[3]]);
        let value = u16::from_be_bytes([req[4], req[5]]);
        let reply = match req[1] {
            3 => {
                let regs = regs.lock().unwrap();
                let words: Option<Vec<u16>> = (addr..addr + value)
                    .map(|a| regs.get(&a).copied())
                    .collect();
                match words {
                    Some(words) => {
                        let mut body = vec![unit, 3, (words.len() * 2) as u8];
                        for w in words {
                            body.extend_from_slice(&w.to_be_bytes());
                        }
                        with_crc(body)
                    }
                    None => with_crc(vec![unit, 0x83, 2]),
                }
            }
            6 => {
                regs.lock().unwrap().insert(addr, value);
                req.to_vec()
            }
            f => with_crc(vec![unit, f | 0x80, 1]),
        };
        line.write_all(&reply).await.unwrap();
    }
}

#[tokio::test]
async fn a_bus_of_controllers_is_read_and_written_at_each_ones_slave() {
    use ModbusDataType::*;
    use ModbusRegisterType::*;
    // An Artisan-style bus (docs/artisan-machines.md): BT on slave 1, ET on slave 2, the
    // burner's output on slave 3; the connection's own unit is 1.
    let (master, device) = tokio::io::duplex(512);
    let bt = Arc::new(Mutex::new(HashMap::from([(0x1000, 2004)])));
    let et = Arc::new(Mutex::new(HashMap::from([(0x1000, 2310)])));
    let burner = Arc::new(Mutex::new(HashMap::from([(0x0102, 0)])));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let slaves = HashMap::from([(1, bt.clone()), (2, et.clone()), (3, burner.clone())]);
    tokio::spawn(rtu_bus(device, slaves, seen.clone()));
    let mut ctx = tokio_modbus::client::rtu::attach_slave(master, Slave(1));

    let on = |mut r: ModbusRegisterMap, unit: Option<i64>, role: &str| {
        r.unit_id = unit;
        r.role = Some(role.into());
        r
    };
    let map = [
        on(scaled(Holding, 0x1000, "BT", Int16, 0.1, false), None, "bt"),
        on(
            scaled(Holding, 0x1000, "ET", Int16, 0.1, false),
            Some(2),
            "et",
        ),
        on(
            reg(Holding, 0x0102, "Burner", Uint16, None, true),
            Some(3),
            "heater",
        ),
    ];
    let plan = plan_reads(&map);
    assert_eq!(
        plan.iter().map(|r| r.unit).collect::<Vec<_>>(),
        vec![None, Some(2), Some(3)],
        "one read per slave, the connection's first"
    );
    let poll = poll_modbus_registers(&mut ctx, &plan, &map, Duration::from_secs(1), 1)
        .await
        .expect("the poll");
    assert_eq!(poll.payload["beanTemp"], json!(200.4));
    assert_eq!(poll.payload["envTemp"], json!(231.0));
    assert_eq!(poll.payload["heater"], json!(0));
    assert!(poll.refused.is_empty(), "{:?}", poll.refused);
    let firsts: Vec<u8> = seen.lock().unwrap().iter().map(|r| r[0]).collect();
    assert_eq!(firsts, vec![1, 2, 3]);
    assert_eq!(poll.raw[1]["unit_id"], json!(2));
    assert!(poll.raw[0].get("unit_id").is_none());

    // The heater goes to slave 3's register; the link is back on slave 1 after it.
    let writes = crate::transports::transport::plan_unit_writes(
        "modbus_rtu",
        &map,
        &roastwarden_core::DeviceCommand::Heater { percent: 40.0 },
    )
    .unwrap();
    assert_eq!(writes.len(), 1);
    let (unit, w) = &writes[0];
    assert_eq!(*unit, Some(3));
    write_on(&mut ctx, *unit, 1, w, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(burner.lock().unwrap()[&0x0102], 40);
    assert_eq!(bt.lock().unwrap().get(&0x0102), None);
    let poll = poll_modbus_registers(&mut ctx, &plan, &map, Duration::from_secs(1), 1)
        .await
        .expect("the poll after the write");
    assert_eq!(poll.payload["heater"], json!(40));
}

#[tokio::test]
async fn rtu_polls_and_writes_a_pid_controller_at_its_own_addresses() {
    let (master, device) = tokio::io::duplex(512);
    // PV 200.4 °C and SV 185.0 °C, ×10, at 0x1000 and 0x1001; nothing at 0.
    let regs = Arc::new(Mutex::new(HashMap::from([(0x1000, 2004), (0x1001, 1850)])));
    let seen = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(rtu_controller(device, 7, regs.clone(), seen.clone()));

    let mut ctx = tokio_modbus::client::rtu::attach_slave(master, Slave(7));
    let map = [
        scaled(
            ModbusRegisterType::Holding,
            0x1000,
            "BT",
            ModbusDataType::Int16,
            0.1,
            false,
        ),
        scaled(
            ModbusRegisterType::Holding,
            0x1001,
            "SV",
            ModbusDataType::Int16,
            0.1,
            true,
        ),
    ];
    let plan = plan_reads(&map);
    let poll = poll_modbus_registers(&mut ctx, &plan, &map, Duration::from_secs(1), 7)
        .await
        .expect("the poll");
    assert_eq!(poll.payload["beanTemp"], json!(200.4));
    assert_eq!(poll.payload["setpoint"], json!(185.0));
    assert_eq!(poll.refused, [] as [std::string::String; 0]);
    // One read, from 0x1000, addressed to slave 7.
    assert_eq!(
        seen.lock().unwrap()[0],
        with_crc(vec![7, 3, 0x10, 0x00, 0x00, 0x02])
    );

    // A setpoint goes to SV, the only writable one, as 1900.
    let writes = crate::transports::transport::plan_modbus_writes(
        "modbus_rtu",
        &map,
        &roastwarden_core::DeviceCommand::Setpoint { value: 190.0 },
    )
    .unwrap();
    for w in &writes {
        execute_modbus_write(&mut ctx, w, Duration::from_secs(1))
            .await
            .unwrap();
    }
    assert_eq!(regs.lock().unwrap()[&0x1001], 1900);
    // The read-only PV is never written.
    assert_eq!(regs.lock().unwrap()[&0x1000], 2004);
}

#[tokio::test]
async fn a_refused_read_drops_its_registers_and_a_silent_device_fails_the_poll() {
    let (master, device) = tokio::io::duplex(512);
    let regs = Arc::new(Mutex::new(HashMap::from([(0x1000, 2004)])));
    let seen = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(rtu_controller(device, 7, regs, seen));
    let mut ctx = tokio_modbus::client::rtu::attach_slave(master, Slave(7));
    // 0x1000 answers; 0x2000 (after a gap) is refused with an exception.
    let map = [
        reg(
            ModbusRegisterType::Holding,
            0x1000,
            "pv",
            ModbusDataType::Int16,
            None,
            false,
        ),
        reg(
            ModbusRegisterType::Holding,
            0x2000,
            "alarm",
            ModbusDataType::Uint16,
            None,
            false,
        ),
    ];
    let plan = plan_reads(&map);
    let poll = poll_modbus_registers(&mut ctx, &plan, &map, Duration::from_secs(1), 7)
        .await
        .unwrap();
    assert_eq!(poll.payload["pv"], json!(2004));
    assert!(poll.payload.get("alarm").is_none());
    assert_eq!(poll.refused.len(), 1, "{:?}", poll.refused);
    // Only refused reads: the poll fails (and the poller reopens the link).
    let only = [map[1].clone()];
    assert!(poll_modbus_registers(
        &mut ctx,
        &plan_reads(&only),
        &only,
        Duration::from_secs(1),
        7
    )
    .await
    .is_err());

    // Another unit id on the line gets no answer: the poll times out instead of hanging.
    let (master, device) = tokio::io::duplex(512);
    let regs = Arc::new(Mutex::new(HashMap::from([(0x1000, 1)])));
    tokio::spawn(rtu_controller(device, 9, regs, Arc::default()));
    let mut ctx = tokio_modbus::client::rtu::attach_slave(master, Slave(7));
    let one = [map[0].clone()];
    let err = poll_modbus_registers(
        &mut ctx,
        &plan_reads(&one),
        &one,
        Duration::from_millis(200),
        7,
    )
    .await
    .err()
    .expect("a timeout");
    assert!(err.to_string().contains("no answer"), "{err}");
}

#[test]
fn rtu_config_reads_with_defaults_and_frames_the_port() {
    let c: ModbusRtuConnectionConfig =
        serde_json::from_value(json!({ "port": "/dev/ttyUSB0" })).unwrap();
    assert_eq!(
        (c.baud, c.parity, c.data_bits, c.stop_bits, c.unit_id),
        (9600, SerialParity::None, 8, 1, 1)
    );
    assert_eq!((c.poll_interval_ms, c.timeout_ms), (1000, 1000));
    let c: ModbusRtuConnectionConfig = serde_json::from_value(json!({
        "port": "/dev/ttyUSB1", "baud": 19200, "parity": "even", "stop_bits": 1,
        "unit_id": 3, "poll_interval_ms": 500
    }))
    .unwrap();
    let link = ModbusLink::Rtu(c.clone());
    assert_eq!(link.name(), "modbus_rtu");
    assert_eq!(link.describe(), "/dev/ttyUSB1 19200 8E1 unit 3");
    assert_eq!(link.poll_interval(), Duration::from_millis(500));
    assert!(rtu_port_builder(&c).is_ok());
    let bad = ModbusRtuConnectionConfig { stop_bits: 3, ..c };
    assert!(rtu_port_builder(&bad).is_err());
    assert!(serde_json::from_value::<ModbusRtuConnectionConfig>(
        json!({ "port": "/dev/ttyUSB0", "parity": "mark" })
    )
    .is_err());
}

#[test]
fn connection_configs_are_checked_before_they_are_saved() {
    let ok = |p: Protocol, c: serde_json::Value| check_connection_config(&p, &c);
    assert!(ok(Protocol::ModbusRtu, json!({ "port": "/dev/ttyUSB0" })).is_ok());
    assert!(ok(
        Protocol::ModbusTcp,
        json!({ "host": "pid.local", "port": 502, "unit_id": 1 })
    )
    .is_ok());
    assert!(ok(Protocol::Mqtt, json!({ "topic_prefix": "roaster/x" })).is_ok());
    assert!(ok(Protocol::WebSocket, json!({ "url": "ws://10.0.0.5/ws" })).is_ok());
    assert!(ok(Protocol::SerialTc4, json!({ "port": "/dev/ttyACM0" })).is_ok());

    let err = |p: Protocol, c: serde_json::Value| check_connection_config(&p, &c).unwrap_err();
    assert!(err(Protocol::ModbusRtu, json!({ "port": "" })).contains("serial port"));
    assert!(err(
        Protocol::ModbusRtu,
        json!({ "port": "/dev/ttyUSB0", "unit_id": 0 })
    )
    .contains("1 to 247"));
    assert!(err(
        Protocol::ModbusRtu,
        json!({ "port": "/dev/ttyUSB0", "data_bits": 7 })
    )
    .contains("8 data bits"));
    assert!(err(
        Protocol::ModbusRtu,
        json!({ "port": "/dev/ttyUSB0", "parity": "mark" })
    )
    .contains("Modbus RTU config"));
    assert!(err(Protocol::ModbusTcp, json!({ "host": " " })).contains("host"));
    assert!(err(
        Protocol::ModbusTcp,
        json!({ "host": "10.0.0.2", "poll_interval_ms": 10 })
    )
    .contains("poll_interval_ms"));
    assert!(err(Protocol::WebSocket, json!({ "url": "http://x" })).contains("ws://"));
    assert!(err(
        Protocol::SerialTc4,
        json!({ "port": "/dev/ttyACM0", "et_channel": 2, "bt_channel": 2 })
    )
    .contains("one channel only"));
    assert!(err(
        Protocol::SerialTc4,
        json!({ "port": "/dev/ttyACM0", "t3_channel": 5 })
    )
    .contains("T3 channel is 5"));
    assert!(err(Protocol::Mqtt, json!("roaster/x")).contains("JSON object"));
}

fn entry(
    register_type: ModbusRegisterType,
    address: i32,
    name: &str,
    data_type: ModbusDataType,
    writable: bool,
) -> CreateRegisterMapEntry {
    CreateRegisterMapEntry {
        register_type,
        address,
        name: name.into(),
        data_type,
        byte_order: None,
        scale_factor: None,
        offset: None,
        unit: None,
        description: None,
        writable: Some(writable),
        role: None,
        unit_id: None,
    }
}

#[test]
fn register_maps_are_checked_before_they_are_saved() {
    use ModbusDataType::*;
    use ModbusRegisterType::*;
    // The standard layout passes as it is.
    let standard: Vec<CreateRegisterMapEntry> = default_register_map()
        .into_iter()
        .map(|r| {
            let mut e = entry(r.register_type, r.address, &r.name, r.data_type, r.writable);
            e.byte_order = Some("ABCD".into());
            e.scale_factor = Some(1.0);
            e
        })
        .collect();
    assert_eq!(check_register_map(&standard), Ok(()));
    // The dashboard's blank row (byte order "AB", empty unit) too.
    let mut blank = entry(Input, 0, "bt", Uint16, false);
    blank.byte_order = Some("AB".into());
    assert_eq!(check_register_map(&[blank]), Ok(()));

    let err = |map: &[CreateRegisterMapEntry]| check_register_map(map).unwrap_err();
    assert_eq!(
        err(&[entry(Input, 0, " ", Uint16, false)]),
        "register 1: it needs a name"
    );
    assert!(err(&[
        entry(Input, 0, "BT", Int16, false),
        entry(Holding, 9, "bt", Int16, false)
    ])
    .starts_with("register 2 ('bt'): register 1 has the same name"));
    assert!(err(&[entry(Coil, 0, "e_stop", Uint16, true)]).contains("data type is bool"));
    assert!(err(&[entry(Input, 3, "sv", Float32, true)]).contains("read-only"));
    assert!(err(&[entry(Discrete, 3, "door", Bool, true)]).contains("read-only"));
    assert!(err(&[entry(Holding, 65535, "sv", Float32, true)]).contains("takes two"));
    assert!(err(&[entry(Holding, -1, "sv", Int16, true)]).contains("outside 0 to 65535"));
    let mut odd = entry(Holding, 0, "sv", Float32, true);
    odd.byte_order = Some("ACBD".into());
    assert!(err(&[odd]).contains("byte order 'ACBD'"));
    let mut zero = entry(Holding, 0, "sv", Int16, true);
    zero.scale_factor = Some(0.0);
    assert!(err(&[zero]).contains("scale factor 0"));
}

fn roled(mut e: CreateRegisterMapEntry, role: &str) -> CreateRegisterMapEntry {
    e.role = Some(role.into());
    e
}

/// docs/ingest-mapping.md, "Modbus TCP and RTU": one readable and one writable register
/// per role; emergency stop written only; an unknown role named.
#[test]
fn register_roles_are_checked() {
    use ModbusDataType::*;
    use ModbusRegisterType::*;
    let ok = [
        roled(entry(Input, 0, "PV", Int16, false), "bt"),
        roled(entry(Input, 1, "OUT", Uint16, false), "heater"),
        roled(entry(Holding, 9, "MV", Uint16, true), "heater"),
        roled(entry(Coil, 0, "STOP", Bool, true), "emergency_stop"),
        entry(Holding, 10, "notes", Uint16, false),
    ];
    assert_eq!(check_register_map(&ok), Ok(()));
    let err = |map: &[CreateRegisterMapEntry]| check_register_map(map).unwrap_err();
    assert!(err(&[
        roled(entry(Input, 0, "PV", Int16, false), "bt"),
        roled(entry(Input, 1, "PV2", Int16, false), "bt"),
    ])
    .contains("register 1 already has the role bt as a readable register"));
    assert!(err(&[
        roled(entry(Holding, 0, "A", Int16, true), "fan"),
        roled(entry(Coil, 1, "B", Bool, true), "fan"),
    ])
    .contains("writable"));
    assert!(
        err(&[roled(entry(Coil, 0, "STOP", Bool, false), "emergency_stop")])
            .contains("written, never read")
    );
    assert!(err(&[roled(entry(Input, 0, "PV", Int16, false), "bean")])
        .contains("role 'bean' is not one of"));
    // Empty is no role.
    assert_eq!(
        check_register_map(&[roled(entry(Input, 0, "PV", Int16, false), " ")]),
        Ok(())
    );
}

/// A register with a role is its role's field over any name; °F converted; a role's
/// read-only register is its reading, its writable one only the command's.
#[test]
fn registers_with_roles_decode_as_their_field() {
    use ModbusDataType::*;
    use ModbusRegisterType::*;
    let mut pv = reg(Holding, 0x1000, "PV", Int16, Some("°F"), false);
    pv.role = Some("bt".into());
    pv.scale_factor = Some(0.1);
    let mut out = reg(Input, 3, "OUT1", Uint16, Some("%"), false);
    out.role = Some("heater".into());
    let mut mv = reg(Holding, 0x1002, "MV", Uint16, Some("%"), true);
    mv.role = Some("heater".into());
    // A register named BT with no role would be beanTemp by name: the role wins.
    let named = reg(Input, 4, "BT", Uint16, None, false);
    let regs = [pv, out, mv, named];
    let words: HashMap<(ModbusRegisterType, usize), u16> = HashMap::from([
        ((Holding, 0x1000), 4010),
        ((Input, 3), 35),
        ((Holding, 0x1002), 80),
        ((Input, 4), 999),
    ]);
    let mut payload = serde_json::Map::new();
    let data_of = |kind| -> HashMap<usize, u16> {
        words
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|((_, a), w)| (*a, *w))
            .collect()
    };
    for kind in KINDS {
        let plain: Vec<&ModbusRegisterMap> = regs
            .iter()
            .filter(|r| r.register_type == kind && role_of(r).is_none())
            .collect();
        decode_registers(&plain, &data_of(kind), &mut payload);
    }
    normalize_field_names(&mut payload);
    let mut roled: Vec<&ModbusRegisterMap> = regs.iter().filter(|r| role_of(r).is_some()).collect();
    roled.sort_by_key(|r| !is_writable(r));
    for r in roled {
        decode_registers(&[r], &data_of(r.register_type), &mut payload);
    }
    assert_eq!(payload["beanTemp"], json!(205.0), "401.0 °F");
    assert_eq!(
        payload["heater"],
        json!(35),
        "the reading, not the command register"
    );
    assert!(payload.get("PV").is_none() && payload.get("MV").is_none());
}

#[test]
fn mapped_connection_configs_are_checked() {
    let ok = |p: Protocol, c: serde_json::Value| check_connection_config(&p, &c);
    assert!(ok(
        Protocol::Mqtt,
        json!({ "telemetry_topic": "tele/roaster/SENSOR",
                "ingest": { "units": "F", "fields": { "bt": "MAX31855.Temperature" } },
                "commands": { "heater": { "topic": "cmnd/roaster/Dimmer" } } })
    )
    .is_ok());
    assert!(ok(
            Protocol::WebSocket,
            json!({ "url": "ws://10.0.0.9/ws", "ingest": { "fields": { "bt": "data.BT" } },
                "poll": { "message": "{\"command\":\"getData\",\"id\":{id}}", "interval_ms": 1000 } })
        )
        .is_ok());
    assert!(ok(
        Protocol::SerialTc4,
        json!({ "port": "/dev/ttyUSB0", "ingest": { "units": "F" },
                "commands": { "drum": { "payload": "OT2;{value}" } } })
    )
    .is_ok());
    let err = |p: Protocol, c: serde_json::Value| check_connection_config(&p, &c).unwrap_err();
    assert!(err(
        Protocol::Mqtt,
        json!({ "telemetry_topic": "tele/+/SENSOR" })
    )
    .contains("wildcard"));
    assert!(err(
        Protocol::Mqtt,
        json!({ "ingest": { "fields": { "fan": "f" } } })
    )
    .contains("bt or et"));
    assert!(err(
        Protocol::WebSocket,
        json!({ "url": "ws://x", "poll": { "message": "x", "interval_ms": 50 } })
    )
    .contains("at least 200"));
    assert!(err(
        Protocol::ModbusTcp,
        json!({ "host": "10.0.0.2", "ingest": { "units": "F" } })
    )
    .contains("role in the register map"));
    assert!(err(
        Protocol::SerialTc4,
        json!({ "port": "/dev/ttyUSB0", "commands": { "drum": { "topic": "a" } } })
    )
    .contains("MQTT"));
    assert_eq!(
        poll_message("{\"command\":\"getData\",\"id\":{id}}", 7),
        "{\"command\":\"getData\",\"id\":7}"
    );
}

/// docs/ingest-mapping.md, "Tests": a device with its own telemetry topic and °F mapping
/// records °C samples; a heater command goes to its template's topic with its payload;
/// another device cannot claim the topic; the default ESP32 path is unchanged.
#[tokio::test]
async fn an_mqtt_device_with_its_own_topics_and_fields() {
    use crate::test_support::{eventually, Harness};
    let h = Harness::new().await;
    let dev = h
        .owner(
            "POST",
            "/api/devices",
            Some(json!({ "name": "Tasmota", "device_id": "tasmota_01" })),
        )
        .await
        .json();
    let id = dev["id"].as_str().unwrap().to_string();
    let r = h
        .owner(
            "POST",
            &format!("/api/devices/{id}/connections"),
            Some(json!({ "protocol": "mqtt", "config": {
                    "topic_prefix": "cmnd/roaster",
                    "telemetry_topic": "tele/roaster/SENSOR",
                    "ingest": { "units": "F", "fields": {
                        "bt": "MAX31855.Temperature", "et": "DS18B20.Temperature",
                        "fan": "Fan" } },
                    "commands": { "heater": { "topic": "cmnd/roaster/Dimmer", "decimals": 0 } }
                } })),
        )
        .await;
    assert_eq!(r.status, 201, "{:?}", r.json());

    h.state
            .mqtt
            .publish(
                "tele/roaster/SENSOR",
                rumqttc::QoS::AtMostOnce,
                false,
                br#"{"MAX31855":{"Temperature":392.0},"DS18B20":{"Temperature":"437"},"Fan":55,"Time":"x"}"#.to_vec(),
            )
            .await
            .unwrap();
    let cached = || async {
        h.state
            .telemetry_cache
            .read()
            .await
            .get("tasmota_01")
            .map(|(v, _)| v.clone())
    };
    assert!(eventually(|| async { cached().await.is_some() }).await);
    let v = cached().await.unwrap();
    assert_eq!(v["beanTemp"], json!(200.0));
    assert_eq!(v["envTemp"], json!(225.0));
    assert_eq!(v["fan"], json!(55.0));
    assert!(v.get("Time").is_none() && v.get("MAX31855").is_none());

    // A heater command: the template's topic and payload (control manual first).
    h.owner(
        "PUT",
        "/api/roaster/tasmota_01/setup",
        Some(json!({ "kind": "drum_gas", "control": "manual" })),
    )
    .await;
    h.state
        .mqtt
        .subscribe("cmnd/#", rumqttc::QoS::AtMostOnce)
        .await
        .unwrap();
    let mut rx = h.state.mqtt.events();
    let r = h
        .owner(
            "POST",
            "/api/roaster/tasmota_01/control/heater",
            Some(json!({ "percent": 41.6 })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let got = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(roastwarden_mqtt::MqttEvent::Publish { topic, payload }) = rx.recv().await
                && topic.starts_with("cmnd/") {
                    return (topic, String::from_utf8(payload).unwrap());
                }
        }
    })
    .await
    .unwrap();
    assert_eq!(got, ("cmnd/roaster/Dimmer".to_string(), "42".to_string()));

    // Another device cannot claim the topic, nor this roaster's own roaster/<id>/….
    let other = h
        .owner(
            "POST",
            "/api/devices",
            Some(json!({ "name": "Other", "device_id": "other_01" })),
        )
        .await
        .json();
    let oid = other["id"].as_str().unwrap();
    for cfg in [
        json!({ "telemetry_topic": "tele/roaster/SENSOR" }),
        json!({ "telemetry_topic": "roaster/tasmota_01/telemetry" }),
    ] {
        let r = h
            .owner(
                "POST",
                &format!("/api/devices/{oid}/connections"),
                Some(json!({ "protocol": "mqtt", "config": cfg })),
            )
            .await;
        assert_eq!(r.status, 400, "{:?}", r.json());
        assert!(r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tasmota_01"));
    }

    // The ESP32-style default device still goes the way it always has.
    h.state
        .mqtt
        .publish(
            "roaster/esp32_x/telemetry",
            rumqttc::QoS::AtMostOnce,
            false,
            br#"{"beanTemp":150.5,"envTemp":180.0,"heater":40,"fan":70}"#.to_vec(),
        )
        .await
        .unwrap();
    assert!(
        eventually(|| async {
            h.state
                .telemetry_cache
                .read()
                .await
                .get("esp32_x")
                .is_some_and(|(v, _)| v["beanTemp"] == json!(150.5))
        })
        .await
    );

    // The mapping removed: the topic is no longer read.
    let conn_id = h
        .owner("GET", &format!("/api/devices/{id}"), None)
        .await
        .json()["connections"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    h.owner(
        "DELETE",
        &format!("/api/devices/{id}/connections/{conn_id}"),
        None,
    )
    .await;
    assert!(h.state.mqtt_routes.for_device("tasmota_01").is_none());
}

// ---- Pollers follow the database: start, restart, stop with no server restart ----

/// A Modbus TCP PID controller: holding registers from a shared table.
#[derive(Clone)]
struct FakeController {
    regs: Arc<Mutex<HashMap<u16, u16>>>,
}

impl tokio_modbus::server::Service for FakeController {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = std::future::Ready<Result<Response, ExceptionCode>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let regs = self.regs.lock().unwrap();
        std::future::ready(match req {
            Request::ReadHoldingRegisters(a, n) => (a..a + n)
                .map(|x| {
                    regs.get(&x)
                        .copied()
                        .ok_or(ExceptionCode::IllegalDataAddress)
                })
                .collect::<Result<Vec<u16>, _>>()
                .map(Response::ReadHoldingRegisters),
            _ => Err(ExceptionCode::IllegalFunction),
        })
    }
}

async fn fake_controller(regs: Arc<Mutex<HashMap<u16, u16>>>) -> u16 {
    use tokio_modbus::server::tcp::{accept_tcp_connection, Server};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let server = Server::new(listener);
        let on_connected = |stream, addr| {
            let svc = FakeController { regs: regs.clone() };
            async move { accept_tcp_connection(stream, addr, move |_| Ok(Some(svc.clone()))) }
        };
        let _ = server.serve(&on_connected, |_| {}).await;
    });
    port
}

#[tokio::test]
async fn pollers_start_restart_and_stop_as_connections_and_maps_change() {
    use crate::test_support::{eventually, Harness};
    let h = Harness::new().await;
    let regs = Arc::new(Mutex::new(HashMap::from([(0x1000, 2004), (0x1001, 1850)])));
    let port = fake_controller(regs).await;

    let dev = h
        .owner(
            "POST",
            "/api/devices",
            Some(json!({ "name": "PID", "device_id": "pid_01" })),
        )
        .await
        .json();
    let id = dev["id"].as_str().unwrap().to_string();
    let bt = || async {
        h.state
            .telemetry_cache
            .read()
            .await
            .get("pid_01")
            .and_then(|(v, _)| v.get("beanTemp").and_then(serde_json::Value::as_f64))
    };
    let live = || async { h.state.transports.live_transports("pid_01").await };

    // A bad map is refused with what is wrong, and nothing is saved.
    let bad = h
        .owner(
            "PUT",
            &format!("/api/devices/{id}/register-map"),
            Some(
                json!([{ "register_type": "input", "address": 0, "name": "BT",
                              "data_type": "int16", "writable": true }]),
            ),
        )
        .await;
    assert_eq!(bad.status, 400);
    assert!(bad.json()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("read-only"));
    let map = |scale: f64| {
        json!([
            { "register_type": "holding", "address": 4096, "name": "BT",
              "data_type": "int16", "scale_factor": scale, "writable": false },
            { "register_type": "holding", "address": 4097, "name": "SV",
              "data_type": "int16", "scale_factor": 0.1, "writable": true }
        ])
    };
    let r = h
        .owner(
            "PUT",
            &format!("/api/devices/{id}/register-map"),
            Some(map(0.1)),
        )
        .await;
    assert_eq!(r.status, 200);

    // A bad connection is refused too.
    let r = h
        .owner(
            "POST",
            &format!("/api/devices/{id}/connections"),
            Some(json!({ "protocol": "modbus_tcp", "config": { "host": "" } })),
        )
        .await;
    assert_eq!(r.status, 400);
    let conn = h
        .owner(
            "POST",
            &format!("/api/devices/{id}/connections"),
            Some(json!({ "protocol": "modbus_tcp", "config": {
                    "host": "127.0.0.1", "port": port, "unit_id": 1, "poll_interval_ms": 100 } })),
        )
        .await;
    assert_eq!(conn.status, 201);
    let conn_id = conn.json()["id"].as_str().unwrap().to_string();
    // The device is still pending: no poller yet.
    assert_eq!(
        h.state.pollers.running().await,
        [] as [std::string::String; 0]
    );

    // Activating it starts the poller, with no restart: PV read from 0x1000.
    h.owner(
        "PUT",
        &format!("/api/devices/{id}"),
        Some(json!({ "status": "active" })),
    )
    .await;
    assert_eq!(h.state.pollers.running().await, vec![conn_id.clone()]);
    assert!(eventually(|| async { bt().await == Some(200.4) }).await);
    assert!(eventually(|| async { live().await == vec!["modbus_tcp"] }).await);

    // A new map restarts it: BT unscaled now.
    h.owner(
        "PUT",
        &format!("/api/devices/{id}/register-map"),
        Some(map(1.0)),
    )
    .await;
    assert!(eventually(|| async { bt().await == Some(2004.0) }).await);

    // Disabled: stopped, its command sink gone at once.
    h.owner(
        "PUT",
        &format!("/api/devices/{id}/connections/{conn_id}"),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(
        h.state.pollers.running().await,
        [] as [std::string::String; 0]
    );
    assert_eq!(live().await, [] as [&str; 0]);
    // Enabled again, then deleted.
    h.owner(
        "PUT",
        &format!("/api/devices/{id}/connections/{conn_id}"),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert!(eventually(|| async { live().await == vec!["modbus_tcp"] }).await);
    // A config change that does not parse is refused and the poller keeps running.
    let r = h
        .owner(
            "PUT",
            &format!("/api/devices/{id}/connections/{conn_id}"),
            Some(json!({ "config": { "host": "127.0.0.1", "port": 0 } })),
        )
        .await;
    assert_eq!(r.status, 400);
    assert_eq!(h.state.pollers.running().await, vec![conn_id.clone()]);
    let r = h
        .owner(
            "DELETE",
            &format!("/api/devices/{id}/connections/{conn_id}"),
            None,
        )
        .await;
    assert_eq!(r.status, 204);
    assert_eq!(
        h.state.pollers.running().await,
        [] as [std::string::String; 0]
    );
    assert_eq!(live().await, [] as [&str; 0]);
}
