//! The built-in MQTT broker (docs/DEPLOY.md, "One container"): with
//! `ROASTWARDEN_BROKER_ADDR` set (`0.0.0.0:1883`) the server runs its own broker, rmqtt as a library,
//! so an install is one container: no mosquitto beside it. Off by default; a deployment with its
//! own broker (Home Assistant's, a mosquitto of its own) keeps it.
//!
//! The same rules as the compose file's mosquitto (docker/mosquitto/acl.in; SECURITY.md, "MQTT"):
//! no anonymous clients; the server signs in as `roastwarden` and may use every topic; each roaster
//! in `MQTT_DEVICES` (`device_id:password,…`) or added from the dashboard (`mqtt_passwords.rs`)
//! signs in as its device id and may use `roaster/<id>/#` and read the server's presence.
//! Retained messages are kept in memory (the roasters send their status again when they
//! reconnect after a restart).
//!
//! A roaster's sign-in is checked by the server's own hook ([`RoasterSignIn`]), ahead of the
//! ACL plugin, against the live list: a password added, changed or removed counts at the next
//! connect, without a restart. The ACL plugin's rules keep the server's and Home Assistant's
//! users and every topic rule.
//!
//! What is lost against a separate broker: the server's last will. A broker in the server's own
//! process goes down with it, so the roasters see the connection drop rather than "offline" on
//! `roastwarden/server/status`; the firmware treats both the same (its heater stops), but a hung
//! server whose broker still answers would not be noticed. Owners who want that keep mosquitto.

use std::fmt::Write as _;
use std::net::SocketAddr;

use anyhow::{Context, Result, bail};

/// The server's own user on the broker (`MQTT_USERNAME` for an outside broker)
pub const SERVER_USER: &str = "roastwarden";

#[derive(Clone)]
pub struct BrokerConfig {
    pub addr: SocketAddr,
    /// The server's password: `MQTT_PASSWORD`, else a random one made at start (nobody else
    /// needs it: the server is the only `roastwarden` client)
    pub server_password: String,
    /// The roasters that may sign in: (device id, password)
    pub devices: Vec<(String, String)>,
    /// Home Assistant's password (`MQTT_HOME_ASSISTANT_PASSWORD`): it signs in as
    /// `homeassistant` and reads the discovery and state topics alone (`home_assistant.rs`)
    pub home_assistant_password: Option<String>,
}

impl std::fmt::Debug for BrokerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerConfig")
            .field("addr", &self.addr)
            .field(
                "devices",
                &self.devices.iter().map(|(d, _)| d).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

/// What `/api/about` says of the broker (`roast status`)
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
#[expect(
    clippy::doc_markdown,
    reason = "OpenAPI description: a change here changes docs/openapi.json"
)]
pub struct BrokerInfo {
    /// Where the built-in broker listens (ROASTWARDEN_BROKER_ADDR)
    pub addr: String,
    /// The roasters that may sign in (MQTT_DEVICES and Roasters → MQTT passwords), by device id
    pub devices: Vec<String>,
    /// Home Assistant may sign in as `homeassistant` (MQTT_HOME_ASSISTANT_PASSWORD is set)
    pub home_assistant: bool,
}

/// Where `ROASTWARDEN_BROKER_ADDR` asks the built-in broker to listen. Unset, empty or `off`: no
/// built-in broker. `off` is there for `docker-compose.single.yml`, whose
/// `${ROASTWARDEN_BROKER_ADDR:-0.0.0.0:1883}` turns an empty value in `.env` into the default:
/// an owner with a broker of their own (Home Assistant's, their Mosquitto) writes `off`.
fn wanted_addr(value: Option<&str>) -> Result<Option<SocketAddr>> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if v.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    v.parse().map(Some).with_context(|| {
        format!(
            "ROASTWARDEN_BROKER_ADDR: \"{v}\" is not an address and port (0.0.0.0:1883), or off"
        )
    })
}

impl BrokerConfig {
    /// The built-in broker as the environment asks for it: `None` when `ROASTWARDEN_BROKER_ADDR` is
    /// unset, empty or `off` (a broker of the owner's own: `MQTT_BROKER_HOST`).
    pub fn from_env() -> Result<Option<Self>> {
        let Some(addr) = wanted_addr(std::env::var("ROASTWARDEN_BROKER_ADDR").ok().as_deref())?
        else {
            return Ok(None);
        };
        let server_password = match std::env::var("MQTT_PASSWORD")
            .ok()
            .filter(|v| !v.is_empty())
        {
            Some(p) => p,
            None => random_password()?,
        };
        let devices = parse_devices(&std::env::var("MQTT_DEVICES").unwrap_or_default())?;
        let home_assistant_password = std::env::var("MQTT_HOME_ASSISTANT_PASSWORD")
            .ok()
            .filter(|v| !v.trim().is_empty());
        Ok(Some(Self {
            addr,
            server_password,
            devices,
            home_assistant_password,
        }))
    }

    pub fn info(&self) -> BrokerInfo {
        BrokerInfo {
            addr: self.addr.to_string(),
            devices: self.devices.iter().map(|(d, _)| d.clone()).collect(),
            home_assistant: self.home_assistant_password.is_some(),
        }
    }

    /// Where the server's own client connects: the broker's port on this machine
    pub fn local_addr(&self) -> SocketAddr {
        let ip: std::net::IpAddr = if self.addr.is_ipv6() {
            std::net::Ipv6Addr::LOCALHOST.into()
        } else {
            std::net::Ipv4Addr::LOCALHOST.into()
        };
        // A broker bound to one address only answers there
        let ip = if self.addr.ip().is_unspecified() {
            ip
        } else {
            self.addr.ip()
        };
        SocketAddr::new(ip, self.addr.port())
    }
}

fn random_password() -> Result<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("no random bytes: {e}"))?;
    Ok(b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    }))
}

/// `MQTT_DEVICES` as users.sh reads it: `device_id:password` pairs, comma separated.
pub fn parse_devices(s: &str) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let Some((id, pw)) = pair.split_once(':') else {
            bail!("MQTT_DEVICES: \"{pair}\" needs device_id:password");
        };
        let (id, pw) = (id.trim(), pw.trim());
        if id.is_empty() || pw.is_empty() {
            bail!("MQTT_DEVICES: \"{pair}\" needs device_id:password");
        }
        // A topic level: no wildcards or separators (the ACL puts it in roaster/<id>/#)
        if id.contains(['/', '+', '#']) {
            bail!("MQTT_DEVICES: the device id \"{id}\" may not contain /, + or #");
        }
        if id == SERVER_USER {
            bail!("MQTT_DEVICES: \"{SERVER_USER}\" is the server's own user");
        }
        if id == crate::notify::home_assistant::HA_USER {
            bail!("MQTT_DEVICES: \"{id}\" is Home Assistant's user (MQTT_HOME_ASSISTANT_PASSWORD)");
        }
        if out.iter().any(|(d, _)| d == id) {
            bail!("MQTT_DEVICES: \"{id}\" is listed twice");
        }
        out.push((id.to_string(), pw.to_string()));
    }
    Ok(out)
}

/// A TOML string (a JSON string is a valid TOML basic string)
fn toml_str(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

/// The rmqtt-acl rules: acl.in's, plus the server's and Home Assistant's passwords. Top to
/// bottom, the first rule that matches decides; a connect rule with a password matches only
/// with that password. The roasters' sign-ins never reach these rules: [`RoasterSignIn`]
/// decides them first.
pub fn acl_rules(cfg: &BrokerConfig) -> String {
    let mut r = String::from("disconnect_if_pub_rejected = false\nrules = [\n");
    // Who may sign in
    let _ = writeln!(
        r,
        "  [\"allow\", {{ user = {}, password = {}, superuser = true }}, \"connect\"],",
        toml_str(SERVER_USER),
        toml_str(&cfg.server_password)
    );
    let ha = crate::notify::home_assistant::HA_USER;
    if let Some(pw) = &cfg.home_assistant_password {
        let _ = writeln!(
            r,
            "  [\"allow\", {{ user = {}, password = {} }}, \"connect\"],",
            toml_str(ha),
            toml_str(pw)
        );
    }
    r.push_str("  [\"deny\", \"all\", \"connect\"],\n");
    // The server: every topic (a roaster with topics of its own, the simulated drum's sim/drum/)
    let _ = writeln!(
        r,
        "  [\"allow\", {{ user = {} }}, \"pubsub\", [\"#\"]],",
        toml_str(SERVER_USER)
    );
    // Home Assistant: reads the discovery, the roasters' state and the server's presence;
    // sends its own birth message alone. Never a roaster's topics.
    if cfg.home_assistant_password.is_some() {
        let _ = write!(
            r,
            "  [\"allow\", {{ user = {u} }}, \"subscribe\", [\"homeassistant/#\", \"{root}/#\", {st}]],\n\
             \x20 [\"allow\", {{ user = {u} }}, \"publish\", [\"homeassistant/status\"]],\n\
             \x20 [\"deny\", {{ user = {u} }}],\n",
            u = toml_str(ha),
            root = crate::notify::home_assistant::ROOT,
            st = toml_str(crate::SERVER_STATUS_TOPIC),
        );
    }
    // A roaster: its own topics, and the server's presence to read
    r.push_str("  [\"allow\", \"all\", \"pubsub\", [\"roaster/%u/#\"]],\n");
    let _ = writeln!(
        r,
        "  [\"allow\", \"all\", \"subscribe\", [{}]],",
        toml_str(crate::SERVER_STATUS_TOPIC)
    );
    r.push_str("  [\"deny\", \"all\"]\n]\n");
    r
}

const RETAINER: &str = "storage.type = \"ram\"\nmax_retained_messages = 10000\n\
max_payload_size = \"256KB\"\nretained_message_ttl = \"0m\"\n";

/// A roaster's sign-in, checked against the live list (`mqtt_passwords.rs`) before the ACL
/// plugin's rules: a roaster's user with its password is let in, with another password turned
/// away; any other user goes on to the ACL plugin (the server, Home Assistant, a stranger).
struct RoasterSignIn {
    passwords: crate::roasters::mqtt_passwords::MqttPasswords,
}

/// Above the ACL plugin's (10): runs first
const SIGN_IN_PRIORITY: rmqtt::hook::Priority = 20;

#[async_trait::async_trait]
impl rmqtt::hook::Handler for RoasterSignIn {
    async fn hook(
        &self,
        param: &rmqtt::hook::Parameter,
        acc: Option<rmqtt::hook::HookResult>,
    ) -> rmqtt::hook::ReturnType {
        use rmqtt::hook::{HookResult, Parameter};
        use rmqtt::types::AuthResult;
        let Parameter::ClientAuthenticate(info) = param else {
            return (true, acc);
        };
        let Some(user) = info.username() else {
            return (true, acc);
        };
        let user: &str = user.as_ref();
        let password: &[u8] = info
            .password()
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_default();
        match self.passwords.check(user, password) {
            None => (true, acc),
            Some(true) => (
                false,
                Some(HookResult::AuthResult(AuthResult::Allow(false, None))),
            ),
            Some(false) => (
                false,
                Some(HookResult::AuthResult(AuthResult::BadUsernameOrPassword)),
            ),
        }
    }
}

/// The running broker: who is connected, and signing a roaster out.
#[derive(Clone)]
pub struct Handle {
    scx: rmqtt::context::ServerContext,
}

impl Handle {
    /// The users connected now
    pub async fn signed_in(&self) -> std::collections::HashSet<String> {
        let sessions: Vec<_> = {
            let shared = self.scx.extends.shared().await;
            shared.iter().filter_map(|e| e.session()).collect()
        };
        let mut out = std::collections::HashSet::new();
        for s in sessions {
            if s.connected().await.unwrap_or_default() {
                out.insert(s.id.username_ref().to_string());
            }
        }
        out
    }

    /// Every client signed in as `user` disconnected; how many
    pub async fn sign_out(&self, user: &str) -> usize {
        let ids: Vec<rmqtt::types::Id> = {
            let shared = self.scx.extends.shared().await;
            shared
                .iter()
                .filter_map(|e| e.session())
                .filter(|s| s.id.username_ref() == user)
                .map(|s| s.id.clone())
                .collect()
        };
        let mut n = 0;
        for id in ids {
            let mut entry = self.scx.extends.shared().await.entry(id);
            match entry.kick(true, true, true).await {
                Ok(_) => n += 1,
                Err(e) => tracing::warn!(user, error = %e, "could not sign a client out"),
            }
        }
        n
    }
}

/// Start the broker: its listener is bound before this returns (an address in use is an error
/// here, not later), and it runs on a task of its own. The roasters sign in with `passwords`.
pub async fn start(
    cfg: &BrokerConfig,
    passwords: crate::roasters::mqtt_passwords::MqttPasswords,
) -> Result<Handle> {
    use rmqtt::{context::ServerContext, net::Builder, server::MqttServer};
    let scx = ServerContext::new()
        // rmqtt refuses every new connection (ServiceUnavailable) while the machine's load
        // average is over 80 or its CPU over 90 %. A roaster reconnecting mid-roast must be let
        // in however busy the computer is (a Raspberry Pi rendering a report), and a dozen
        // clients are no load to shed.
        .busy_check_enable(false)
        .plugins_config_map_add("rmqtt-acl", acl_rules(cfg))
        .plugins_config_map_add("rmqtt-retainer", RETAINER)
        .build()
        .await;
    rmqtt_acl::register(&scx, true, true)
        .await
        .map_err(|e| anyhow::anyhow!("MQTT broker: the ACL plugin: {e}"))?;
    let register = scx.extends.hook_mgr().register();
    register
        .add_priority(
            rmqtt::hook::Type::ClientAuthenticate,
            SIGN_IN_PRIORITY,
            Box::new(RoasterSignIn { passwords }),
        )
        .await;
    register.start().await;
    rmqtt_retainer::register(&scx, true, true)
        .await
        .map_err(|e| anyhow::anyhow!("MQTT broker: retained messages: {e}"))?;
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`Listener`'s own path (`rmqtt_net`) is private to rmqtt"
    )]
    let listener = Builder::new()
        .name("external/tcp")
        .laddr(cfg.addr)
        // A restart binds again at once: without it the last run's connections in TIME_WAIT
        // hold the port for a minute and the server stops with "address in use" (found on the
        // arm64 test, `docker compose restart`). Mosquitto sets it too.
        .reuseaddr(Some(true))
        .allow_anonymous(false)
        // As mosquitto.conf: a telemetry frame is about 1 KB
        .max_packet_size(256 * 1024)
        .max_connections(100)
        .bind()
        .and_then(|l| l.tcp())
        .map_err(|e| anyhow::anyhow!("MQTT broker: cannot listen on {}: {e}", cfg.addr))?;
    let handle = Handle { scx: scx.clone() };
    let server = MqttServer::new(scx).listener(listener).build();
    tokio::spawn(async move {
        if let Err(e) = server.run().await {
            tracing::error!(error = %e, "the built-in MQTT broker stopped");
        }
    });
    tracing::info!(addr = %cfg.addr, "built-in MQTT broker listening");
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
    use std::time::Duration;

    #[test]
    fn broker_addr_off_or_empty_means_none_and_a_bad_one_says_so() {
        assert_eq!(wanted_addr(None).unwrap(), None);
        assert_eq!(wanted_addr(Some("  ")).unwrap(), None);
        assert_eq!(wanted_addr(Some("off")).unwrap(), None);
        assert_eq!(wanted_addr(Some("OFF")).unwrap(), None);
        assert_eq!(
            wanted_addr(Some(" 0.0.0.0:1883 ")).unwrap(),
            Some("0.0.0.0:1883".parse().unwrap())
        );
        let e = format!("{:#}", wanted_addr(Some("yes")).unwrap_err());
        assert!(e.contains("\"yes\"") && e.contains("or off"), "{e}");
    }

    #[test]
    fn devices_are_read_as_users_sh_reads_them() {
        let d = parse_devices(" esp32_roaster_01:abc , drum:x:y,").unwrap();
        assert_eq!(
            d,
            vec![
                ("esp32_roaster_01".to_string(), "abc".to_string()),
                ("drum".to_string(), "x:y".to_string())
            ]
        );
        assert_eq!(
            parse_devices("").unwrap(),
            [] as [(std::string::String, std::string::String); 0]
        );
        for bad in ["nopass", "a:", ":pw", "a/b:pw", "roastwarden:pw", "a:1,a:2"] {
            assert!(parse_devices(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_password_with_quotes_stays_one_toml_string() {
        let cfg = BrokerConfig {
            addr: "127.0.0.1:1883".parse().unwrap(),
            server_password: "s\"e\\c".into(),
            devices: vec![("d1".into(), "p\"w".into())],
            home_assistant_password: None,
        };
        let rules = acl_rules(&cfg);
        assert!(rules.contains(r#"password = "s\"e\\c""#), "{rules}");
        // The roasters are not among the rules: RoasterSignIn checks them
        assert!(!rules.contains(r#"user = "d1""#), "{rules}");
        assert!(
            rules.trim_end().ends_with("[\"deny\", \"all\"]\n]"),
            "{rules}"
        );
    }

    #[test]
    fn the_server_connects_on_this_machine() {
        let mut cfg = BrokerConfig {
            addr: "0.0.0.0:1884".parse().unwrap(),
            server_password: "x".into(),
            devices: vec![],
            home_assistant_password: None,
        };
        assert_eq!(cfg.local_addr().to_string(), "127.0.0.1:1884");
        cfg.addr = "192.168.1.5:1883".parse().unwrap();
        assert_eq!(cfg.local_addr().to_string(), "192.168.1.5:1883");
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// `start` returns before rmqtt listens: wait for the port to take a connection, so the
    /// first client is not refused on a loaded machine (a CI run beside a build took seconds).
    async fn listening(port: u16) {
        for _ in 0..200 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the broker did not listen on 127.0.0.1:{port} within 10 s");
    }

    /// A client's first answers: whether it was let in, then the messages it receives in `wait`
    async fn client(
        port: u16,
        user: &str,
        pw: &str,
        subscribe: &[&str],
        publish: &[(&str, &str, bool)],
        wait: Duration,
    ) -> (bool, Vec<(String, String)>) {
        let mut o = MqttOptions::new(format!("t-{user}-{}", free_port()), "127.0.0.1", port);
        o.set_credentials(user, pw);
        o.set_keep_alive(Duration::from_secs(5));
        let (c, mut ev) = AsyncClient::new(o, 16);
        let mut got = Vec::new();
        let mut connected = false;
        // The broker's answer gets 10 s (a refusal comes back at once; a loaded CI machine, a
        // mutants run beside it, took over 2 s to say yes), then `wait` for the messages.
        let mut end = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let next = tokio::time::timeout_at(end, ev.poll()).await;
            match next {
                // Said on stderr, so a failed assertion in CI tells a refusal from silence.
                Err(_) => {
                    if !connected {
                        eprintln!("{user}: no answer from the broker within 10 s");
                    }
                    break;
                }
                Ok(Err(e)) => {
                    if !connected {
                        eprintln!("{user}: connection error before the answer: {e}");
                    }
                    break;
                }
                Ok(Ok(Event::Incoming(Packet::ConnAck(a)))) => {
                    connected = a.code == rumqttc::ConnectReturnCode::Success;
                    if !connected {
                        eprintln!("{user}: refused, {:?}", a.code);
                        break;
                    }
                    end = tokio::time::Instant::now() + wait;
                    for t in subscribe {
                        c.subscribe(*t, QoS::AtLeastOnce).await.unwrap();
                    }
                    for (t, m, retain) in publish {
                        c.publish(*t, QoS::AtLeastOnce, *retain, m.as_bytes().to_vec())
                            .await
                            .unwrap();
                    }
                }
                Ok(Ok(Event::Incoming(Packet::Publish(p)))) => got.push((
                    p.topic.clone(),
                    String::from_utf8_lossy(&p.payload).to_string(),
                )),
                Ok(Ok(_)) => {}
            }
        }
        let _ = c.disconnect().await;
        (connected, got)
    }

    /// The roasters' passwords as the server loads them: `cfg.devices` as `MQTT_DEVICES`
    async fn passwords(
        cfg: &BrokerConfig,
    ) -> (
        crate::roasters::mqtt_passwords::MqttPasswords,
        std::path::PathBuf,
    ) {
        let dir = std::env::temp_dir().join(format!("rw-broker-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::open(&dir.join("t.db").to_string_lossy())
            .await
            .unwrap();
        let p = crate::roasters::mqtt_passwords::MqttPasswords::load(db, cfg.devices.clone(), true)
            .await
            .unwrap();
        (p, dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_password_from_the_dashboard_counts_at_once_and_a_new_one_signs_out() {
        let port = free_port();
        let cfg = BrokerConfig {
            addr: format!("127.0.0.1:{port}").parse().unwrap(),
            server_password: "server-pw".into(),
            devices: vec![("r1".into(), "pw1".into())],
            home_assistant_password: None,
        };
        let (passwords, dir) = passwords(&cfg).await;
        let handle = start(&cfg, passwords.clone()).await.unwrap();
        listening(port).await;
        passwords.attach(handle.clone());
        // How long a connected client listens for messages (`client`).
        let short = Duration::from_secs(2);

        // Not known yet; added, it signs in without a restart, the old roaster too
        assert!(!client(port, "r3", "r3-password", &[], &[], short).await.0);
        passwords
            .add("r3", Some("r3-password".into()))
            .await
            .unwrap();
        assert!(client(port, "r3", "r3-password", &[], &[], short).await.0);
        assert!(!client(port, "r3", "wrong-one", &[], &[], short).await.0);
        assert!(client(port, "r1", "pw1", &[], &[], short).await.0);
        // A roaster's own password never signs in as the server
        assert!(
            !client(port, SERVER_USER, "r3-password", &[], &[], short)
                .await
                .0
        );

        // Connected, it is listed; a new password signs it out at once
        let started = tokio::time::Instant::now();
        let held = tokio::spawn(async move {
            client(
                port,
                "r3",
                "r3-password",
                &["roaster/r3/#"],
                &[],
                Duration::from_secs(6),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert!(handle.signed_in().await.contains("r3"));
        let new = passwords.change("r3", None).await.unwrap();
        let (ok, _) = held.await.unwrap();
        assert!(ok);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "signed out, not left to its 6 s: {:?}",
            started.elapsed()
        );
        assert!(!client(port, "r3", "r3-password", &[], &[], short).await.0);
        assert!(client(port, "r3", &new, &[], &[], short).await.0);

        // Removed: no longer signs in
        passwords.remove("r3").await.unwrap();
        assert!(!client(port, "r3", &new, &[], &[], short).await.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_busy_computer_does_not_turn_a_roaster_away() {
        let port = free_port();
        let cfg = BrokerConfig {
            addr: format!("127.0.0.1:{port}").parse().unwrap(),
            server_password: "server-pw".into(),
            devices: vec![("r1".into(), "pw1".into())],
            home_assistant_password: None,
        };
        let (passwords, dir) = passwords(&cfg).await;
        let handle = start(&cfg, passwords).await.unwrap();
        // With the check on, a load average over 80 or a CPU over 90 % refused every sign-in
        // (seen in CI with four test runs on one machine).
        assert!(!handle.scx.busy_check_enable);
        assert!(!handle.scx.is_busy().await);
        listening(port).await;
        assert!(
            client(port, "r1", "pw1", &[], &[], Duration::from_secs(1))
                .await
                .0
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_built_in_broker_keeps_each_roaster_to_its_own_topics() {
        let port = free_port();
        let cfg = BrokerConfig {
            addr: format!("127.0.0.1:{port}").parse().unwrap(),
            server_password: "server-pw".into(),
            devices: vec![("r1".into(), "pw1".into()), ("r2".into(), "pw2".into())],
            home_assistant_password: Some("ha-pw".into()),
        };
        let (passwords, dir) = passwords(&cfg).await;
        start(&cfg, passwords).await.unwrap();
        listening(port).await;
        // How long a connected client listens for messages (`client`).
        let short = Duration::from_secs(2);

        // Nobody without a password, nobody with the wrong one
        assert!(!client(port, "r1", "nope", &[], &[], short).await.0);
        assert!(!client(port, "stranger", "x", &[], &[], short).await.0);

        // The server's presence (retained) and a retained status from r1
        let (ok, _) = client(
            port,
            SERVER_USER,
            "server-pw",
            &[],
            &[(crate::SERVER_STATUS_TOPIC, "online", true)],
            short,
        )
        .await;
        assert!(ok);
        let (ok, _) = client(
            port,
            "r1",
            "pw1",
            &[],
            &[
                ("roaster/r1/status", "r1 online", true),
                // Not its own: dropped
                ("roaster/r2/status", "forged", true),
            ],
            short,
        )
        .await;
        assert!(ok);

        // The server sees every roaster's retained status, the forged one not among them
        let (_, got) = client(
            port,
            SERVER_USER,
            "server-pw",
            &["roaster/+/status"],
            &[],
            short,
        )
        .await;
        assert_eq!(
            got,
            vec![("roaster/r1/status".to_string(), "r1 online".to_string())],
        );

        // r2 reads the server's presence and its own topics, not r1's
        let (ok, got) = client(
            port,
            "r2",
            "pw2",
            &[crate::SERVER_STATUS_TOPIC, "roaster/r1/#", "roaster/r2/#"],
            &[],
            short,
        )
        .await;
        assert!(ok);
        assert_eq!(
            got,
            vec![(crate::SERVER_STATUS_TOPIC.to_string(), "online".to_string())],
        );

        // A command from the server reaches its roaster live
        let p2 = port;
        let listener = tokio::spawn(async move {
            client(
                p2,
                "r1",
                "pw1",
                &["roaster/r1/control/#"],
                &[],
                Duration::from_secs(2),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(600)).await;
        client(
            port,
            SERVER_USER,
            "server-pw",
            &[],
            &[("roaster/r1/control/fan", "40", false)],
            short,
        )
        .await;
        let (_, got) = listener.await.unwrap();
        assert_eq!(
            got,
            vec![("roaster/r1/control/fan".to_string(), "40".to_string())]
        );

        // Home Assistant: the discovery and the state, never a roaster's topics; it cannot
        // send a command or forge a discovery
        let ha = crate::notify::home_assistant::HA_USER;
        assert!(!client(port, ha, "wrong", &[], &[], short).await.0);
        client(
            port,
            SERVER_USER,
            "server-pw",
            &[],
            &[("homeassistant/device/roastwarden_r1/config", "{}", true)],
            short,
        )
        .await;
        let (ok, _) = client(
            port,
            ha,
            "ha-pw",
            &[],
            &[
                ("roaster/r1/control/fan", "100", false),
                ("homeassistant/device/roastwarden_r2/config", "forged", true),
                ("homeassistant/status", "online", false),
            ],
            short,
        )
        .await;
        assert!(ok);
        let (ok, got) = client(
            port,
            ha,
            "ha-pw",
            &[
                "homeassistant/+/+/config",
                "roaster/#",
                crate::SERVER_STATUS_TOPIC,
            ],
            &[],
            short,
        )
        .await;
        assert!(ok);
        let mut got = got;
        got.sort();
        assert_eq!(
            got,
            vec![
                (
                    "homeassistant/device/roastwarden_r1/config".to_string(),
                    "{}".to_string()
                ),
                (crate::SERVER_STATUS_TOPIC.to_string(), "online".to_string()),
            ],
            "Home Assistant reads the discovery and the server's presence, nothing forged"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
