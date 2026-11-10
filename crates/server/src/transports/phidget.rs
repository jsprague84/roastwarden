//! Phidgets temperature boards through the Phidget22 network server (docs/phidgets.md).
//!
//! The board (a 1048, a TMP1101 on a VINT hub, a TMP1100, a 1051, a TMP1200) is plugged into
//! a computer running Phidgets' network server; Roastwarden is a network client of it, as
//! Artisan is with "remote" on. No Phidgets library is linked: the protocol is the one
//! Phidgets' own JavaScript client speaks (`phidget22` on npm, ISC), read from it and from
//! libphidget22's server (BSD):
//!
//! - every message is a 16-byte header (magic `0x50484930`, the body's length, flags, a
//!   request and a reply number, a type and a subtype, little-endian) and a JSON body;
//! - the client says hello (protocol 2.4), then proves the password:
//!   `base64(sha256("phidgetclient" + password + nonceC + nonceS + salt))`;
//! - the server then announces each device and, to a client whose type starts with `www`,
//!   each channel; the client opens a channel by its ids and sets it with bridge packets
//!   (data interval, thermocouple type); the server sends each reading as a bridge packet
//!   event, and a keepalive the client must answer.
//!
//! Readings are named by the board's temperature input, `ch0` to `ch7` (Phidgets' channel
//! number in the `TemperatureSensor` class), in °C. Each sample is the mean of the readings
//! since the last one, through the connection's ingest mapping (default BT `ch0`, ET `ch1`).
//! A Phidget connection reads only: it registers no command sink.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::transports::ingest::{Ingest, Kind};
use crate::transports::types::{Phidget22ConnectionConfig, ThermocoupleType};

// ---------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------

pub(crate) const MAGIC: u32 = 0x5048_4930;
pub(crate) const HEADER_LEN: usize = 16;
/// libphidget22's limit on one message's body.
const MAX_BODY: usize = 128 * 1024;

pub(crate) const MSG_CONNECT: u8 = 10;
pub(crate) const MSG_COMMAND: u8 = 20;
pub(crate) const MSG_DEVICE: u8 = 30;

pub(crate) const SUB_HANDSHAKE_C0: u8 = 10;
pub(crate) const SUB_HANDSHAKE_S0: u8 = 11;
pub(crate) const SUB_AUTH_C0: u8 = 30;
pub(crate) const SUB_AUTH_S0: u8 = 31;
pub(crate) const SUB_AUTH_C1: u8 = 32;
#[allow(dead_code)] // the mock server answers with it
pub(crate) const SUB_REPLY: u8 = 40;
pub(crate) const SUB_KEEPALIVE: u8 = 41;
pub(crate) const SUB_ATTACH: u8 = 50;
pub(crate) const SUB_DETACH: u8 = 55;
pub(crate) const SUB_OPEN: u8 = 60;
pub(crate) const SUB_BRIDGE: u8 = 70;
pub(crate) const SUB_CHANNEL: u8 = 80;

pub(crate) const FLAG_REPLY: u16 = 0x0002;

const PROTO_MAJOR: u32 = 2;
const PROTO_MINOR: u32 = 4;
const IDENT: &str = "phidgetclient";
/// A type starting with `www` makes the server announce each channel (libphidget22's
/// `PNCF_SENDCHANNELS`), as it does for Phidgets' JavaScript client.
const CLIENT_TYPE: &str = "www,roastwarden";

/// Phidget22's channel class of a temperature sensor.
pub(crate) const CLASS_TEMPERATURE: i64 = 28;

/// Bridge packets a temperature channel sends and takes.
#[allow(dead_code)] // the mock server answers an open with it
pub(crate) const BP_SET_STATUS: i64 = 0;
pub(crate) const BP_ERROR_EVENT: i64 = 17;
pub(crate) const BP_CHANGE_TRIGGER: i64 = 46;
pub(crate) const BP_DATA_INTERVAL: i64 = 54;
pub(crate) const BP_RTD_TYPE: i64 = 76;
pub(crate) const BP_RTD_WIRES: i64 = 77;
pub(crate) const BP_THERMOCOUPLE_TYPE: i64 = 84;
pub(crate) const BP_TEMPERATURE_CHANGE: i64 = 95;

/// A message's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub len: u32,
    pub flags: u16,
    pub reqseq: u16,
    pub repseq: u16,
    pub kind: u8,
    pub sub: u8,
}

impl Header {
    pub(crate) fn encode(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&self.len.to_le_bytes());
        b[8..10].copy_from_slice(&self.flags.to_le_bytes());
        b[10..12].copy_from_slice(&self.reqseq.to_le_bytes());
        b[12..14].copy_from_slice(&self.repseq.to_le_bytes());
        b[14] = self.kind;
        b[15] = self.sub;
        b
    }

    pub(crate) fn decode(b: &[u8; HEADER_LEN]) -> Result<Self, String> {
        let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if magic != MAGIC {
            return Err(format!(
                "not a Phidget22 server (the message starts {magic:#010x}, not {MAGIC:#010x})"
            ));
        }
        Ok(Header {
            len: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            flags: u16::from_le_bytes([b[8], b[9]]),
            reqseq: u16::from_le_bytes([b[10], b[11]]),
            repseq: u16::from_le_bytes([b[12], b[13]]),
            kind: b[14],
            sub: b[15],
        })
    }
}

/// One message: its header and its body (Null when empty).
#[derive(Debug, Clone)]
pub(crate) struct Message {
    pub header: Header,
    pub body: Value,
}

/// libphidget22 prints a double that is not a number as `nan` (or `-nan(…)`), which is not
/// JSON: such a value becomes `null`. Text inside strings is left alone.
pub(crate) fn clean_nan(text: &str) -> String {
    let b = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            in_str = c != b'"';
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        let start = i + usize::from(c == b'-' || c == b'+');
        if b.len() >= start + 3 && b[start..start + 3].eq_ignore_ascii_case(b"nan") {
            let mut end = start + 3;
            if end < b.len() && b[end] == b'(' {
                while end < b.len() && b[end] != b')' {
                    end += 1;
                }
                end = (end + 1).min(b.len());
            }
            out.extend_from_slice(b"null");
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    // Only ASCII was replaced, so the bytes are still UTF-8.
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

pub(crate) async fn read_message(r: &mut OwnedReadHalf) -> std::io::Result<Message> {
    let mut h = [0u8; HEADER_LEN];
    r.read_exact(&mut h).await?;
    let header =
        Header::decode(&h).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = header.len as usize;
    if len > MAX_BODY {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("a {len}-byte message (at most {MAX_BODY})"),
        ));
    }
    let mut data = vec![0u8; len];
    r.read_exact(&mut data).await?;
    let body = if data.is_empty() {
        Value::Null
    } else {
        let text = String::from_utf8_lossy(&data);
        // A body that is not JSON is kept as text (a reply's message, say).
        serde_json::from_str(&clean_nan(text.trim_end_matches('\0')))
            .unwrap_or_else(|_| Value::String(text.into_owned()))
    };
    Ok(Message { header, body })
}

pub(crate) async fn write_message(
    w: &mut OwnedWriteHalf,
    mut header: Header,
    body: &str,
) -> std::io::Result<()> {
    header.len = body.len() as u32;
    let mut buf = Vec::with_capacity(HEADER_LEN + body.len());
    buf.extend_from_slice(&header.encode());
    buf.extend_from_slice(body.as_bytes());
    w.write_all(&buf).await
}

/// The handshake's proof of the password.
pub(crate) fn proof(password: &str, nonce_c: &str, nonce_s: &str, salt: &str) -> String {
    let digest = Sha256::digest(format!("{IDENT}{password}{nonce_c}{nonce_s}{salt}").as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// A nonce as Phidgets' client makes one: 8 base64 characters (libphidget22 keeps 15).
fn nonce() -> String {
    let mut bytes = [0u8; 6];
    let _ = getrandom::fill(&mut bytes);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Phidget22's error codes worth a sentence (libphidget22's `PhidgetReturnCode`); the rest by
/// number.
fn error_text(code: i64) -> String {
    match code {
        2 => "no such device on the server (Phidget22 error 2, not found)".into(),
        7 => "the password is wrong (Phidget22 error 7, access denied)".into(),
        9 => "the channel is open elsewhere (Phidget22 error 9, busy)".into(),
        20 => "the board does not take that setting (Phidget22 error 20, unsupported)".into(),
        21 => "a value out of the board's range (Phidget22 error 21, invalid argument)".into(),
        52 => "the channel is not attached (Phidget22 error 52)".into(),
        58 => "a keepalive was missed (Phidget22 error 58)".into(),
        c => format!("Phidget22 error {c}"),
    }
}

/// The server's keepalive comes every 4 s (its default); a server silent this long is gone
/// (Phidgets' own client waits 8 s).
const SILENT: Duration = Duration::from_secs(12);

// ---------------------------------------------------------------------------
// A link to the server
// ---------------------------------------------------------------------------

/// What can go wrong on a link, said for the person setting it up.
#[derive(Debug)]
pub(crate) struct LinkError(pub String);

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, LinkError> {
    Err(LinkError(m.into()))
}

/// The writing side, shared by the link and its reader (which answers keepalives).
struct Writer {
    w: OwnedWriteHalf,
    seq: u16,
}

impl Writer {
    fn next_seq(&mut self) -> u16 {
        self.seq = if self.seq == u16::MAX {
            11
        } else {
            self.seq + 1
        };
        self.seq
    }
}

/// A connected, authenticated link. Messages are read by a task of their own, which answers
/// the server's keepalives at once whatever the link's owner is doing (the server drops a
/// client that misses one) and ends the link when the server goes silent.
pub(crate) struct Link {
    rx: mpsc::Receiver<std::io::Result<Message>>,
    w: std::sync::Arc<tokio::sync::Mutex<Writer>>,
    reader: JoinHandle<()>,
    /// Messages read while waiting for a reply, for [`Link::recv`].
    pending: VecDeque<Message>,
    pub server: String,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Link {
    /// Connect and log in (5 s for each step).
    pub(crate) async fn connect(host: &str, port: u16, password: &str) -> Result<Link, LinkError> {
        let server = format!("{host}:{port}");
        let stream =
            match tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((host, port)))
                .await
            {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => return err(format!("could not reach {server}: {e}")),
                Err(_) => return err(format!("{server} did not answer in 5 seconds")),
            };
        let _ = stream.set_nodelay(true);
        let (mut r, w) = stream.into_split();
        let w = std::sync::Arc::new(tokio::sync::Mutex::new(Writer { w, seq: 10 }));
        let (tx, rx) = mpsc::channel(256);
        let keepalive_w = w.clone();
        let reader = tokio::spawn(async move {
            loop {
                let m = match tokio::time::timeout(SILENT, read_message(&mut r)).await {
                    Ok(m) => m,
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("silent for {} s (no keepalive)", SILENT.as_secs()),
                    )),
                };
                if let Ok(k) = &m
                    && k.header.kind == MSG_COMMAND && k.header.sub == SUB_KEEPALIVE {
                        let mut w = keepalive_w.lock().await;
                        let h = Header {
                            len: 0,
                            flags: FLAG_REPLY,
                            reqseq: w.next_seq(),
                            repseq: k.header.reqseq,
                            kind: MSG_COMMAND,
                            sub: SUB_KEEPALIVE,
                        };
                        if write_message(&mut w.w, h, "").await.is_err() {
                            let _ = tx.send(m).await;
                            return;
                        }
                        continue;
                    }
                let end = m.is_err();
                if tx.send(m).await.is_err() || end {
                    return;
                }
            }
        });
        let mut link = Link {
            rx,
            w,
            reader,
            pending: VecDeque::new(),
            server,
        };
        link.handshake(password).await?;
        Ok(link)
    }

    /// Write a message; its request number (a fresh one when `reqseq` is 0 and it is a
    /// request).
    async fn write(
        &mut self,
        flags: u16,
        numbered: bool,
        kind: u8,
        sub: u8,
        body: &str,
    ) -> Result<u16, LinkError> {
        let mut w = self.w.lock().await;
        let reqseq = if numbered { w.next_seq() } else { 0 };
        let h = Header {
            len: 0,
            flags,
            reqseq,
            repseq: 0,
            kind,
            sub,
        };
        write_message(&mut w.w, h, body)
            .await
            .map_err(|e| LinkError(format!("{}: write failed: {e}", self.server)))?;
        Ok(reqseq)
    }

    /// The next message from the wire, within `wait`.
    async fn read(&mut self, wait: Duration) -> Result<Option<Message>, LinkError> {
        match tokio::time::timeout(wait, self.rx.recv()).await {
            Err(_) => Ok(None),
            Ok(None) => err(format!("{} closed the connection", self.server)),
            Ok(Some(Err(e))) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                err(format!("{} closed the connection", self.server))
            }
            Ok(Some(Err(e))) => err(format!("{}: {e}", self.server)),
            Ok(Some(Ok(m))) => Ok(Some(m)),
        }
    }

    /// A handshake step's answer: the next message, which must be `kind`/`sub`.
    async fn expect(&mut self, kind: u8, sub: u8, step: &str) -> Result<Value, LinkError> {
        match self.read(Duration::from_secs(5)).await? {
            None => err(format!("{}: no answer to the {step}", self.server)),
            Some(m) if m.header.kind == kind && m.header.sub == sub => Ok(m.body),
            Some(m) => err(format!(
                "{}: the {step} was answered with message {}/{}, not {kind}/{sub}",
                self.server, m.header.kind, m.header.sub
            )),
        }
    }

    async fn handshake(&mut self, password: &str) -> Result<(), LinkError> {
        let hello = json!({ "type": CLIENT_TYPE, "pmajor": PROTO_MAJOR, "pminor": PROTO_MINOR });
        self.write(0, false, MSG_CONNECT, SUB_HANDSHAKE_C0, &hello.to_string())
            .await?;
        let s0 = self
            .expect(MSG_CONNECT, SUB_HANDSHAKE_S0, "handshake")
            .await?;
        if s0["result"].as_i64() != Some(0) {
            return err(format!(
                "{} refused the handshake ({}; it speaks protocol {}.{}, Roastwarden {PROTO_MAJOR}.{PROTO_MINOR})",
                self.server,
                error_text(s0["result"].as_i64().unwrap_or(-1)),
                s0["pmajor"],
                s0["pminor"]
            ));
        }
        let nonce_c = nonce();
        let c0 = json!({ "ident": IDENT, "nonceC": nonce_c });
        self.write(0, false, MSG_CONNECT, SUB_AUTH_C0, &c0.to_string())
            .await?;
        let s1 = self.expect(MSG_CONNECT, SUB_AUTH_S0, "login").await?;
        if s1["result"].as_i64() != Some(0) {
            return err(format!(
                "{} refused the login: {}",
                self.server,
                error_text(s1["result"].as_i64().unwrap_or(-1))
            ));
        }
        if s1["nonceC"].as_str() != Some(nonce_c.as_str()) {
            return err(format!(
                "{}: the login's nonce came back changed",
                self.server
            ));
        }
        let nonce_s = s1["nonceS"].as_str().unwrap_or_default().to_string();
        let salt = s1["salt"].as_str().unwrap_or_default().to_string();
        let c1 = json!({
            "nonceC": nonce_c,
            "nonceS": nonce_s,
            "proof": proof(password, &nonce_c, &nonce_s, &salt),
        });
        self.write(0, false, MSG_CONNECT, SUB_AUTH_C1, &c1.to_string())
            .await?;
        // The server's verdict is a simple reply (COMMAND/REPLY, `{E}`).
        let verdict = match self.read(Duration::from_secs(5)).await? {
            Some(m) => m.body,
            None => return err(format!("{}: no answer to the password", self.server)),
        };
        match verdict["E"].as_i64() {
            Some(0) => Ok(()),
            Some(c) if password.is_empty() && c == 7 => err(format!(
                "{} wants a password (Phidget22 error 7): set the connection's password",
                self.server
            )),
            Some(c) => err(format!(
                "{} refused the login: {}",
                self.server,
                error_text(c)
            )),
            None => err(format!(
                "{}: the login's answer has no result: {verdict}",
                self.server
            )),
        }
    }

    /// Send a request and wait for its reply (5 s), keeping what else arrives meanwhile.
    async fn request(&mut self, kind: u8, sub: u8, body: &Value) -> Result<Value, LinkError> {
        let seq = self.write(0, true, kind, sub, &body.to_string()).await?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(m) = self.read(left).await? else {
                return err(format!("{}: no reply in 5 seconds", self.server));
            };
            if m.header.flags & FLAG_REPLY != 0 && m.header.repseq == seq {
                return Ok(m.body);
            }
            self.pending.push_back(m);
        }
    }

    /// The next message (keepalives are the reader's), within `wait`.
    pub(crate) async fn recv(&mut self, wait: Duration) -> Result<Option<Message>, LinkError> {
        if let Some(m) = self.pending.pop_front() {
            return Ok(Some(m));
        }
        self.read(wait).await
    }

    /// The devices and temperature inputs the server announces after the login: everything
    /// that arrives until it has been quiet a moment.
    pub(crate) async fn discover(&mut self) -> Result<Vec<Board>, LinkError> {
        let mut boards = Boards::default();
        let mut wait = Duration::from_millis(1500);
        while let Some(m) = self.recv(wait).await? {
            boards.take(&m);
            wait = Duration::from_millis(300);
        }
        Ok(boards.list())
    }

    /// Open a temperature input; the reply is the channel's state (its limits among it).
    pub(crate) async fn open(&mut self, board: &Board, input: &Input) -> Result<Value, LinkError> {
        let body = json!({
            "phid": board.phid,
            "channel": input.chid,
            "class": CLASS_TEMPERATURE,
            "index": input.unique_index,
            "version": input.version,
            "chreset": 1,
        });
        let reply = self.request(MSG_DEVICE, SUB_OPEN, &body).await?;
        if let Some(code) = reply["E"].as_i64().filter(|c| *c != 0) {
            return err(format!(
                "{} would not open {} ch{}: {}{}",
                self.server,
                board.label(),
                input.index,
                error_text(code),
                reply["R"]
                    .as_str()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            ));
        }
        Ok(reply)
    }

    /// Set one of an open input's properties (a bridge packet with one entry).
    pub(crate) async fn set(
        &mut self,
        board: &Board,
        input: &Input,
        packet: i64,
        entry_type: &str,
        value: Value,
    ) -> Result<(), LinkError> {
        let body = json!({
            "v": 0,
            "s": 2,
            "f": 0,
            "p": packet,
            "I": board.phid,
            "X": input.unique_index,
            "c": 1,
            "e": { "0": { "t": entry_type, "v": value } },
        });
        let reply = self.request(MSG_DEVICE, SUB_BRIDGE, &body).await?;
        match reply["E"].as_i64() {
            None | Some(0) => Ok(()),
            Some(c) => err(format!(
                "ch{}: {}{}",
                input.index,
                error_text(c),
                reply["R"]
                    .as_str()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Boards and their inputs, as the server announces them
// ---------------------------------------------------------------------------

/// A device with temperature inputs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Board {
    pub phid: u64,
    pub serial: i64,
    pub hub_port: i64,
    /// `USB`, `VINT`, …
    pub kind: String,
    /// The part number (`1048`, `TMP1101`) when the name says it, else the name.
    pub name: String,
    pub inputs: Vec<Input>,
}

/// One temperature input (a channel of class `TemperatureSensor`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Input {
    pub chid: u64,
    /// The channel's place among the device's channels (what opening it takes).
    pub unique_index: i64,
    /// Phidgets' channel number in its class: `ch<index>`.
    pub index: i64,
    pub version: i64,
    /// `Thermocouple Input`, `Temperature Sensor (IC)`, `RTD input`
    pub name: String,
}

impl Input {
    pub(crate) fn is_thermocouple(&self) -> bool {
        self.name.to_ascii_lowercase().contains("thermocouple")
    }
    pub(crate) fn is_rtd(&self) -> bool {
        self.name.to_ascii_lowercase().contains("rtd")
    }
    /// What the input is, in a few words.
    pub(crate) fn what(&self) -> &'static str {
        if self.is_thermocouple() {
            "thermocouple"
        } else if self.is_rtd() {
            "RTD"
        } else if self.name.contains("IC") {
            "the board's own sensor"
        } else {
            "temperature"
        }
    }
}

impl Board {
    pub(crate) fn label(&self) -> String {
        if self.kind == "VINT" {
            format!("{} #{} port {}", self.name, self.serial, self.hub_port)
        } else {
            format!("{} #{}", self.name, self.serial)
        }
    }

    pub(crate) fn input(&self, index: u8) -> Option<&Input> {
        self.inputs.iter().find(|i| i.index == i64::from(index))
    }

    /// The board's inputs in one line: `ch0–ch3 thermocouple, ch4 the board's own sensor`.
    pub(crate) fn describe(&self) -> String {
        let mut groups: Vec<(&'static str, Vec<i64>)> = Vec::new();
        for i in &self.inputs {
            match groups.iter_mut().find(|(w, _)| *w == i.what()) {
                Some((_, v)) => v.push(i.index),
                None => groups.push((i.what(), vec![i.index])),
            }
        }
        groups
            .iter()
            .map(|(what, idx)| {
                let span = match (idx.first(), idx.last()) {
                    (Some(a), Some(b)) if a != b => format!("ch{a}–ch{b}"),
                    (Some(a), _) => format!("ch{a}"),
                    _ => String::new(),
                };
                format!("{span} {what}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The part number in a device's name: `PhidgetTemperatureSensor 4-Input` is a 1048 by its
/// id, `4x Thermocouple Phidget` a TMP1101.
fn part_of(device_id: i64, name: &str) -> String {
    let known = match device_id {
        26 => Some("1048"),
        28 => Some("1051"),
        88 => Some("TMP1100"),
        89 => Some("TMP1101"),
        90 => Some("TMP1200"),
        _ => None,
    };
    known.map_or_else(|| name.to_string(), str::to_string)
}

#[derive(Default)]
struct Boards {
    devices: BTreeMap<u64, Board>,
}

fn id_of(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

impl Boards {
    /// Take in an announcement (a device or a channel); anything else is ignored.
    fn take(&mut self, m: &Message) {
        if m.header.kind != MSG_DEVICE {
            return;
        }
        let b = &m.body;
        match m.header.sub {
            SUB_ATTACH => {
                let Some(phid) = id_of(&b["phid"]) else {
                    return;
                };
                let name = b["name"].as_str().unwrap_or("Phidget").to_string();
                let device_id = b["deviceID"].as_i64().unwrap_or(-1);
                self.devices.insert(
                    phid,
                    Board {
                        phid,
                        serial: b["serialNumber"].as_i64().unwrap_or(-1),
                        hub_port: b["hubPort"].as_i64().unwrap_or(0),
                        kind: b["type"].as_str().unwrap_or("").to_string(),
                        name: part_of(device_id, &name),
                        inputs: Vec::new(),
                    },
                );
            }
            SUB_DETACH => {
                if let Some(phid) = id_of(&b["phid"]) {
                    self.devices.remove(&phid);
                }
            }
            SUB_CHANNEL => {
                if b["class"].as_i64() != Some(CLASS_TEMPERATURE) {
                    return;
                }
                let (Some(parent), Some(chid)) = (id_of(&b["parent"]), id_of(&b["chid"])) else {
                    return;
                };
                if let Some(dev) = self.devices.get_mut(&parent) {
                    dev.inputs.push(Input {
                        chid,
                        unique_index: b["uniqueIndex"].as_i64().unwrap_or(0),
                        index: b["index"].as_i64().unwrap_or(0),
                        version: b["version"].as_i64().unwrap_or(0),
                        name: b["name"].as_str().unwrap_or("").to_string(),
                    });
                    dev.inputs.sort_by_key(|i| i.index);
                }
            }
            _ => {}
        }
    }

    /// The devices that have temperature inputs, by serial number and port.
    fn list(self) -> Vec<Board> {
        let mut v: Vec<Board> = self
            .devices
            .into_values()
            .filter(|d| !d.inputs.is_empty())
            .collect();
        v.sort_by_key(|d| (d.serial, d.hub_port));
        v
    }
}

// ---------------------------------------------------------------------------
// What a connection reads
// ---------------------------------------------------------------------------

/// A connection's setup, read once from its config.
pub(crate) struct Setup {
    pub config: Phidget22ConnectionConfig,
    pub ingest: Ingest,
    /// The inputs the mapping reads, `ch<n>`.
    pub inputs: Vec<u8>,
}

/// The mapping when a connection has none: BT `ch0`, ET `ch1` (Artisan's 1048 and TMP1101).
pub(crate) fn default_ingest() -> Value {
    json!({ "fields": { "bt": "ch0", "et": "ch1" } })
}

impl Setup {
    pub(crate) fn from_config(config: &Value) -> Result<Setup, String> {
        let c: Phidget22ConnectionConfig =
            serde_json::from_value(config.clone()).map_err(|e| format!("Phidget22 config: {e}"))?;
        let spec = c
            .ingest
            .clone()
            .filter(|v| !v.is_null())
            .unwrap_or_else(default_ingest);
        let ingest = crate::transports::ingest::parse_ingest(&spec, Kind::Phidget)?;
        let mut inputs: Vec<u8> = ingest
            .fields
            .iter()
            .filter_map(|f| crate::transports::ingest::phidget_input(&f.path_text))
            .collect();
        inputs.sort_unstable();
        inputs.dedup();
        Ok(Setup {
            config: c,
            ingest,
            inputs,
        })
    }

    /// The thermocouple type of input `n`.
    pub(crate) fn thermocouple(&self, n: i64) -> ThermocoupleType {
        self.config
            .thermocouple_types
            .as_ref()
            .and_then(|m| m.get(&format!("ch{n}")).copied())
            .unwrap_or(self.config.thermocouple_type)
    }

    /// The board this connection reads, of those the server has.
    pub(crate) fn choose<'a>(&self, boards: &'a [Board]) -> Result<&'a Board, String> {
        let c = &self.config;
        let fits: Vec<&Board> = boards
            .iter()
            .filter(|b| c.serial_number.is_none_or(|s| i64::from(s) == b.serial))
            .filter(|b| c.hub_port.is_none_or(|p| i64::from(p) == b.hub_port))
            .collect();
        let missing = |b: &Board| -> Vec<String> {
            self.inputs
                .iter()
                .filter(|n| b.input(**n).is_none())
                .map(|n| format!("ch{n}"))
                .collect()
        };
        if let Some(b) = fits.iter().find(|b| missing(b).is_empty()) {
            return Ok(b);
        }
        let all = if boards.is_empty() {
            "the server has no temperature board attached (is the board plugged in, and \
             shown in the Phidget Control Panel there?)"
                .to_string()
        } else {
            format!(
                "it has {}",
                boards
                    .iter()
                    .map(|b| format!("{} ({})", b.label(), b.describe()))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        match fits.first() {
            None if c.serial_number.is_some() || c.hub_port.is_some() => Err(format!(
                "no temperature board{}{} on the server: {all}",
                c.serial_number
                    .map(|s| format!(" with serial number {s}"))
                    .unwrap_or_default(),
                c.hub_port
                    .map(|p| format!(" on hub port {p}"))
                    .unwrap_or_default(),
            )),
            None => Err(all),
            Some(b) => Err(format!(
                "{} has no {} ({})",
                b.label(),
                missing(b).join(", "),
                b.describe()
            )),
        }
    }
}

/// The open inputs' readings since the last sample.
#[derive(Default)]
pub(crate) struct Readings {
    /// chid → input number.
    by_chid: HashMap<u64, i64>,
    since: HashMap<i64, (f64, u32)>,
    last: HashMap<i64, (f64, Instant)>,
    /// An input's newest error event (an open thermocouple), until a reading comes.
    pub errors: HashMap<i64, String>,
}

impl Readings {
    pub(crate) fn watch(&mut self, input: &Input) {
        self.by_chid.insert(input.chid, input.index);
    }

    /// A bridge packet from the server: a reading or an error event of an open input.
    pub(crate) fn take(&mut self, m: &Message, now: Instant) {
        if m.header.kind != MSG_DEVICE || m.header.sub != SUB_BRIDGE {
            return;
        }
        let b = &m.body;
        let Some(n) = id_of(&b["O"]).and_then(|c| self.by_chid.get(&c).copied()) else {
            return;
        };
        match b["p"].as_i64() {
            Some(BP_TEMPERATURE_CHANGE) => match b["e"]["0"]["v"].as_f64() {
                Some(v) if v.is_finite() && v.abs() < 1.0e6 => {
                    let e = self.since.entry(n).or_insert((0.0, 0));
                    e.0 += v;
                    e.1 += 1;
                    self.last.insert(n, (v, now));
                    self.errors.remove(&n);
                }
                _ => {
                    self.last.remove(&n);
                }
            },
            Some(BP_ERROR_EVENT) => {
                // The board says the reading is not good (a thermocouple open, out of range):
                // the input has no value until the next reading.
                self.since.remove(&n);
                self.last.remove(&n);
                let text = b["e"]["1"]["v"].as_str().unwrap_or("an error event");
                self.errors.insert(n, text.to_string());
            }
            _ => {}
        }
    }

    /// The sample's raw values, `{"ch0": …}` in °C: each input's mean since the last sample,
    /// else its newest reading when no older than `stale`.
    pub(crate) fn sample(&mut self, now: Instant, stale: Duration) -> Map<String, Value> {
        let mut out = Map::new();
        let mut inputs: Vec<i64> = self.by_chid.values().copied().collect();
        inputs.sort_unstable();
        for n in inputs {
            let v = match self.since.remove(&n) {
                Some((sum, count)) if count > 0 => Some(sum / f64::from(count)),
                _ => self
                    .last
                    .get(&n)
                    .filter(|(_, at)| now.duration_since(*at) <= stale)
                    .map(|(v, _)| *v),
            };
            if let Some(v) = v {
                out.insert(format!("ch{n}"), json!((v * 1000.0).round() / 1000.0));
            }
        }
        out
    }
}

/// Connect, find the board, open its mapped inputs and set them; the link, the board and
/// what could not be set.
pub(crate) async fn start(setup: &Setup) -> Result<(Link, Board, Vec<String>), LinkError> {
    let c = &setup.config;
    let mut link = Link::connect(&c.host, c.port, c.password.as_deref().unwrap_or("")).await?;
    let boards = link.discover().await?;
    let board = setup
        .choose(&boards)
        .map_err(|e| LinkError(format!("{}: {e}", link.server)))?
        .clone();
    let warnings = open_inputs(&mut link, setup, &board).await?;
    Ok((link, board, warnings))
}

/// Open the mapped inputs of `board` and set each (data interval, change trigger,
/// thermocouple or RTD type); a setting the board refuses is a warning, not a failure.
async fn open_inputs(
    link: &mut Link,
    setup: &Setup,
    board: &Board,
) -> Result<Vec<String>, LinkError> {
    let c = &setup.config;
    let mut warnings = Vec::new();
    for n in &setup.inputs {
        let Some(input) = board.input(*n).cloned() else {
            continue;
        };
        let status = link.open(board, &input).await?;
        // Each input's own limits (a TMP1101's own sensor takes 300 ms at least): the asked
        // interval within them.
        let limit = |k: &str| {
            status["e"][k]["v"]
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0 && *v < 4.0e9)
                .map(|v| v as u32)
        };
        let mut interval = c.data_interval_ms;
        if let Some(lo) = limit("minDataInterval").filter(|lo| interval < *lo) {
            warnings.push(format!(
                "ch{}: reports every {lo} ms, its shortest (asked {interval})",
                input.index
            ));
            interval = lo;
        }
        if let Some(hi) = limit("maxDataInterval").filter(|hi| interval > *hi) {
            interval = hi;
        }
        let mut sets: Vec<(i64, &str, Value)> = vec![
            (BP_DATA_INTERVAL, "u", json!(interval)),
            (BP_CHANGE_TRIGGER, "g", json!(0.0)),
        ];
        if input.is_thermocouple() {
            sets.push((
                BP_THERMOCOUPLE_TYPE,
                "d",
                json!(setup.thermocouple(input.index).code()),
            ));
        }
        if input.is_rtd() {
            sets.push((BP_RTD_TYPE, "d", json!(c.rtd_type.code())));
            // Phidget22's wire setups: 1 two wires, 2 three, 3 four.
            sets.push((
                BP_RTD_WIRES,
                "d",
                json!(i64::from(c.rtd_wires.clamp(2, 4)) - 1),
            ));
        }
        for (packet, t, v) in sets {
            if let Err(e) = link.set(board, &input, packet, t, v).await {
                warnings.push(e.0);
            }
        }
    }
    Ok(warnings)
}

/// How old a reading may be in a sample.
pub(crate) fn stale_after(config: &Phidget22ConnectionConfig) -> Duration {
    Duration::from_millis(
        u64::from(config.data_interval_ms)
            .saturating_mul(3)
            .max(3000),
    )
}

// ---------------------------------------------------------------------------
// Trying a connection before it is saved
// ---------------------------------------------------------------------------

/// What a try of the settings found: the boards, the one read and its first readings.
pub(crate) struct Probe {
    pub boards: Vec<Board>,
    pub chosen: Option<Board>,
    pub readings: Map<String, Value>,
    pub problem: Option<String>,
    pub warnings: Vec<String>,
}

impl Probe {
    pub(crate) fn summary(&self, server: &str) -> String {
        let mut s = format!("Phidget22 server at {server}: ");
        s += &if self.boards.is_empty() {
            "no temperature board attached".to_string()
        } else {
            self.boards
                .iter()
                .map(|b| format!("{} ({})", b.label(), b.describe()))
                .collect::<Vec<_>>()
                .join("; ")
        };
        if let Some(b) = &self.chosen {
            let vals: Vec<String> = self
                .readings
                .iter()
                .map(|(k, v)| format!("{k} {} °C", v.as_f64().unwrap_or(f64::NAN)))
                .collect();
            let _ = write!(
                s,
                ". Reading {}: {}",
                b.label(),
                if vals.is_empty() {
                    "no reading yet".to_string()
                } else {
                    vals.join(", ")
                }
            );
        }
        if let Some(p) = &self.problem {
            let _ = write!(s, ". {p}");
        }
        for w in &self.warnings {
            let _ = write!(s, ". Not set: {w}");
        }
        s
    }
}

/// Log in, list the boards, open the mapped inputs and wait (3 s) for a reading of each.
pub(crate) async fn probe(config: &Value) -> Result<Probe, LinkError> {
    let setup = Setup::from_config(config).map_err(LinkError)?;
    let c = &setup.config;
    let mut link = Link::connect(&c.host, c.port, c.password.as_deref().unwrap_or("")).await?;
    let boards = link.discover().await?;
    let board = match setup.choose(&boards) {
        Ok(b) => b.clone(),
        Err(e) => {
            return Ok(Probe {
                boards,
                chosen: None,
                readings: Map::new(),
                problem: Some(e),
                warnings: Vec::new(),
            })
        }
    };
    let warnings = open_inputs(&mut link, &setup, &board).await?;
    let mut readings = Readings::default();
    for n in &setup.inputs {
        if let Some(i) = board.input(*n) {
            readings.watch(i);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen: HashMap<i64, f64> = HashMap::new();
    while seen.len() < setup.inputs.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(m) = link.recv(left).await? else {
            break;
        };
        readings.take(&m, Instant::now());
        for (k, v) in readings.sample(Instant::now(), Duration::from_secs(3)) {
            if let (Some(n), Some(v)) = (k[2..].parse::<i64>().ok(), v.as_f64()) {
                seen.insert(n, v);
            }
        }
    }
    let mut out = Map::new();
    let mut keys: Vec<&i64> = seen.keys().collect();
    keys.sort();
    for n in keys {
        out.insert(format!("ch{n}"), json!(seen[n]));
    }
    let problem = (!readings.errors.is_empty()).then(|| {
        readings
            .errors
            .iter()
            .map(|(n, e)| format!("ch{n}: {e}"))
            .collect::<Vec<_>>()
            .join("; ")
    });
    Ok(Probe {
        boards,
        chosen: Some(board),
        readings: out,
        problem,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// A Phidget22 server for the tests (and the harness's)
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
#[allow(
    clippy::unwrap_used,
    reason = "a test double: a panic fails the test using it"
)]
pub mod mock {
    //! A Phidget22 network server with one board, speaking what libphidget22's does: the
    //! handshake and login, the board's announcements, open, set, the readings as events, a
    //! keepalive. The readings come from [`Board::temps`]; what the client set is recorded.

    use super::{
        json, mpsc, proof, read_message, write_message, Duration, HashMap, Header, Instant,
        JoinHandle, Message, OwnedWriteHalf, TcpStream, Value, BP_DATA_INTERVAL, BP_ERROR_EVENT,
        BP_SET_STATUS, BP_TEMPERATURE_CHANGE, FLAG_REPLY, IDENT, MSG_COMMAND, MSG_CONNECT,
        MSG_DEVICE, SUB_ATTACH, SUB_AUTH_S0, SUB_BRIDGE, SUB_CHANNEL, SUB_HANDSHAKE_C0,
        SUB_HANDSHAKE_S0, SUB_KEEPALIVE, SUB_OPEN, SUB_REPLY,
    };
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    /// A board the mock serves.
    #[derive(Clone)]
    pub struct MockBoard {
        pub kind: &'static str,
        pub name: &'static str,
        pub device_id: i64,
        pub serial: i64,
        pub hub_port: i64,
        /// Its temperature inputs' names, `ch0` first.
        pub inputs: Vec<&'static str>,
    }

    impl MockBoard {
        /// A 1048: four thermocouple inputs and the board's own sensor.
        pub fn usb_1048(serial: i64) -> Self {
            MockBoard {
                kind: "USB",
                name: "PhidgetTemperatureSensor 4-Input",
                device_id: 26,
                serial,
                hub_port: 0,
                inputs: vec![
                    "Thermocouple Input",
                    "Thermocouple Input",
                    "Thermocouple Input",
                    "Thermocouple Input",
                    "Temperature Sensor (IC)",
                ],
            }
        }

        /// A TMP1101 on port `port` of a VINT hub.
        pub fn vint_tmp1101(serial: i64, port: i64) -> Self {
            MockBoard {
                kind: "VINT",
                name: "4x Thermocouple Phidget",
                device_id: 89,
                serial,
                hub_port: port,
                ..Self::usb_1048(serial)
            }
        }
    }

    /// What the mock serves and what it was told.
    #[derive(Default)]
    pub struct State {
        /// The newest temperature of each input, `ch<n>` → °C; none: no events.
        pub temps: HashMap<i64, f64>,
        /// Send an error event for this input instead of its reading (an open thermocouple).
        pub open_circuit: Vec<i64>,
        /// Every bridge packet set, `(input, packet, value)`.
        pub sets: Vec<(i64, i64, Value)>,
        /// The inputs opened.
        pub opened: Vec<i64>,
        /// Logins refused for a wrong password.
        pub refused: u32,
        /// Keepalives answered.
        pub keepalives: u32,
        /// Clients dropped for a keepalive not answered in time, as the real server does.
        pub dropped: u32,
    }

    pub struct Server {
        pub port: u16,
        pub state: Arc<Mutex<State>>,
        task: JoinHandle<()>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    fn phid(board: &MockBoard) -> u64 {
        // libphidget22's ids are large; one above 2^53 checks they are read whole.
        0x0020_0000_0000_0000 + board.serial as u64 * 16 + board.hub_port as u64
    }

    fn chid(board: &MockBoard, unique: usize) -> u64 {
        phid(board) * 64 + unique as u64 + 1
    }

    /// Serve `board` on a free local port; `password` empty: none.
    pub async fn serve(board: MockBoard, password: &str, keepalive: Duration) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        let password = password.to_string();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let (board, st, password) = (board.clone(), st.clone(), password.clone());
                tokio::spawn(async move {
                    let _ = session(stream, board, st, &password, keepalive).await;
                });
            }
        });
        Server { port, state, task }
    }

    async fn send(
        w: &mut OwnedWriteHalf,
        flags: u16,
        repseq: u16,
        kind: u8,
        sub: u8,
        body: &str,
    ) -> std::io::Result<()> {
        let h = Header {
            len: 0,
            flags,
            reqseq: 0,
            repseq,
            kind,
            sub,
        };
        write_message(w, h, body).await
    }

    async fn session(
        stream: TcpStream,
        board: MockBoard,
        st: Arc<Mutex<State>>,
        password: &str,
        keepalive: Duration,
    ) -> std::io::Result<()> {
        let (mut r, mut w) = stream.into_split();
        let hello = read_message(&mut r).await?;
        assert_eq!(
            (hello.header.kind, hello.header.sub),
            (MSG_CONNECT, SUB_HANDSHAKE_C0)
        );
        assert!(hello.body["type"].as_str().unwrap().starts_with("www"));
        send(
            &mut w,
            0,
            0,
            MSG_CONNECT,
            SUB_HANDSHAKE_S0,
            r#"{"type":"phidget22server","pmajor":2,"pminor":4,"result":0}"#,
        )
        .await?;
        let c0 = read_message(&mut r).await?;
        assert_eq!(c0.body["ident"], IDENT);
        let nonce_c = c0.body["nonceC"].as_str().unwrap_or_default().to_string();
        assert!(nonce_c.len() < 16, "libphidget22 keeps 15 characters");
        let (nonce_s, salt) = ("Sx81mNoq", "pq3Kz0aa");
        let s0 = json!({ "srvname": "mock", "result": 0, "nonceC": nonce_c,
            "nonceS": nonce_s, "salt": salt, "count": 1 });
        send(&mut w, 0, 0, MSG_CONNECT, SUB_AUTH_S0, &s0.to_string()).await?;
        let c1 = read_message(&mut r).await?;
        let ok = c1.body["proof"].as_str() == Some(&proof(password, &nonce_c, nonce_s, salt));
        let verdict = json!({ "E": if ok { 0 } else { 7 } });
        send(
            &mut w,
            FLAG_REPLY,
            0,
            MSG_COMMAND,
            SUB_REPLY,
            &verdict.to_string(),
        )
        .await?;
        if !ok {
            st.lock().unwrap().refused += 1;
            return Ok(());
        }
        // The board, then its channels: its temperature inputs, then four voltage inputs
        // (a 1048's, which a temperature client leaves alone).
        let dev = json!({ "type": board.kind, "phid": phid(&board), "parent": 0, "vendorID": 1730,
            "productID": 50, "interfaceNum": 0, "version": 200, "serialNumber": board.serial,
            "label": "", "index": 0, "deviceID": board.device_id, "vintID": 0,
            "hubPort": board.hub_port, "isHubPort": 0, "name": board.name, "desc": "",
            "fwstr": "" });
        send(&mut w, 0, 0, MSG_DEVICE, SUB_ATTACH, &dev.to_string()).await?;
        let mut unique = 0;
        for (n, name) in board.inputs.iter().enumerate() {
            let ch = json!({ "parent": phid(&board), "chid": chid(&board, unique), "class": 28,
                "uniqueIndex": unique, "index": n, "version": 1, "name": name,
                "channelname": "PhidgetTemperatureSensor" });
            send(&mut w, 0, 0, MSG_DEVICE, SUB_CHANNEL, &ch.to_string()).await?;
            unique += 1;
        }
        for n in 0..4 {
            let ch = json!({ "parent": phid(&board), "chid": chid(&board, unique), "class": 29,
                "uniqueIndex": unique, "index": n, "version": 1, "name": "Voltage Input",
                "channelname": "PhidgetVoltageInput" });
            send(&mut w, 0, 0, MSG_DEVICE, SUB_CHANNEL, &ch.to_string()).await?;
            unique += 1;
        }

        let (tx, mut rx) = mpsc::channel::<Message>(64);
        let reader = tokio::spawn(async move {
            while let Ok(m) = read_message(&mut r).await {
                if tx.send(m).await.is_err() {
                    return;
                }
            }
        });
        let mut open: Vec<(i64, u64)> = Vec::new();
        let mut interval_ms: HashMap<i64, u64> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_millis(20));
        // As the real server: a keepalive at once, then one each interval; a client that has
        // not answered the last one by then is dropped.
        let mut since_keepalive = Instant::now().checked_sub(keepalive).unwrap();
        let mut awaiting = false;
        let mut elapsed: HashMap<i64, Instant> = HashMap::new();
        loop {
            tokio::select! {
                m = rx.recv() => {
                    let Some(m) = m else { break };
                    let h = m.header;
                    match (h.kind, h.sub) {
                        (MSG_DEVICE, SUB_OPEN) => {
                            let unique = m.body["index"].as_i64().unwrap_or(-1);
                            let n = unique; // temperature inputs come first
                            // As libphidget22: the state with the input's limits; 1e300 is
                            // "unknown". A board's own sensor reports every 300 ms at most.
                            let own = board.inputs.get(n as usize).is_some_and(|i| i.contains("IC"));
                            let status = json!({ "v": 0, "s": 1, "f": 4, "p": BP_SET_STATUS,
                                "O": m.body["channel"], "X": unique, "c": 5,
                                "e": { "_class_version_": { "t": "u", "v": 1 },
                                       "dataInterval": { "t": "u", "v": 256 },
                                       "minDataInterval": { "t": "u", "v": if own { 300 } else { 32 } },
                                       "maxDataInterval": { "t": "u", "v": 60000 },
                                       "temperature": { "t": "g", "v": 1.0e300 } } })
                                .to_string();
                            send(&mut w, FLAG_REPLY, h.reqseq, MSG_DEVICE, SUB_BRIDGE, &status).await?;
                            open.push((n, m.body["channel"].as_u64().unwrap_or(0)));
                            st.lock().unwrap().opened.push(n);
                        }
                        (MSG_DEVICE, SUB_BRIDGE) => {
                            let n = m.body["X"].as_i64().unwrap_or(-1);
                            let p = m.body["p"].as_i64().unwrap_or(-1);
                            let v = m.body["e"]["0"]["v"].clone();
                            let mut reply = json!({ "E": 0 });
                            if p == BP_DATA_INTERVAL {
                                let ms = v.as_u64().unwrap_or(0);
                                let own = board.inputs.get(n as usize).is_some_and(|i| i.contains("IC"));
                                let lo = if own { 300 } else { 32 };
                                if (lo..=60_000).contains(&ms) {
                                    interval_ms.insert(n, ms);
                                } else {
                                    reply = json!({ "E": 21, "R": format!("Value must be in range: {lo} - 60000.") });
                                }
                            }
                            st.lock().unwrap().sets.push((n, p, v));
                            send(&mut w, FLAG_REPLY, h.reqseq, MSG_COMMAND, SUB_REPLY, &reply.to_string()).await?;
                        }
                        (MSG_COMMAND, SUB_KEEPALIVE) if h.flags & FLAG_REPLY != 0 => {
                            st.lock().unwrap().keepalives += 1;
                            awaiting = false;
                        }
                        _ => {}
                    }
                }
                _ = tick.tick() => {
                    if since_keepalive.elapsed() >= keepalive {
                        if awaiting {
                            st.lock().unwrap().dropped += 1;
                            break;
                        }
                        since_keepalive = Instant::now();
                        awaiting = true;
                        let h = Header { len: 0, flags: 4, reqseq: 7, repseq: 0,
                            kind: MSG_COMMAND, sub: SUB_KEEPALIVE };
                        write_message(&mut w, h, "").await?;
                    }
                    for (n, chid) in &open {
                        let every = Duration::from_millis(*interval_ms.get(n).unwrap_or(&256));
                        let last = elapsed.entry(*n).or_insert_with(Instant::now);
                        if last.elapsed() < every { continue }
                        *last = Instant::now();
                        let (temp, broken) = {
                            let s = st.lock().unwrap();
                            (s.temps.get(n).copied(), s.open_circuit.contains(n))
                        };
                        let body = if broken {
                            json!({ "v": 0, "s": 1, "f": 1, "p": BP_ERROR_EVENT, "O": chid,
                                "c": 2, "e": { "0": { "t": "d", "v": 4103 },
                                "1": { "t": "s", "v": "Thermocouple open circuit" } } })
                        } else if let Some(t) = temp {
                            json!({ "v": 0, "s": 1, "f": 1, "p": BP_TEMPERATURE_CHANGE,
                                "O": chid, "c": 1, "e": { "0": { "t": "g", "v": t } } })
                        } else { continue };
                        send(&mut w, 0, 0, MSG_DEVICE, SUB_BRIDGE, &body.to_string()).await?;
                    }
                }
            }
        }
        reader.abort();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{serve, MockBoard};
    use super::*;

    #[test]
    fn a_header_round_trips_little_endian() {
        let h = Header {
            len: 300,
            flags: FLAG_REPLY,
            reqseq: 0x1234,
            repseq: 11,
            kind: MSG_DEVICE,
            sub: SUB_BRIDGE,
        };
        let b = h.encode();
        assert_eq!(
            &b[0..4],
            &[0x30, 0x49, 0x48, 0x50],
            "magic, as Phidgets' client"
        );
        assert_eq!(&b[4..6], &[44, 1]);
        assert_eq!(Header::decode(&b).unwrap(), h);
        let mut bad = b;
        bad[0] = 0;
        assert!(Header::decode(&bad)
            .unwrap_err()
            .contains("not a Phidget22 server"));
    }

    #[test]
    fn nan_becomes_null_outside_strings() {
        let t = r#"{"a":nan,"b":-nan(ind),"c":[1,NaN],"d":"nan here","e":"q\"nan"}"#;
        let v: Value = serde_json::from_str(&clean_nan(t)).unwrap();
        assert_eq!(v["a"], Value::Null);
        assert_eq!(v["b"], Value::Null);
        assert_eq!(v["c"], json!([1, null]));
        assert_eq!(v["d"], "nan here");
        assert_eq!(v["e"], "q\"nan");
        assert_eq!(clean_nan(r#"{"name":"Thermo°"}"#), r#"{"name":"Thermo°"}"#);
    }

    #[test]
    fn the_proof_is_phidgets_clients() {
        // sha256("phidgetclient" + "" + "abc" + "def" + "ghi"), base64: what Phidgets'
        // JavaScript client computes for these nonces.
        let p = proof("", "abc", "def", "ghi");
        let want = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(b"phidgetclientabcdefghi"));
        assert_eq!(p, want);
        assert_ne!(proof("x", "abc", "def", "ghi"), p);
        assert!(nonce().len() < 16);
    }

    fn setup(v: Value) -> Setup {
        Setup::from_config(&v).unwrap()
    }

    #[test]
    fn the_setup_reads_its_inputs_from_the_mapping() {
        let s = setup(json!({ "host": "h" }));
        assert_eq!(s.inputs, vec![0, 1], "BT ch0, ET ch1 by default");
        assert_eq!(s.config.port, 5661);
        assert_eq!(s.config.data_interval_ms, 256);
        let s = setup(json!({ "host": "h", "thermocouple_types": { "ch2": "J" },
            "ingest": { "fields": { "bt": "ch1", "et": "ch0", "exhaust": "ch2" } } }));
        assert_eq!(s.inputs, vec![0, 1, 2]);
        assert_eq!(s.thermocouple(2), ThermocoupleType::J);
        assert_eq!(s.thermocouple(1), ThermocoupleType::K);
        let e = |v: Value| Setup::from_config(&v).err().unwrap();
        assert!(
            e(json!({ "host": "h", "ingest": { "fields": { "bt": "ch8" } } }))
                .contains("ch0 to ch7")
        );
        assert!(
            e(json!({ "host": "h", "ingest": { "units": "F", "fields": { "bt": "ch0" } } }))
                .contains("unknown key")
        );
        assert!(e(json!({ "host": "h", "thermocouple_type": "N" })).contains("Phidget22 config"));
    }

    fn board(serial: i64, port: i64, n: i64) -> Board {
        Board {
            phid: serial as u64,
            serial,
            hub_port: port,
            kind: "VINT".into(),
            name: "TMP1101".into(),
            inputs: (0..n)
                .map(|i| Input {
                    chid: i as u64,
                    unique_index: i,
                    index: i,
                    version: 1,
                    name: if i < 4 {
                        "Thermocouple Input".into()
                    } else {
                        "Temperature Sensor (IC)".into()
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn the_board_is_the_first_that_fits() {
        let boards = vec![board(100, 0, 5), board(200, 3, 5)];
        let s = setup(json!({ "host": "h" }));
        assert_eq!(s.choose(&boards).unwrap().serial, 100);
        let s = setup(json!({ "host": "h", "serial_number": 200 }));
        assert_eq!(s.choose(&boards).unwrap().serial, 200);
        let s = setup(json!({ "host": "h", "hub_port": 3 }));
        assert_eq!(s.choose(&boards).unwrap().serial, 200);
        let s = setup(json!({ "host": "h", "serial_number": 300 }));
        let e = s.choose(&boards).unwrap_err();
        assert!(
            e.contains("serial number 300") && e.contains("TMP1101 #100 port 0"),
            "{e}"
        );
        let s = setup(json!({ "host": "h", "ingest": { "fields": { "bt": "ch6" } } }));
        let e = s.choose(&boards).unwrap_err();
        assert!(
            e.contains("has no ch6") && e.contains("ch0–ch3 thermocouple"),
            "{e}"
        );
        assert!(e.contains("ch4 the board's own sensor"), "{e}");
        assert!(s.choose(&[]).unwrap_err().contains("no temperature board"));
    }

    fn event(chid: u64, p: i64, e: Value) -> Message {
        Message {
            header: Header {
                len: 0,
                flags: 0,
                reqseq: 0,
                repseq: 0,
                kind: MSG_DEVICE,
                sub: SUB_BRIDGE,
            },
            body: json!({ "p": p, "O": chid, "e": e }),
        }
    }

    #[test]
    fn a_sample_is_the_mean_since_the_last_and_goes_stale() {
        let b = board(1, 0, 5);
        let mut r = Readings::default();
        r.watch(&b.inputs[0]);
        r.watch(&b.inputs[1]);
        let t0 = Instant::now();
        for v in [200.0, 202.0, 204.0] {
            r.take(
                &event(0, BP_TEMPERATURE_CHANGE, json!({ "0": { "v": v } })),
                t0,
            );
        }
        r.take(
            &event(1, BP_TEMPERATURE_CHANGE, json!({ "0": { "v": 250.5 } })),
            t0,
        );
        r.take(
            &event(9, BP_TEMPERATURE_CHANGE, json!({ "0": { "v": 1.0 } })),
            t0,
        );
        let s = r.sample(t0, Duration::from_secs(3));
        assert_eq!(s.get("ch0"), Some(&json!(202.0)));
        assert_eq!(s.get("ch1"), Some(&json!(250.5)));
        assert_eq!(s.len(), 2, "an input not opened is not read");
        // No new reading: the newest stands until it is stale.
        let s = r.sample(t0 + Duration::from_secs(2), Duration::from_secs(3));
        assert_eq!(s.get("ch0"), Some(&json!(204.0)));
        assert!(r
            .sample(t0 + Duration::from_secs(4), Duration::from_secs(3))
            .is_empty());
        // An error event (open circuit) leaves the input out until it reads again.
        r.take(
            &event(0, BP_TEMPERATURE_CHANGE, json!({ "0": { "v": 210.0 } })),
            t0,
        );
        r.take(
            &event(
                0,
                BP_ERROR_EVENT,
                json!({ "0": { "v": 4103 }, "1": { "v": "open" } }),
            ),
            t0,
        );
        r.take(
            &event(0, BP_TEMPERATURE_CHANGE, json!({ "0": { "v": null } })),
            t0,
        );
        assert!(r.sample(t0, Duration::from_secs(3)).get("ch0").is_none());
        assert_eq!(r.errors.get(&0).map(String::as_str), Some("open"));
    }

    #[tokio::test]
    async fn the_client_logs_in_opens_sets_and_reads_from_a_mock_server() {
        let srv = serve(
            MockBoard::usb_1048(370_001),
            "s3cret",
            Duration::from_millis(200),
        )
        .await;
        srv.state
            .lock()
            .unwrap()
            .temps
            .extend([(0, 201.25), (1, 230.5)]);
        let cfg = json!({ "host": "127.0.0.1", "port": srv.port, "password": "s3cret",
            "thermocouple_types": { "ch1": "J" } });
        let s = setup(cfg);
        let (mut link, b, warnings) = start(&s).await.unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(b.label(), "1048 #370001");
        assert_eq!(
            b.describe(),
            "ch0–ch3 thermocouple, ch4 the board's own sensor"
        );
        assert!(b.phid > (1 << 53), "a large id is read whole");
        let mut r = Readings::default();
        b.inputs[..2].iter().for_each(|i| r.watch(i));
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Some(m) = link.recv(Duration::from_millis(100)).await.unwrap() {
                r.take(&m, Instant::now());
            }
        }
        let sample = r.sample(Instant::now(), Duration::from_secs(3));
        assert_eq!(sample.get("ch0"), Some(&json!(201.25)));
        assert_eq!(sample.get("ch1"), Some(&json!(230.5)));
        let st = srv.state.lock().unwrap();
        assert_eq!(st.opened, vec![0, 1]);
        assert!(st.sets.contains(&(0, BP_DATA_INTERVAL, json!(256))));
        assert!(st.sets.contains(&(0, BP_CHANGE_TRIGGER, json!(0.0))));
        assert!(st.sets.contains(&(0, BP_THERMOCOUPLE_TYPE, json!(2))), "K");
        assert!(st.sets.contains(&(1, BP_THERMOCOUPLE_TYPE, json!(1))), "J");
        assert!(st.keepalives >= 5, "keepalives answered: {}", st.keepalives);
    }

    #[tokio::test]
    async fn a_wrong_or_missing_password_says_so() {
        let srv = serve(MockBoard::usb_1048(1), "right", Duration::from_secs(5)).await;
        let e = Link::connect("127.0.0.1", srv.port, "wrong")
            .await
            .err()
            .unwrap();
        assert!(e.0.contains("password is wrong"), "{e}");
        let e = Link::connect("127.0.0.1", srv.port, "")
            .await
            .err()
            .unwrap();
        assert!(e.0.contains("wants a password"), "{e}");
        assert_eq!(srv.state.lock().unwrap().refused, 2);
        let e = Link::connect("127.0.0.1", 1, "").await.err().unwrap();
        assert!(e.0.contains("could not reach 127.0.0.1:1"), "{e}");
    }

    /// A roaster on a Phidget, as Add roaster makes it: read by the poller, judged by the
    /// check, its mapping previewed; its password never shown and kept through an edit.
    #[tokio::test]
    async fn a_phidget_roaster_is_read_end_to_end() {
        use crate::test_support::{eventually, Harness};
        let srv = serve(
            MockBoard::vint_tmp1101(620_011, 1),
            "pw",
            Duration::from_secs(2),
        )
        .await;
        srv.state
            .lock()
            .unwrap()
            .temps
            .extend([(0, 180.25), (1, 220.5), (2, 150.0)]);
        let h = Harness::new().await;
        let dev = h
            .owner(
                "POST",
                "/api/devices",
                Some(json!({ "name": "Gas drum", "device_id": "phidget_drum" })),
            )
            .await;
        assert!(dev.status.is_success(), "{:?}", dev.json());
        let reg = dev.json()["id"].as_str().unwrap().to_string();
        let config = json!({ "host": "127.0.0.1", "port": srv.port, "password": "pw",
            "hub_port": 1, "poll_interval_ms": 500,
            "ingest": { "fields": { "bt": "ch0", "et": "ch1", "exhaust": "ch2" } } });
        let bad = h
            .owner(
                "POST",
                &format!("/api/devices/{reg}/connections"),
                Some(json!({ "protocol": "phidget22", "enabled": true,
                    "config": { "host": "h", "commands": { "heater": {} } } })),
            )
            .await;
        assert_eq!(bad.status, 400);
        assert!(bad.json().to_string().contains("reads temperatures only"));
        let c = h
            .owner(
                "POST",
                &format!("/api/devices/{reg}/connections"),
                Some(json!({ "protocol": "phidget22", "enabled": true, "config": config })),
            )
            .await;
        assert_eq!(c.status, 201, "{:?}", c.json());
        assert_eq!(c.json()["config"]["password"], "********", "never shown");
        let conn = c.json()["id"].as_str().unwrap().to_string();
        h.owner(
            "PUT",
            &format!("/api/devices/{reg}"),
            Some(json!({ "status": "active" })),
        )
        .await;
        let s = h
            .owner(
                "PUT",
                "/api/roaster/phidget_drum/setup",
                Some(json!({ "kind": "logger", "control": "off",
                    "channels": [{ "key": "exhaust", "label": "Exhaust", "unit": "C" }] })),
            )
            .await;
        assert!(s.status.is_success(), "{:?}", s.json());

        let check = || async {
            h.owner("GET", "/api/roaster/phidget_drum/check", None)
                .await
                .json()
        };
        assert!(
            eventually(|| async { check().await["reading"]["bt"] == json!(180.25) }).await,
            "{:#}",
            check().await
        );
        let last = check().await;
        assert_eq!(last["reading"]["et"], json!(220.5), "{last:#}");
        // A gas drum's outputs cannot come through a Phidget: the check says so.
        h.owner(
            "PUT",
            "/api/roaster/phidget_drum/setup",
            Some(json!({ "kind": "drum_gas", "control": "off",
                "channels": [{ "key": "exhaust", "label": "Exhaust", "unit": "C" }] })),
        )
        .await;
        let outputs = check().await["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["check"] == "outputs")
            .map(|l| l["message"].to_string())
            .unwrap_or_default();
        assert!(outputs.contains("reads temperatures only"), "{outputs}");

        // The preview: the newest raw sample through the mapping.
        let p = h
            .owner(
                "POST",
                "/api/roaster/phidget_drum/ingest/preview",
                Some(json!({ "connection_id": conn })),
            )
            .await;
        assert!(p.status.is_success(), "{:?}", p.json());
        assert_eq!(
            p.json()["sample"]["exhaust"],
            json!(150.0),
            "{:#}",
            p.json()
        );

        // An edit that sends the mask back keeps the password: still logged in.
        let mut edited = config.clone();
        edited["password"] = json!("********");
        edited["poll_interval_ms"] = json!(400);
        let u = h
            .owner(
                "PUT",
                &format!("/api/devices/{reg}/connections/{conn}"),
                Some(json!({ "config": edited })),
            )
            .await;
        assert_eq!(u.status, 200, "{:?}", u.json());
        let refused = srv.state.lock().unwrap().refused;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(
            srv.state.lock().unwrap().refused,
            refused,
            "the saved password was used"
        );
        assert!(
            srv.state.lock().unwrap().opened.len() >= 6,
            "reopened after the edit"
        );

        // The connection test of a saved form (password masked) uses the saved password.
        let t = h
            .owner(
                "POST",
                "/api/devices/test-connection",
                Some(
                    json!({ "protocol": "phidget22", "device_id": "phidget_drum",
                    "config": edited }),
                ),
            )
            .await;
        assert_eq!(t.json()["success"], json!(true), "{:#}", t.json());
        assert!(t.json()["message"]
            .as_str()
            .unwrap()
            .contains("TMP1101 #620011 port 1"));
    }

    /// Against a real Phidget22 network server (Phidgets' own, in Docker, or a board's):
    /// `PHIDGET22_SERVER=host:port PHIDGET22_PASSWORD=… cargo test -p
    /// roastwarden-server --lib real_phidget22_server -- --ignored --nocapture`. Logs in,
    /// prints what the server announces, and with `PHIDGET22_EXPECT_BOARD` set, reads it.
    #[tokio::test]
    #[ignore = "needs a Phidget22 network server"]
    async fn real_phidget22_server() {
        let addr = std::env::var("PHIDGET22_SERVER").expect("PHIDGET22_SERVER=host:port");
        let (host, port) = addr.rsplit_once(':').expect("host:port");
        let password = std::env::var("PHIDGET22_PASSWORD").unwrap_or_default();
        let port: u16 = port.parse().unwrap();
        let mut link = Link::connect(host, port, &password).await.unwrap();
        println!("logged in to {}", link.server);
        let mut all = Vec::new();
        while let Some(m) = link.recv(Duration::from_millis(1500)).await.unwrap() {
            println!("{}/{} {}", m.header.kind, m.header.sub, m.body);
            all.push(m);
        }
        let mut boards = Boards::default();
        for m in &all {
            boards.take(m);
        }
        let boards = boards.list();
        println!("temperature boards: {boards:?}");
        // A keepalive arrives within the server's interval and is answered: still logged in.
        tokio::time::sleep(Duration::from_secs(12)).await;
        assert!(
            link.recv(Duration::from_millis(100)).await.is_ok(),
            "still connected"
        );
        if std::env::var("PHIDGET22_EXPECT_BOARD").is_ok() {
            let cfg = json!({ "host": host, "port": port, "password": password });
            let p = probe(&cfg).await.unwrap();
            println!("{}", p.summary(&addr));
            assert!(!p.readings.is_empty());
        }
        if !password.is_empty() {
            let e = Link::connect(host, port, "not-the-password")
                .await
                .err()
                .unwrap();
            println!("wrong password: {e}");
            assert!(e.0.contains("password"), "{e}");
        }
    }

    /// The real server drops a client that misses a keepalive (every 4 s): the link answers
    /// them whatever its owner does, so a poller busy elsewhere stays connected.
    #[tokio::test]
    async fn keepalives_are_answered_while_nobody_reads() {
        let srv = serve(MockBoard::usb_1048(7), "", Duration::from_millis(150)).await;
        srv.state.lock().unwrap().temps.insert(0, 99.5);
        let s = setup(
            json!({ "host": "127.0.0.1", "port": srv.port, "data_interval_ms": 50,
            "ingest": { "fields": { "bt": "ch0" } } }),
        );
        let (mut link, b, _) = start(&s).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let (dropped, answered) = {
            let st = srv.state.lock().unwrap();
            (st.dropped, st.keepalives)
        };
        assert_eq!(dropped, 0, "answered: {answered}");
        assert!(answered >= 6, "{answered}");
        let mut r = Readings::default();
        r.watch(&b.inputs[0]);
        let until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < until {
            if let Some(m) = link.recv(Duration::from_millis(100)).await.unwrap() {
                r.take(&m, Instant::now());
            }
        }
        let raw = r.sample(Instant::now(), Duration::from_secs(3));
        assert_eq!(raw.get("ch0"), Some(&json!(99.5)));
    }

    #[tokio::test]
    async fn the_probe_lists_the_boards_and_reads_them() {
        let srv = serve(MockBoard::vint_tmp1101(500, 2), "", Duration::from_secs(5)).await;
        srv.state
            .lock()
            .unwrap()
            .temps
            .extend([(0, 21.5), (1, 22.0)]);
        srv.state.lock().unwrap().open_circuit.push(2);
        let cfg = json!({ "host": "127.0.0.1", "port": srv.port, "data_interval_ms": 10,
            "ingest": { "fields": { "bt": "ch0", "et": "ch1", "exhaust": "ch2" } } });
        let p = probe(&cfg).await.unwrap();
        let text = p.summary("127.0.0.1");
        assert!(text.contains("TMP1101 #500 port 2"), "{text}");
        assert!(text.contains("ch0 21.5 °C, ch1 22 °C"), "{text}");
        assert!(text.contains("ch2: Thermocouple open circuit"), "{text}");
        assert!(
            text.contains("Not set: ch0: reports every 32 ms, its shortest (asked 10)"),
            "{text}"
        );
        let cfg = json!({ "host": "127.0.0.1", "port": srv.port, "serial_number": 9 });
        let p = probe(&cfg).await.unwrap();
        assert!(p.chosen.is_none());
        assert!(p
            .summary("x")
            .contains("no temperature board with serial number 9"));
    }
}
