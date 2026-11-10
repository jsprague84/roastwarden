//! `roast roaster firmware` and `roaster password`: an ESP32's sketch with its settings filled in, and
//! who may sign in to the built-in MQTT broker as a roaster.

use std::fmt::Write as _;

use clap::Subcommand;
use reqwest::Method;
use serde_json::{Value, json};

use crate::api::{Api, enc};
use crate::out::{Failure, Out, code};

#[derive(Subcommand, Debug)]
pub enum PasswordCommand {
    /// Every roaster that may sign in: from .env or added here, connected now, set up
    List,
    /// A new roaster's password; printed once (`roaster firmware ID` fills it in later)
    Add {
        /// Its id, as it will be in Roastwarden (its MQTT user): letters, digits, _ and -
        roaster: String,
        /// Read the password from stdin (one line); default a random one
        #[arg(long)]
        password_stdin: bool,
    },
    /// A new password for one added here; connected, it is signed out until it has the new one
    New {
        roaster: String,
        /// Read the password from stdin (one line); default a random one
        #[arg(long)]
        password_stdin: bool,
    },
    /// Remove one added here: it is signed out and can no longer sign in
    Remove { roaster: String },
}

/// `roast roaster firmware`: what this server can fill in.
pub async fn firmware_info(api: &Api, out: &Out) -> Result<u8, Failure> {
    let info = api.get("/api/firmware").await?;
    out.emit(&info, |i| {
        let mut s = String::new();
        for k in i["kinds"].as_array().into_iter().flatten() {
            let _ = write!(
                s,
                "{:<10} {}\n           {}\n",
                k["kind"].as_str().unwrap_or("?").replace('_', "-"),
                k["name"].as_str().unwrap_or(""),
                k["guide"].as_str().unwrap_or("")
            );
        }
        let _ = writeln!(
            s,
            "MQTT port {}{}",
            i["port"],
            if i["built_in_broker"].as_bool() == Some(true) {
                " (the built-in broker)"
            } else {
                ""
            }
        );
        let ids: Vec<String> = i["roasters"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let id = r["id"].as_str().unwrap_or("?");
                if r["added"].as_bool() == Some(true) {
                    format!("{id} (added)")
                } else {
                    id.to_string()
                }
            })
            .collect();
        if ids.is_empty() {
            s.push_str(
                "No roaster passwords here: `roast roaster password add ID` (the built-in broker), \
                 or give the roaster's password with --password-stdin",
            );
        } else {
            let _ = write!(s, "Passwords filled in for: {}", ids.join(", "));
        }
        s
    });
    Ok(code::OK)
}

/// Who may sign in to the built-in broker, and adding, changing or removing a roaster's
/// password.
pub async fn password(api: &Api, out: &Out, action: PasswordCommand) -> Result<u8, Failure> {
    let given = |id: &str, v: &Value, what: &str| {
        let pw = v["password"].as_str().unwrap_or("?");
        format!(
            "{id}'s MQTT password ({what}): {pw}\nShown once. `roast roaster firmware {id}` fills \
             it into an ESP32's sketch."
        )
    };
    let own = |stdin: bool| -> Result<Option<String>, Failure> {
        if !stdin {
            return Ok(None);
        }
        let p = stdin_line("the password")?;
        if p.is_empty() {
            return Err(Failure::new(
                "USAGE",
                "--password-stdin: no password on stdin",
            ));
        }
        Ok(Some(p))
    };
    match action {
        PasswordCommand::List => {
            let v = api.get("/api/mqtt/passwords").await?;
            out.emit(&v, |v| {
                let mut s = String::new();
                if v["built_in_broker"].as_bool() != Some(true) {
                    s.push_str(
                        "This server uses an MQTT broker of its own: a roaster's user and password \
                         are added there.\n",
                    );
                }
                let rows = v["passwords"].as_array().cloned().unwrap_or_default();
                if rows.is_empty() {
                    s.push_str("No roaster may sign in yet: `roast roaster password add ID`.");
                }
                for r in rows {
                    let mut notes = vec![if r["source"] == "env" {
                        "MQTT_DEVICES (.env)".to_string()
                    } else {
                        format!(
                            "added {}",
                            r["created_at"]
                                .as_str()
                                .unwrap_or("?")
                                .get(..10)
                                .unwrap_or("?")
                        )
                    }];
                    if r["signed_in"].as_bool() == Some(true) {
                        notes.push("signed in".into());
                    }
                    if r["added"].as_bool() != Some(true) {
                        notes.push("not set up under Roasters yet".into());
                    }
                    let _ = writeln!(
                        s,
                        "{:<20} {}",
                        r["roaster_id"].as_str().unwrap_or("?"),
                        notes.join(", ")
                    );
                }
                s.trim_end().to_string()
            });
        }
        PasswordCommand::Add {
            roaster,
            password_stdin,
        } => {
            let mut body = json!({ "roaster_id": roaster });
            if let Some(p) = own(password_stdin)? {
                body["password"] = json!(p);
            }
            let v = api
                .call(Method::POST, "/api/mqtt/passwords", Some(&body))
                .await?;
            out.emit(&v, |v| given(&roaster, v, "it may sign in now"));
        }
        PasswordCommand::New {
            roaster,
            password_stdin,
        } => {
            let body = match own(password_stdin)? {
                Some(p) => json!({ "password": p }),
                None => json!({}),
            };
            let v = api
                .call(
                    Method::PUT,
                    &format!("/api/mqtt/passwords/{}", enc(&roaster)),
                    Some(&body),
                )
                .await?;
            out.emit(&v, |v| {
                given(&roaster, v, "new; signed out until its sketch has it")
            });
        }
        PasswordCommand::Remove { roaster } => {
            api.call(
                Method::DELETE,
                &format!("/api/mqtt/passwords/{}", enc(&roaster)),
                None,
            )
            .await?;
            let v = json!({ "removed": roaster });
            out.emit(&v, |_| {
                format!("{roaster} can no longer sign in to the broker (its roasts and setup stay)")
            });
        }
    }
    Ok(code::OK)
}

pub struct FirmwareArgs {
    pub roaster: String,
    pub kind: String,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub wifi: Option<String>,
    pub wifi_password_stdin: bool,
    pub password_stdin: bool,
    pub board: String,
    pub no_et: bool,
    pub output: Option<std::path::PathBuf>,
}

fn stdin_line(what: &str) -> Result<String, Failure> {
    let mut s = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
        .map_err(|e| Failure::new("USAGE", format!("could not read {what} from stdin: {e}")))?;
    Ok(s.lines().next().unwrap_or("").to_string())
}

/// `roast roaster firmware ID`: the zip, written to a file.
pub async fn firmware(api: &Api, out: &Out, f: FirmwareArgs) -> Result<u8, Failure> {
    let address = match f.address.or_else(|| api.host()) {
        Some(a) if a != "localhost" && a != "127.0.0.1" && a != "::1" => a,
        _ => {
            return Err(Failure::new(
                "USAGE",
                "--address: the server's address on the network (192.168.1.50), which the ESP32 \
                 connects to; roast calls it at localhost",
            ));
        }
    };
    let mut body = json!({
        "kind": f.kind.replace('-', "_"),
        "roaster_id": f.roaster,
        "server_address": address,
        "board": f.board,
        "et_probe": !f.no_et,
    });
    if let Some(p) = f.port {
        body["port"] = json!(p);
    }
    if let Some(w) = &f.wifi {
        body["wifi_name"] = json!(w);
        body["wifi_password"] = json!(if f.wifi_password_stdin {
            stdin_line("the Wi-Fi password")?
        } else {
            String::new()
        });
    }
    if f.password_stdin {
        let pw = stdin_line("the roaster's password")?;
        if pw.is_empty() {
            return Err(Failure::new(
                "USAGE",
                "--password-stdin read an empty password",
            ));
        }
        body["password"] = json!(pw);
    }
    let (bytes, name, headers) = api
        .transfer_with_headers(
            Method::POST,
            "/api/firmware",
            crate::api::Upload::Json(&body),
        )
        .await?;
    let missing: Vec<String> = headers
        .get("x-firmware-missing")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if f.output.as_deref() == Some(std::path::Path::new("-")) {
        use std::io::Write;
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| Failure::new("FAILED", e.to_string()))?;
        return Ok(code::OK);
    }
    let file = f.output.unwrap_or_else(|| {
        let n = name.unwrap_or_else(|| "roastwarden-firmware.zip".into());
        std::path::PathBuf::from(
            n.rsplit(['/', '\\'])
                .next()
                .unwrap_or("roastwarden-firmware.zip"),
        )
    });
    std::fs::write(&file, &bytes)
        .map_err(|e| Failure::new("FAILED", format!("could not write {}: {e}", file.display())))?;
    let answer =
        json!({ "file": file.display().to_string(), "bytes": bytes.len(), "missing": missing });
    out.emit(&answer, |a| {
        let mut s = format!(
            "wrote {} ({} bytes)",
            a["file"].as_str().unwrap_or("?"),
            a["bytes"]
        );
        let m: Vec<&str> = a["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .collect();
        if !m.is_empty() {
            let _ = write!(s, "; still to fill in by hand: {}", m.join(", "));
        }
        s
    });
    Ok(code::OK)
}
