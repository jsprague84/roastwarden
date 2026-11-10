//! The example roasters of `docs/connect-your-roaster.md`, end to end. Each test adds an
//! example's `examples/roasters/<name>/roaster.json` the way Add roaster does (the roaster, its
//! register map, its connection, active, its setup at control off), runs a device that speaks
//! what the example's sketch speaks, and expects the check's verdict ok, then heater, fan and
//! emergency stop in the device's own syntax at control manual.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::test_support::{Harness, eventually};

/// What every example device reads at room temperature, °C.
const BT: f64 = 21.5;
const ET: f64 = 22.0;

/// An example device's outputs, and every command it was sent, in its own syntax.
struct Sim {
    heater: f64,
    fan: f64,
    enabled: bool,
    heard: Vec<String>,
    /// A TC4's opening lines (UNITS, CHAN): the protocol's, not control
    opened: Vec<String>,
}

type Shared = Arc<Mutex<Sim>>;

fn sim() -> Shared {
    Arc::new(Mutex::new(Sim {
        heater: 0.0,
        fan: 0.0,
        enabled: true,
        heard: Vec::new(),
        opened: Vec::new(),
    }))
}

impl Sim {
    /// As every sketch does on emergency stop: heater off, fan full.
    fn stop(&mut self) {
        self.heater = 0.0;
        self.fan = 100.0;
    }
}

fn example(name: &str) -> Value {
    let text = match name {
        "mqtt-esp32" => include_str!("../../../examples/roasters/mqtt-esp32/roaster.json"),
        "modbus-tcp" => include_str!("../../../examples/roasters/modbus-tcp/roaster.json"),
        "tc4-serial" => include_str!("../../../examples/roasters/tc4-serial/roaster.json"),
        "websocket" => include_str!("../../../examples/roasters/websocket/roaster.json"),
        "roastwarden-starter" => {
            include_str!("../../../examples/roasters/roastwarden-starter/roaster.json")
        }
        "roastwarden-fluid-bed" => {
            include_str!("../../../examples/roasters/roastwarden-fluid-bed/roaster.json")
        }
        n => panic!("no example {n}"),
    };
    serde_json::from_str(text).unwrap()
}

/// Add the roaster as Add roaster does; its device id.
async fn add(h: &Harness, r: &Value) -> String {
    let id = r["device_id"].as_str().unwrap().to_string();
    let dev = h
        .owner(
            "POST",
            "/api/devices",
            Some(json!({ "name": r["name"], "device_id": id })),
        )
        .await;
    assert!(dev.status.is_success(), "{:?}", dev.json());
    let row = dev.json()["id"].as_str().unwrap().to_string();
    if let Some(map) = r.get("register_map") {
        let put = h
            .owner(
                "PUT",
                &format!("/api/devices/{row}/register-map"),
                Some(map.clone()),
            )
            .await;
        assert_eq!(put.status, 200, "{:?}", put.json());
    }
    let mut conn = r["connection"].clone();
    conn["enabled"] = json!(true);
    let c = h
        .owner(
            "POST",
            &format!("/api/devices/{row}/connections"),
            Some(conn),
        )
        .await;
    assert_eq!(c.status, 201, "{:?}", c.json());
    h.owner(
        "PUT",
        &format!("/api/devices/{row}"),
        Some(json!({ "status": "active" })),
    )
    .await;
    let s = h
        .owner(
            "PUT",
            &format!("/api/roaster/{id}/setup"),
            Some(json!({ "kind": r["kind"], "control": "off" })),
        )
        .await;
    assert!(s.status.is_success(), "{:?}", s.json());
    id
}

/// The check until its verdict is ok (or the last one, to show why not).
async fn check_ok(h: &Harness, id: &str) -> Value {
    let uri = format!("/api/roaster/{id}/check");
    let get = || async { h.owner("GET", &uri, None).await.json() };
    let ok = eventually(|| async { get().await["verdict"] == "ok" }).await;
    let last = get().await;
    assert!(ok, "{id}'s check: {last:#}");
    last
}

/// The common part: the check ok at control off, then manual control's commands reach the
/// device as `expect` says (fan 60, heater 40, emergency stop) and the readings echo them.
async fn the_example_passes(
    h: &Harness,
    r: &Value,
    device: &Shared,
    expect: [&[&str]; 3],
) -> Value {
    let id = r["device_id"].as_str().unwrap();
    let check = check_ok(h, id).await;
    println!("{id}: {check:#}");
    assert_eq!(check["reading"]["bt"], json!(BT), "{check}");
    assert_eq!(check["reading"]["et"], json!(ET), "{check}");
    let lines: HashMap<&str, &str> = check["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["check"].as_str().unwrap(), l["message"].as_str().unwrap()))
        .collect();
    assert!(lines["outputs"].contains("Heater, Fan"), "{lines:?}");
    assert!(lines["control"].starts_with("control off"), "{lines:?}");
    assert!(
        device.lock().unwrap().heard.is_empty(),
        "nothing is sent at control off"
    );

    let s = h
        .owner(
            "PUT",
            &format!("/api/roaster/{id}/setup"),
            Some(json!({ "kind": r["kind"], "control": "manual" })),
        )
        .await;
    assert!(s.status.is_success(), "{:?}", s.json());
    let heard = |want: &[&str]| {
        let want: Vec<String> = want.iter().map(std::string::ToString::to_string).collect();
        let device = device.clone();
        async move {
            let d = device.lock().unwrap();
            want.iter().all(|w| d.heard.contains(w))
        }
    };
    let reading = |key: &'static str, v: f64| {
        let uri = format!("/api/roaster/{id}/check");
        async move { h.owner("GET", &uri, None).await.json()["reading"][key] == json!(v) }
    };

    for (route, body, want) in [
        ("fan", json!({ "percent": 60 }), expect[0]),
        ("heater", json!({ "percent": 40 }), expect[1]),
    ] {
        let c = h
            .owner(
                "POST",
                &format!("/api/roaster/{id}/control/{route}"),
                Some(body),
            )
            .await;
        assert!(c.status.is_success(), "{route}: {:?}", c.json());
        assert!(
            eventually(|| heard(want)).await,
            "{route}: wanted {want:?}, heard {:?}",
            device.lock().unwrap().heard
        );
    }
    assert!(
        eventually(|| reading("heater", 40.0)).await,
        "the heater echoed"
    );
    assert!(eventually(|| reading("fan", 60.0)).await, "the fan echoed");

    let c = h
        .owner(
            "POST",
            &format!("/api/roaster/{id}/control/emergency_stop"),
            None,
        )
        .await;
    assert!(c.status.is_success(), "stop: {:?}", c.json());
    assert!(
        eventually(|| heard(expect[2])).await,
        "stop: wanted {:?}, heard {:?}",
        expect[2],
        device.lock().unwrap().heard
    );
    assert!(
        eventually(|| reading("heater", 0.0)).await,
        "the heater off"
    );
    let d = device.lock().unwrap();
    assert_eq!((d.heater, d.fan), (0.0, 100.0));
    check
}

/// `examples/roasters/roastwarden-starter`: the beginner's sketch (docs/esp32-starter.md). It
/// sends BT and ET in Roastwarden's own names and takes no commands, so as a logger it checks
/// ok with nothing mapped, and nothing is ever sent to it.
#[tokio::test]
async fn the_starter_example() {
    let h = Harness::new().await;
    let r = example("roastwarden-starter");
    let id = add(&h, &r).await;
    let prefix = format!("roaster/{id}");
    let mqtt = h.state.mqtt.clone();
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    mqtt.subscribe(&format!("{prefix}/control/#"), rumqttc::QoS::AtMostOnce)
        .await
        .unwrap();
    let mut rx = mqtt.events();
    mqtt.publish(
        &format!("{prefix}/status"),
        rumqttc::QoS::AtLeastOnce,
        true,
        br#"{"status":"online","version":"starter-1","caps":[]}"#.to_vec(),
    )
    .await
    .unwrap();
    let got = heard.clone();
    let p = prefix.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    // the sketch's snprintf: two decimals, nothing else
                    let body = format!("{{\"beanTemp\":{BT:.2},\"envTemp\":{ET:.2}}}");
                    let _ = mqtt.publish(&format!("{p}/telemetry"), rumqttc::QoS::AtMostOnce,
                        false, body.into_bytes()).await;
                }
                ev = rx.recv() => {
                    if let Ok(roastwarden_mqtt::MqttEvent::Publish { topic, .. }) = ev
                        && topic.starts_with(&format!("{p}/control/")) {
                            got.lock().unwrap().push(topic);
                        }
                }
            }
        }
    });

    let check = check_ok(&h, &id).await;
    assert_eq!(check["reading"]["bt"], json!(BT), "{check}");
    assert_eq!(check["reading"]["et"], json!(ET), "{check}");
    let line = |name: &str| {
        check["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["check"] == name)
            .map(|l| l["message"].as_str().unwrap().to_string())
            .unwrap()
    };
    assert!(line("outputs").contains("no outputs declared"), "{check}");
    assert!(line("caps").contains("nothing optional"), "{check}");
    assert!(line("control").starts_with("control off"), "{check}");

    // A logger is sent nothing, not even STOP
    let c = h
        .owner(
            "POST",
            &format!("/api/roaster/{id}/control/emergency_stop"),
            None,
        )
        .await;
    println!("stop on a logger: {} {:?}", c.status, c.json());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        heard.lock().unwrap().is_empty(),
        "{:?}",
        heard.lock().unwrap()
    );
}

/// `examples/roasters/mqtt-esp32`: Roastwarden's own topics and names, nothing mapped.
#[tokio::test]
async fn the_mqtt_esp32_example() {
    let h = Harness::new().await;
    let r = example("mqtt-esp32");
    let id = add(&h, &r).await;
    let device = sim();

    let mqtt = h.state.mqtt.clone();
    let prefix = format!("roaster/{id}");
    mqtt.subscribe(&format!("{prefix}/control/#"), rumqttc::QoS::AtMostOnce)
        .await
        .unwrap();
    let mut rx = mqtt.events();
    mqtt.publish(
        &format!("{prefix}/status"),
        rumqttc::QoS::AtLeastOnce,
        true,
        br#"{"status":"online","version":"example-1","caps":[]}"#.to_vec(),
    )
    .await
    .unwrap();
    let d = device.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let body = {
                        let s = d.lock().unwrap();
                        json!({ "beanTemp": BT, "envTemp": ET, "heater": s.heater,
                            "heaterApplied": if s.enabled && s.fan >= 30.0 { s.heater } else { 0.0 },
                            "fan": s.fan, "heaterEnable": s.enabled as u8 })
                    };
                    let _ = mqtt.publish(&format!("{prefix}/telemetry"), rumqttc::QoS::AtMostOnce,
                        false, body.to_string().into_bytes()).await;
                }
                ev = rx.recv() => {
                    let Ok(roastwarden_mqtt::MqttEvent::Publish { topic, payload }) = ev else { continue };
                    let Some(cmd) = topic.strip_prefix(&format!("{prefix}/control/")) else { continue };
                    let v = String::from_utf8_lossy(&payload).to_string();
                    let mut s = d.lock().unwrap();
                    match cmd {
                        "heater" => s.heater = v.parse().unwrap_or(0.0),
                        "fan" => s.fan = v.parse().unwrap_or(0.0),
                        "heater_enable" => s.enabled = v == "1",
                        "emergency_stop" => s.stop(),
                        _ => {}
                    }
                    s.heard.push(format!("{cmd} {v}"));
                }
            }
        }
    });

    let check = the_example_passes(
        &h,
        &r,
        &device,
        [&["fan 60"], &["heater 40"], &["emergency_stop 1"]],
    )
    .await;
    assert!(
        check["lines"][5]["message"]
            .as_str()
            .unwrap()
            .contains("nothing optional"),
        "caps [] said: {check}"
    );
}

/// `examples/roasters/roastwarden-fluid-bed`: the fluid-bed controller (docs/esp32-fluid-bed.md),
/// as its sketch speaks: its caps (an onboard PID, feedforward, the lease, time-proportioned
/// heat, averaged reads), the PID's state in every sample, the heater off at power-up until
/// `heater_enable`, and no heat at or below 40 % fan.
#[tokio::test]
async fn the_fluid_bed_example() {
    let h = Harness::new().await;
    let r = example("roastwarden-fluid-bed");
    let id = add(&h, &r).await;
    let device = sim();
    device.lock().unwrap().enabled = false;

    let mqtt = h.state.mqtt.clone();
    let prefix = format!("roaster/{id}");
    mqtt.subscribe(&format!("{prefix}/control/#"), rumqttc::QoS::AtMostOnce)
        .await
        .unwrap();
    let mut rx = mqtt.events();
    mqtt.publish(
        &format!("{prefix}/status"),
        rumqttc::QoS::AtLeastOnce,
        true,
        format!(
            r#"{{"status":"online","id":"{id}","version":"fluid-bed-1","caps":["pid","ff","lease","tpc","oversample"]}}"#
        )
        .into_bytes(),
    )
    .await
    .unwrap();
    let d = device.clone();
    let started = std::time::Instant::now();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let body = {
                        let s = d.lock().unwrap();
                        let applied = if s.enabled && s.fan > 40.0 { s.heater } else { 0.0 };
                        json!({ "beanTemp": BT, "envTemp": ET, "heater": s.heater,
                            "heaterApplied": applied, "fan": s.fan, "setpoint": 0.0,
                            "controlMode": 0, "heaterEnable": s.enabled as u8,
                            "Kp": 5.5, "Ki": 0.085, "Kd": 0.0,
                            "uptimeMs": started.elapsed().as_millis() as u64,
                            "stopCode": 0, "stopUptimeMs": 0 })
                    };
                    let _ = mqtt.publish(&format!("{prefix}/telemetry"), rumqttc::QoS::AtMostOnce,
                        false, body.to_string().into_bytes()).await;
                }
                ev = rx.recv() => {
                    let Ok(roastwarden_mqtt::MqttEvent::Publish { topic, payload }) = ev else { continue };
                    let Some(cmd) = topic.strip_prefix(&format!("{prefix}/control/")) else { continue };
                    let v = String::from_utf8_lossy(&payload).to_string();
                    let mut s = d.lock().unwrap();
                    match cmd {
                        "heater" => s.heater = v.parse().unwrap_or(0.0),
                        "fan" => s.fan = v.parse().unwrap_or(0.0),
                        "heater_enable" => {
                            s.enabled = v == "1" || v == "true";
                            if !s.enabled { s.heater = 0.0 }
                        }
                        "emergency_stop" => { s.stop(); s.enabled = false }
                        _ => {}
                    }
                    s.heard.push(format!("{cmd} {v}"));
                }
            }
        }
    });

    let check = the_example_passes(
        &h,
        &r,
        &device,
        [&["fan 60"], &["heater 40"], &["emergency_stop 1"]],
    )
    .await;
    let caps = check["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["check"] == "caps")
        .unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string();
    for c in ["pid", "lease", "tpc"] {
        assert!(caps.contains(c), "caps said: {caps}");
    }
}

/// The Modbus TCP example's holding registers (`modbus-tcp.ino`).
#[derive(Clone)]
struct ModbusExample(Shared);

impl ModbusExample {
    fn read(&self, r: u16) -> Option<u16> {
        let s = self.0.lock().unwrap();
        Some(match r {
            0 => (BT * 10.0).round() as i16 as u16,
            1 => (ET * 10.0).round() as i16 as u16,
            2 => s.heater as u16,
            3 => s.fan as u16,
            4 => {
                if s.enabled && s.fan >= 30.0 {
                    s.heater as u16
                } else {
                    0
                }
            }
            5 => s.enabled as u16,
            6 => 0,
            _ => return None,
        })
    }

    fn write(&self, r: u16, v: u16) -> bool {
        let mut s = self.0.lock().unwrap();
        match r {
            2 => s.heater = v.min(100) as f64,
            3 => s.fan = v.min(100) as f64,
            5 => s.enabled = v != 0,
            6 if v != 0 => s.stop(),
            6 => {}
            _ => return false,
        }
        s.heard.push(format!("{r}={v}"));
        true
    }
}

impl tokio_modbus::server::Service for ModbusExample {
    type Request = tokio_modbus::Request<'static>;
    type Response = tokio_modbus::Response;
    type Exception = tokio_modbus::ExceptionCode;
    type Future = std::future::Ready<Result<Self::Response, Self::Exception>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        use tokio_modbus::{ExceptionCode, Request, Response};
        std::future::ready(match req {
            Request::ReadHoldingRegisters(a, n) => (a..a + n)
                .map(|r| self.read(r).ok_or(ExceptionCode::IllegalDataAddress))
                .collect::<Result<Vec<u16>, _>>()
                .map(Response::ReadHoldingRegisters),
            Request::WriteSingleRegister(a, v) if self.write(a, v) => {
                Ok(Response::WriteSingleRegister(a, v))
            }
            Request::WriteMultipleRegisters(a, vs)
                if vs
                    .iter()
                    .enumerate()
                    .all(|(i, v)| self.write(a + i as u16, *v)) =>
            {
                Ok(Response::WriteMultipleRegisters(a, vs.len() as u16))
            }
            Request::WriteSingleRegister(..) | Request::WriteMultipleRegisters(..) => {
                Err(ExceptionCode::IllegalDataAddress)
            }
            _ => Err(ExceptionCode::IllegalFunction),
        })
    }
}

/// `examples/roasters/modbus-tcp`: holding registers with roles.
#[tokio::test]
async fn the_modbus_tcp_example() {
    use tokio_modbus::server::tcp::{Server, accept_tcp_connection};
    let device = sim();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let svc = ModbusExample(device.clone());
    tokio::spawn(async move {
        let on_connected = |stream, addr| {
            let svc = svc.clone();
            async move { accept_tcp_connection(stream, addr, move |_| Ok(Some(svc.clone()))) }
        };
        let _ = Server::new(listener).serve(&on_connected, |_| {}).await;
    });

    let h = Harness::new().await;
    let mut r = example("modbus-tcp");
    r["connection"]["config"]["host"] = json!("127.0.0.1");
    r["connection"]["config"]["port"] = json!(port);
    add(&h, &r).await;
    the_example_passes(&h, &r, &device, [&["3=60"], &["2=40"], &["6=1"]]).await;
}

/// A pseudo-terminal: the device's end, and the path the server opens.
#[cfg(unix)]
fn pty() -> (std::fs::File, std::fs::File, String) {
    use std::os::fd::FromRawFd;
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty fills two descriptors we own from here on; the termios is the slave's.
    unsafe {
        assert_eq!(
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null()
            ),
            0
        );
        let mut t: libc::termios = std::mem::zeroed();
        libc::tcgetattr(slave, &raw mut t);
        libc::cfmakeraw(&raw mut t);
        libc::tcsetattr(slave, libc::TCSANOW, &raw const t);
        let name = std::ffi::CStr::from_ptr(libc::ttyname(slave))
            .to_string_lossy()
            .into_owned();
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
            name,
        )
    }
}

/// `examples/roasters/tc4-serial`: stock aArtisan's lines on a serial port.
#[cfg(unix)]
#[tokio::test]
async fn the_tc4_serial_example() {
    use std::io::{BufRead, Write};
    let device = sim();
    let (master, _slave, path) = pty();
    let d = device.clone();
    std::thread::spawn(move || {
        let mut out = master.try_clone().unwrap();
        for line in std::io::BufReader::new(master).lines() {
            let Ok(line) = line else { return };
            let line = line.trim().to_uppercase();
            let reply = {
                let mut s = d.lock().unwrap();
                let pct = |l: &str| l[4..].parse::<f64>().unwrap_or(0.0).clamp(0.0, 100.0);
                match line.as_str() {
                    "READ" => format!("{:.2},{ET:.2},{BT:.2},{},{}", 21.0, s.heater, s.fan),
                    l if l.starts_with("OT1;") => {
                        s.heater = pct(l);
                        "# OT1".into()
                    }
                    l if l.starts_with("IO3;") => {
                        s.fan = pct(l);
                        "# IO3".into()
                    }
                    l if l.starts_with("CHAN;") => "# Active channels set to 1200".into(),
                    l if l.starts_with("UNITS;") => "# Units C".into(),
                    _ => "# ignored".into(),
                }
            };
            if line.starts_with("UNITS;") || line.starts_with("CHAN;") {
                d.lock().unwrap().opened.push(line);
            } else if line != "READ" {
                d.lock().unwrap().heard.push(line);
            }
            if writeln!(out, "{reply}").is_err() {
                return;
            }
        }
    });

    let h = Harness::new().await;
    let mut r = example("tc4-serial");
    r["connection"]["config"]["port"] = json!(path);
    add(&h, &r).await;
    let check = the_example_passes(
        &h,
        &r,
        &device,
        [&["IO3;60"], &["OT1;40"], &["PID;OFF", "OT1;0", "IO3;100"]],
    )
    .await;
    assert_eq!(check["reading"]["ambientTemp"], json!(21.0), "{check}");
    let opened = device.lock().unwrap().opened.clone();
    assert_eq!(opened, ["UNITS;C", "CHAN;1200"], "the opening lines");
}

/// `examples/roasters/websocket`: its own JSON, pushed; a mapping and command templates.
#[tokio::test]
async fn the_websocket_example() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let device = sim();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let d = device.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                continue;
            };
            let (mut tx, mut rx) = ws.split();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let frame = {
                            let s = d.lock().unwrap();
                            json!({ "BT": BT, "ET": ET, "heater": s.heater, "fan": s.fan })
                        };
                        if tx.send(Message::Text(frame.to_string().into())).await.is_err() { break }
                    }
                    m = rx.next() => match m {
                        Some(Ok(Message::Text(t))) => {
                            let v: Value = serde_json::from_str(&t).unwrap_or_default();
                            let n = v["value"].as_f64().unwrap_or(0.0);
                            let mut s = d.lock().unwrap();
                            match v["set"].as_str() {
                                Some("heater") => s.heater = n,
                                Some("fan") => s.fan = n,
                                Some("enable") => s.enabled = n != 0.0,
                                Some("stop") => s.stop(),
                                _ => {}
                            }
                            s.heard.push(t.to_string());
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        }
    });

    let h = Harness::new().await;
    let mut r = example("websocket");
    r["connection"]["config"]["url"] = json!(format!("ws://127.0.0.1:{port}/"));
    add(&h, &r).await;
    the_example_passes(
        &h,
        &r,
        &device,
        [
            &[r#"{"set":"fan","value":60}"#],
            &[r#"{"set":"heater","value":40}"#],
            &[r#"{"set":"stop"}"#],
        ],
    )
    .await;
}
