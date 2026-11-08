//! An ESP32 roaster's firmware, filled in (docs/esp32-starter.md, docs/esp32-fluid-bed.md): one of
//! the example sketches with the roaster's id, its MQTT password, the server's address and the
//! Wi-Fi written into its settings, as a PlatformIO project in a zip. The sketch sits in a folder
//! of its own name, so the Arduino IDE opens the same files.
//!
//! The sketches are the examples in `examples/roasters/` (built in, so the download and the site
//! never differ); `examples/build.sh` compiles them. The settings are found by their
//! lines (`const char* WIFI_NAME = "…";`): a sketch that renames one fails the tests here.

use std::fmt::Write as _;
use std::io::Write as _;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const STARTER: &str =
    include_str!("../../../../examples/roasters/roastwarden-starter/roastwarden-starter.ino");
const FLUID_BED: &str =
    include_str!("../../../../examples/roasters/roastwarden-fluid-bed/roastwarden-fluid-bed.ino");

/// PlatformIO's ESP32 platform with the Arduino core 3 the sketches need (pioarduino's: the
/// registry's `espressif32` is still on core 2). The same as `examples/build.sh`.
pub const PLATFORM: &str = "https://github.com/pioarduino/platform-espressif32/releases/download/55.03.37/platform-espressif32.zip";

/// What the ESP32 does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareKind {
    /// Reads BT and ET and sends them: a logger, the roaster's own controls run the roast
    Starter,
    /// An electric fluid bed: heater through an SSR, fan, its own PID and safety
    FluidBed,
}

impl FirmwareKind {
    pub fn sketch_name(self) -> &'static str {
        match self {
            Self::Starter => "roastwarden-starter",
            Self::FluidBed => "roastwarden-fluid-bed",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::Starter => STARTER,
            Self::FluidBed => FLUID_BED,
        }
    }

    fn info(self) -> FirmwareKindInfo {
        let (name, summary, guide) = match self {
            Self::Starter => (
                "Starter: reads temperatures",
                "BT and ET from one or two thermocouple boards, once a second. It controls nothing: you roast with the roaster's own controls.",
                "https://roastwarden.com/docs/esp32-starter/",
            ),
            Self::FluidBed => (
                "Fluid-bed controller",
                "An electric fluid bed: the heater through a solid state relay, the fan, a PID on the bean temperature and its own safety. Roastwarden drives it by hand or from a profile.",
                "https://roastwarden.com/docs/esp32-fluid-bed/",
            ),
        };
        FirmwareKindInfo {
            kind: self,
            name: name.into(),
            summary: summary.into(),
            guide: guide.into(),
        }
    }
}

/// The thermocouple board the probes are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThermocoupleBoard {
    #[default]
    Max31855,
    Max6675,
}

fn yes() -> bool {
    true
}

/// What to fill in. Anything left out stays as the sketch's placeholder, to fill in by hand.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FirmwareRequest {
    pub kind: FirmwareKind,
    /// The roaster's id: its MQTT user and topics (`roaster/<id>/…`). Letters, digits, `_`, `-`.
    pub roaster_id: String,
    /// Its MQTT password. Left out: the one the server has for this id (`MQTT_DEVICES`, or added
    /// under Roasters → MQTT passwords), if any.
    #[serde(default)]
    pub password: Option<String>,
    /// The address the ESP32 reaches the MQTT broker at: this server's on the network
    /// (`192.168.1.50`), or the broker's when it runs elsewhere.
    pub server_address: String,
    /// The broker's port. Left out: the built-in broker's, else 1883.
    #[serde(default)]
    pub port: Option<u16>,
    /// The 2.4 GHz Wi-Fi network's name. Never stored.
    #[serde(default)]
    pub wifi_name: Option<String>,
    /// Its password. Never stored, masked in the audit log.
    #[serde(default)]
    pub wifi_password: Option<String>,
    #[serde(default)]
    pub board: ThermocoupleBoard,
    /// A second probe for ET (default true)
    #[serde(default = "yes")]
    pub et_probe: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FirmwareKindInfo {
    pub kind: FirmwareKind,
    pub name: String,
    pub summary: String,
    /// The guide on roastwarden.com
    pub guide: String,
}

/// A roaster id the server has an MQTT password for (`MQTT_DEVICES` or the dashboard's).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FirmwareRoaster {
    pub id: String,
    /// Set up in Roasters already
    pub added: bool,
}

/// What the firmware download can fill in on this server.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FirmwareInfo {
    pub kinds: Vec<FirmwareKindInfo>,
    /// The MQTT port a roaster connects to: the built-in broker's, else 1883
    pub port: u16,
    /// The server runs its own MQTT broker (`ROASTWARDEN_BROKER_ADDR`)
    pub built_in_broker: bool,
    /// The ids the server has a password for (`MQTT_DEVICES`, Roasters → MQTT passwords): the
    /// download fills theirs in
    pub roasters: Vec<FirmwareRoaster>,
}

/// The MQTT port a roaster connects to.
pub fn default_port() -> u16 {
    crate::routes::about::broker_info()
        .and_then(|b| b.addr.parse::<std::net::SocketAddr>().ok())
        .map_or(1883, |a| a.port())
}

/// What can be filled in; `passwords` the roasters' (`MQTT_DEVICES`' and the dashboard's),
/// `added` the roasters set up under Roasters.
pub fn info(
    passwords: &crate::roasters::mqtt_passwords::MqttPasswords,
    added: &[String],
) -> FirmwareInfo {
    FirmwareInfo {
        kinds: vec![FirmwareKind::Starter.info(), FirmwareKind::FluidBed.info()],
        port: default_port(),
        built_in_broker: passwords.built_in(),
        roasters: passwords
            .ids()
            .into_iter()
            .map(|(id, _)| FirmwareRoaster {
                added: added.contains(&id),
                id,
            })
            .collect(),
    }
}

/// The download: a zip and its file name, and the settings left to fill in by hand.
pub struct Firmware {
    pub file_name: String,
    pub zip: Vec<u8>,
    /// `wifi`, `password`: left as the sketch's placeholders
    pub missing: Vec<&'static str>,
}

/// A request checked and completed: the password the server has for the id, the default port.
/// A field given and not empty.
fn given(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty())
}

pub fn build(req: &FirmwareRequest, devices: &[(String, String)], port: u16) -> Result<Firmware> {
    let id = req.roaster_id.trim();
    if id.is_empty()
        || id.len() > 48
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("roaster_id: letters, digits, _ and - only, at most 48");
    }
    let address = req.server_address.trim();
    if address.is_empty()
        || address.len() > 253
        || !address
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        bail!("server_address: an IP address or a host name (192.168.1.50), without http:// or a port");
    }
    let password = given(req.password.as_deref())
        .map(str::to_string)
        .or_else(|| {
            devices
                .iter()
                .find(|(d, _)| d == id)
                .map(|(_, p)| p.clone())
        });
    let wifi_name = given(req.wifi_name.as_deref());
    let wifi_password = req.wifi_password.as_deref().unwrap_or("");
    check_text("password", password.as_deref().unwrap_or(""), 128)?;
    check_text("wifi_name", wifi_name.unwrap_or(""), 32)?;
    check_text("wifi_password", wifi_password, 63)?;
    if wifi_name.is_some() && !wifi_password.is_empty() && wifi_password.len() < 8 {
        bail!(
            "wifi_password: a Wi-Fi password has 8 characters or more (none for an open network)"
        );
    }

    let mut s = Sketch(req.kind.source().to_string());
    s.string("ROASTER_ID", id)?;
    s.string("SERVER_ADDRESS", address)?;
    s.number("SERVER_PORT", &req.port.unwrap_or(port).to_string())?;
    let mut missing = Vec::new();
    match &password {
        Some(p) => s.string("ROASTER_PASSWORD", p)?,
        None => missing.push("password"),
    }
    match wifi_name {
        Some(n) => {
            s.string("WIFI_NAME", n)?;
            s.string("WIFI_PASSWORD", wifi_password)?;
        }
        None => missing.push("wifi"),
    }
    if req.board == ThermocoupleBoard::Max6675 {
        s.switch("BOARD_MAX31855", false)?;
        s.switch("BOARD_MAX6675", true)?;
    }
    if !req.et_probe {
        s.number("PIN_ET_SELECT", "-1")?;
    }

    let sketch = req.kind.sketch_name();
    let folder = format!("{sketch}-{id}");
    let files = [
        (
            format!("{folder}/{sketch}/{sketch}.ino"),
            s.0.replace("\r\n", "\n"),
        ),
        (format!("{folder}/platformio.ini"), platformio_ini(req, id)),
        (format!("{folder}/README.txt"), readme(req, id, &missing)),
    ];
    Ok(Firmware {
        file_name: format!("{folder}.zip"),
        zip: zip(&files)?,
        missing,
    })
}

fn check_text(field: &str, v: &str, max: usize) -> Result<()> {
    if v.len() > max {
        bail!("{field}: at most {max} bytes");
    }
    if v.chars().any(char::is_control) {
        bail!("{field}: no line breaks or control characters");
    }
    Ok(())
}

/// A C string literal's contents.
fn c_string(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The sketch's text, its settings changed line by line.
struct Sketch(String);

impl Sketch {
    /// The one line that starts with `start`, replaced by `f(line)`.
    fn line(&mut self, start: &str, f: impl Fn(&str) -> String) -> Result<()> {
        let mut found = 0;
        let out: Vec<String> = self
            .0
            .lines()
            .map(|l| {
                if l.starts_with(start) {
                    found += 1;
                    f(l)
                } else {
                    l.to_string()
                }
            })
            .collect();
        if found != 1 {
            bail!("the sketch has {found} lines starting \"{start}\", not one");
        }
        self.0 = out.join("\n") + "\n";
        Ok(())
    }

    /// `const char* NAME = "value";`, the comment after it kept.
    fn string(&mut self, name: &str, value: &str) -> Result<()> {
        let start = format!("const char* {name} = \"");
        let v = c_string(value);
        self.line(&start, |l| {
            let rest = &l[start.len()..];
            let tail = rest.find("\";").map_or("\";", |i| &rest[i..]);
            format!("{start}{v}{tail}")
        })
    }

    /// `const int NAME = 1883;`, the comment after it kept.
    fn number(&mut self, name: &str, value: &str) -> Result<()> {
        let start = format!("const int {name} = ");
        self.line(&start, |l| {
            let rest = &l[start.len()..];
            let tail = rest.find(';').map_or(";", |i| &rest[i..]);
            format!("{start}{value}{tail}")
        })
    }

    /// `#define NAME` on, or `// #define NAME` off.
    fn switch(&mut self, name: &str, on: bool) -> Result<()> {
        let on_line = format!("#define {name}");
        let off_line = format!("// #define {name}");
        let is = |l: &str| {
            let t = l.trim_end();
            t == on_line || t == off_line
        };
        let n = self.0.lines().filter(|l| is(l)).count();
        if n != 1 {
            bail!("the sketch has {n} lines switching {name}, not one");
        }
        let want = if on { &on_line } else { &off_line };
        self.0 = self
            .0
            .lines()
            .map(|l| if is(l) { want.clone() } else { l.to_string() })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        Ok(())
    }
}

fn libraries(req: &FirmwareRequest) -> Vec<&'static str> {
    let mut libs = vec!["knolleary/PubSubClient@^2.8"];
    if req.kind == FirmwareKind::FluidBed {
        libs.push("bblanchon/ArduinoJson@^7.4.1");
    }
    libs.push(match req.board {
        ThermocoupleBoard::Max31855 => "adafruit/Adafruit MAX31855 library@^1.4.2",
        ThermocoupleBoard::Max6675 => "adafruit/MAX6675 library@^1.1.2",
    });
    libs
}

fn platformio_ini(req: &FirmwareRequest, id: &str) -> String {
    let sketch = req.kind.sketch_name();
    let libs = libraries(req).iter().fold(String::new(), |mut s, l| {
        let _ = writeln!(s, "    {l}");
        s
    });
    let mut ini = format!(
        "; Roastwarden: {name} for the roaster \"{id}\".\n\
         ; Open this folder in VS Code with the PlatformIO extension, plug in the ESP32, then\n\
         ; PlatformIO: Upload (the arrow in the status bar) and Serial Monitor (the plug).\n\
         ; The sketch is in {sketch}/, so the Arduino IDE opens it too (README.txt).\n\
         \n\
         [platformio]\n\
         src_dir = {sketch}\n\
         default_envs = esp32\n\
         \n\
         [env:esp32]\n\
         platform = {PLATFORM}\n\
         board = esp32dev\n\
         framework = arduino\n\
         monitor_speed = 115200\n\
         lib_deps =\n{libs}",
        name = req.kind.info().name,
    );
    if req.kind == FirmwareKind::FluidBed {
        let _ = write!(
            ini,
            "\n\
             ; Updates over Wi-Fi, once UPDATE_PASSWORD is set in the sketch and uploaded over USB:\n\
             ;   UPDATE_PASSWORD=its-password pio run -e esp32-wifi -t upload\n\
             [env:esp32-wifi]\n\
             extends = env:esp32\n\
             upload_protocol = espota\n\
             upload_port = {id}.local\n\
             upload_flags = --auth=${{sysenv.UPDATE_PASSWORD}}\n"
        );
    }
    ini
}

fn readme(req: &FirmwareRequest, id: &str, missing: &[&str]) -> String {
    let sketch = req.kind.sketch_name();
    let info = req.kind.info();
    let mut todo = String::new();
    if missing.contains(&"wifi") {
        todo.push_str("  - WIFI_NAME and WIFI_PASSWORD: your 2.4 GHz Wi-Fi network\n");
    }
    if missing.contains(&"password") {
        let _ = write!(
            todo,
            "  - ROASTER_PASSWORD: {id}'s MQTT password. Roastwarden's dashboard makes one\n    (Roasters → MQTT passwords), or it is after \"{id}:\" in MQTT_DEVICES, in its .env\n"
        );
    }
    if req.kind == FirmwareKind::FluidBed {
        todo.push_str(
            "  - YOUR SETTINGS 4 to 7: the heater and fan pins, the fan's kind, the safety limits\n",
        );
    }
    let todo = if todo.is_empty() {
        "Nothing: the settings are filled in.\n".to_string()
    } else {
        format!("At the top of the sketch, under YOUR SETTINGS:\n{todo}")
    };
    let libs = libraries(req).iter().fold(String::new(), |mut s, l| {
        let name = l.split_once('/').map_or(*l, |(_, n)| n);
        let _ = writeln!(s, "  - {}", name.split('@').next().unwrap_or(name));
        s
    });
    let mut text = format!(
        "Roastwarden: {name}\n\
         Roaster: {id}\n\
         \n\
         {summary}\n\
         The guide, step by step: {guide}\n\
         \n\
         Still to fill in\n\
         ----------------\n\
         {todo}\n\
         With VS Code and PlatformIO\n\
         ---------------------------\n\
         1. Install VS Code and its PlatformIO IDE extension.\n\
         2. File > Open Folder: this folder (the one with platformio.ini).\n\
         3. Plug in the ESP32. PlatformIO: Upload (the arrow in the bottom bar). The first time it\n   \
            downloads the ESP32 tools and the libraries, which takes a while.\n\
         4. PlatformIO: Serial Monitor (the plug): the ESP32 says what it is doing.\n\
         \n\
         With the Arduino IDE\n\
         --------------------\n\
         1. File > Open: {sketch}/{sketch}.ino\n\
         2. Tools > Board > Boards Manager: \"esp32 by Espressif Systems\", version 3.\n\
         3. Tools > Manage Libraries:\n{libs}\
         4. Tools > Board: ESP32 Dev Module, Tools > Port: the ESP32's. Upload.\n\
         5. Tools > Serial Monitor at 115200 baud.\n\
         \n\
         Then in Roastwarden: Roasters, under \"Found, not added yet\", Set up.\n",
        name = info.name,
        summary = info.summary,
        guide = info.guide,
    );
    if req.kind == FirmwareKind::FluidBed {
        text.push_str(
            "\n\
             Safety\n\
             ------\n\
             Test it the first time with the heating element disconnected: the guide's first test\n\
             checks every cut-off before the heater gets power. A heater on mains power can start a\n\
             fire or kill: wire it safely (an enclosure, a fuse, earth), or have someone who knows\n\
             how do it.\n",
        );
    }
    text
}

fn zip(files: &[(String, String)]) -> Result<Vec<u8>> {
    use zip::{write::SimpleFileOptions, CompressionMethod, DateTime};
    let now = chrono::Local::now().naive_local();
    let stamp = {
        use chrono::{Datelike, Timelike};
        DateTime::from_date_and_time(
            now.year().clamp(1980, 2107) as u16,
            now.month() as u8,
            now.day() as u8,
            now.hour() as u8,
            now.minute() as u8,
            now.second() as u8,
        )
        .unwrap_or_default()
    };
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(stamp)
        .unix_permissions(0o644);
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in files {
        w.start_file(name.as_str(), options)
            .with_context(|| format!("zip: {name}"))?;
        w.write_all(text.as_bytes())?;
    }
    Ok(w.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn req(kind: FirmwareKind) -> FirmwareRequest {
        FirmwareRequest {
            kind,
            roaster_id: "home_roaster".into(),
            password: None,
            server_address: "192.168.1.50".into(),
            port: None,
            wifi_name: Some("Coffee \"Lab\"".into()),
            wifi_password: Some("back\\slash1".into()),
            board: ThermocoupleBoard::Max31855,
            et_probe: true,
        }
    }

    fn unzip(bytes: &[u8]) -> Vec<(String, String)> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..a.len())
            .map(|i| {
                let mut f = a.by_index(i).unwrap();
                let mut s = String::new();
                f.read_to_string(&mut s).unwrap();
                (f.name().unwrap().to_string(), s)
            })
            .collect()
    }

    fn sketch_of(fw: &Firmware) -> String {
        unzip(&fw.zip)
            .into_iter()
            .find(|(n, _)| n.ends_with(".ino"))
            .unwrap()
            .1
    }

    const DEVICES: &[(&str, &str)] = &[("home_roaster", "pw-123"), ("other", "x")];

    fn devices() -> Vec<(String, String)> {
        DEVICES
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn both_sketches_have_every_setting_the_download_fills() {
        for kind in [FirmwareKind::Starter, FirmwareKind::FluidBed] {
            let mut r = req(kind);
            r.board = ThermocoupleBoard::Max6675;
            r.et_probe = false;
            let fw = build(&r, &devices(), 1883).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(fw.missing.is_empty(), "{kind:?}: {:?}", fw.missing);
        }
    }

    #[test]
    fn the_settings_are_filled_in_and_escaped() {
        let fw = build(&req(FirmwareKind::FluidBed), &devices(), 1884).unwrap();
        let s = sketch_of(&fw);
        assert!(s.contains("const char* WIFI_NAME = \"Coffee \\\"Lab\\\"\";\n"));
        assert!(s.contains("const char* WIFI_PASSWORD = \"back\\\\slash1\";\n"));
        assert!(s.contains("const char* SERVER_ADDRESS = \"192.168.1.50\";\n"));
        assert!(s.contains("const char* ROASTER_ID = \"home_roaster\";\n"));
        // From MQTT_DEVICES
        assert!(s.contains("const char* ROASTER_PASSWORD = \"pw-123\";\n"));
        // The comment after the value stays
        assert!(s.contains("const int SERVER_PORT = 1884;  // change it only"));
        assert!(s.contains("\n#define BOARD_MAX31855\n// #define BOARD_MAX6675\n"));
        assert!(s.contains("const int PIN_ET_SELECT = 17;"));
        assert_eq!(fw.file_name, "roastwarden-fluid-bed-home_roaster.zip");
    }

    #[test]
    fn the_board_and_a_single_probe_switch_their_lines() {
        let mut r = req(FirmwareKind::Starter);
        r.board = ThermocoupleBoard::Max6675;
        r.et_probe = false;
        r.port = Some(1990);
        let fw = build(&r, &devices(), 1883).unwrap();
        let s = sketch_of(&fw);
        assert!(s.contains("\n// #define BOARD_MAX31855\n#define BOARD_MAX6675\n"));
        assert!(s.contains("const int PIN_ET_SELECT = -1;  // CS of the board"));
        assert!(s.contains("const int SERVER_PORT = 1990;"));
        let files = unzip(&fw.zip);
        let ini = &files.iter().find(|(n, _)| n.ends_with(".ini")).unwrap().1;
        assert!(ini.contains("adafruit/MAX6675 library"));
        assert!(!ini.contains("MAX31855"));
        assert!(
            !ini.contains("ArduinoJson"),
            "the starter needs no JSON library"
        );
    }

    #[test]
    fn the_zip_is_a_platformio_project_the_arduino_ide_opens_too() {
        let fw = build(&req(FirmwareKind::FluidBed), &devices(), 1883).unwrap();
        let names: Vec<String> = unzip(&fw.zip).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            [
                "roastwarden-fluid-bed-home_roaster/roastwarden-fluid-bed/roastwarden-fluid-bed.ino",
                "roastwarden-fluid-bed-home_roaster/platformio.ini",
                "roastwarden-fluid-bed-home_roaster/README.txt",
            ]
        );
        let files = unzip(&fw.zip);
        let ini = &files[1].1;
        assert!(ini.contains("src_dir = roastwarden-fluid-bed\n"));
        assert!(ini.contains(&format!("platform = {PLATFORM}\n")));
        assert!(ini.contains("upload_port = home_roaster.local"));
        let readme = &files[2].1;
        assert!(readme.contains("roastwarden-fluid-bed/roastwarden-fluid-bed.ino"));
        assert!(readme.contains("YOUR SETTINGS 4 to 7"));
    }

    #[test]
    fn what_the_server_does_not_know_stays_a_placeholder() {
        let mut r = req(FirmwareKind::Starter);
        r.roaster_id = "new_one".into();
        r.wifi_name = None;
        r.wifi_password = None;
        let fw = build(&r, &devices(), 1883).unwrap();
        assert_eq!(fw.missing, ["password", "wifi"]);
        let s = sketch_of(&fw);
        assert!(s.contains("const char* ROASTER_PASSWORD = \"paste-the-password-here\";"));
        assert!(s.contains("const char* WIFI_NAME = \"your-wifi-name\";"));
        let readme = &unzip(&fw.zip)[2].1;
        assert!(readme.contains("new_one's MQTT password"), "{readme}");
        assert!(
            readme.contains("after \"new_one:\" in MQTT_DEVICES"),
            "{readme}"
        );
        // A password given wins over MQTT_DEVICES
        r.roaster_id = "home_roaster".into();
        r.password = Some("mine".into());
        let s = sketch_of(&build(&r, &devices(), 1883).unwrap());
        assert!(s.contains("const char* ROASTER_PASSWORD = \"mine\";"));
    }

    #[test]
    fn bad_input_is_refused_before_it_reaches_the_sketch() {
        let bad = [
            ("roaster_id", "a/b"),
            ("roaster_id", ""),
            ("server_address", "http://192.168.1.50"),
            ("server_address", "1.2.3.4:1883"),
            ("server_address", "a\"; evil();"),
            ("wifi_name", "line\nbreak"),
            ("wifi_password", "short"),
            (
                "wifi_name",
                "a name that is far longer than thirty-two bytes",
            ),
        ];
        for (field, value) in bad {
            let mut r = req(FirmwareKind::Starter);
            match field {
                "roaster_id" => r.roaster_id = value.into(),
                "server_address" => r.server_address = value.into(),
                "wifi_name" => r.wifi_name = Some(value.into()),
                _ => r.wifi_password = Some(value.into()),
            }
            let e = build(&r, &devices(), 1883).err().map(|e| e.to_string());
            assert!(
                e.as_deref().is_some_and(|m| m.starts_with(field)),
                "{field} {value:?}: {e:?}"
            );
        }
        // An open network: no password
        let mut r = req(FirmwareKind::Starter);
        r.wifi_password = None;
        assert!(build(&r, &devices(), 1883).is_ok());
    }

    #[test]
    fn the_platform_is_the_one_the_examples_build_with() {
        let script = include_str!("../../../../examples/build.sh");
        assert!(
            script.contains(&format!("PLATFORM={PLATFORM}\n")),
            "examples/build.sh builds with another platform than firmware.rs"
        );
    }

    #[test]
    fn the_libraries_are_the_ones_the_examples_build_with() {
        let script = include_str!("../../../../examples/build.sh");
        for kind in [FirmwareKind::Starter, FirmwareKind::FluidBed] {
            for board in [ThermocoupleBoard::Max31855, ThermocoupleBoard::Max6675] {
                let mut r = req(kind);
                r.board = board;
                for lib in libraries(&r) {
                    assert!(script.contains(lib), "{lib} is not in examples/build.sh");
                }
            }
        }
    }
}
