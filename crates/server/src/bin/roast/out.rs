//! How `roast` answers: one JSON document on stdout
//! when stdout is not a terminal (or with `--json`), short text otherwise; errors as
//! `{"error": {code, message, hint, …}}`; and exit codes an agent can branch on.

use serde::Serialize;
use serde_json::{Value, json};
use std::io::IsTerminal;

/// Exit codes.
pub mod code {
    /// Done
    pub const OK: u8 = 0;
    /// The call worked and its answer is a failure (the server is not ready, a backup
    /// failed, an approval was denied)
    pub const FAILED: u8 = 1;
    /// Bad arguments (clap's own code)
    pub const USAGE: u8 = 2;
    /// Waiting on a human: an approval to give
    pub const NEEDS_HUMAN: u8 = 3;
    /// No such roast, bean, profile, roaster or file
    pub const NOT_FOUND: u8 = 4;
    /// Not authenticated, or the token may not do that
    pub const REFUSED: u8 = 5;
    /// The request was invalid or conflicts with the current state
    pub const INVALID: u8 = 6;
    /// Roastwarden could not be reached, or failed itself
    pub const UNAVAILABLE: u8 = 7;
}

/// Why a command did not get an answer it could print.
#[derive(Debug)]
pub struct Failure {
    /// The API's error code (`NOT_FOUND`, `TOKEN_SCOPE`, `REAL_HARDWARE`, `CONTROL_OFF`, …), or roast's own
    /// (`UNREACHABLE`, `AMBIGUOUS`, `BAD_RESPONSE`, `FAILED`, …)
    pub code: String,
    pub message: String,
    /// HTTP status, when there was one
    pub status: Option<u16>,
    /// The API's `error.details`, or what roast knows that helps (e.g. the names it knows)
    pub details: Option<Box<Value>>,
}

impl Failure {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            status: None,
            details: None,
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self.code.as_str() {
            "USAGE" => code::USAGE,
            "APPROVAL_PENDING" => code::NEEDS_HUMAN,
            "NOT_FOUND" | "NO_FILE" | "LINK_NOT_ACTIVE" => code::NOT_FOUND,
            "UNAUTHORIZED"
            | "FORBIDDEN"
            | "TOKEN_INVALID"
            | "TOKEN_EXPIRED"
            | "TOKEN_REVOKED"
            | "TOKEN_SCOPE"
            | "TOO_MANY_FAILURES"
            | "OWNER_ONLY"
            | "REAL_HARDWARE"
            | "CONTROL_OFF"
            | "OUTPUT_NOT_DECLARED" => code::REFUSED,
            "UNREACHABLE" | "INTERNAL_ERROR" | "UNAVAILABLE" | "BAD_RESPONSE" => code::UNAVAILABLE,
            "FAILED" | "NOT_READY" => code::FAILED,
            _ => match self.status {
                Some(s) if s >= 500 => code::UNAVAILABLE,
                Some(401 | 403) => code::REFUSED,
                Some(404) => code::NOT_FOUND,
                _ => code::INVALID,
            },
        }
    }

    /// What to do about it, for a reader that has no other context
    pub fn hint(&self) -> String {
        match self.code.as_str() {
            "UNAUTHORIZED" => "Set ROASTWARDEN_TOKEN to an API token (Roastwarden: Settings → Tokens) and ROASTWARDEN_URL to the server".into(),
            "TOKEN_INVALID" => "The token is not one Roastwarden knows; check ROASTWARDEN_TOKEN (rr_ and 40 letters and digits)".into(),
            "TOO_MANY_FAILURES" => "Too many wrong keys or tokens came from this address; wait the seconds named, then check ROASTWARDEN_TOKEN before retrying (do not loop)".into(),
            "TOKEN_EXPIRED" | "TOKEN_REVOKED" => "Ask the owner for a new token (Settings → Tokens)".into(),
            "TOKEN_SCOPE" | "FORBIDDEN" => "This token's preset does not cover that; the owner can make a token with another preset, or do it in the app".into(),
            "OWNER_ONLY" => "Only the owner key may do that (tokens, the per-roaster switch, restoring a backup); ask the owner".into(),
            "REAL_HARDWARE" => "This roaster is real hardware and the owner has not allowed tokens to control it (Roasters → the roaster → Control). Do not work around it: ask the owner, or use virtual_roaster".into(),
            "CONTROL_OFF" => "The roaster's control level does not allow that (off logs only; manual is heater, fan and STOP): see `roast roaster show <roaster>`. Only the owner changes it (Roasters → the roaster → Control → Setup); tell them, do not work around it".into(),
            "OUTPUT_NOT_DECLARED" => "The roaster has not declared that output (a logger has none; a roaster may have a heater and no fan): see `roast roaster show <roaster>`, its `outputs`. Only the owner changes them (Roasters → the roaster → Control → Setup); tell them, do not work around it".into(),
            // routes/settings.rs: a key the server does not read, with nothing stored
            "NOT_FOUND" if self.message.starts_with("No setting ") => "Check the key's spelling; `roast settings` lists the ones that have values, docs/cli.md and the skill name the common ones".into(),
            "NOT_FOUND" => "List what exists with `roast roasts`, `roast beans`, `roast profiles` or `roast plan`".into(),
            "LINK_NOT_ACTIVE" => "The share link was revoked or never existed; ask whoever shared the profile for a new link".into(),
            "AMBIGUOUS" => "Use the id instead of the name (the list commands show both)".into(),
            "CONFLICT" => "The change conflicts with the current state (a roast already running, a name taken); look first, then retry".into(),
            "ALREADY_MARKED" => "The roast has its CHARGE or DROP already (an auto DROP, another screen): `roast show <roast>` has its events; `roast landmark <roast> drop --at M:SS` moves it".into(),
            "APPROVAL_PENDING" => "The owner approves or denies it in Roastwarden (the link went out through the notification channels); run the same command with --wait to wait for the answer".into(),
            "UNREACHABLE" => "Roastwarden did not answer; check ROASTWARDEN_URL and that the server runs (`roast health`)".into(),
            "NOT_READY" => "The server runs but MQTT or the database is not ready; see `roast status`".into(),
            "BAD_RESPONSE" => "The server answered something roast does not understand; are the two the same version? (`roast status`)".into(),
            "BAD_REQUEST" | "UNPROCESSABLE" | "USAGE" => "Check the arguments; `roast <command> --help` has examples".into(),
            "NO_FILE" => "Check the path; `roast backups` lists the copies (in the backups folder beside the database)".into(),
            _ => "See `roast <command> --help`".into(),
        }
    }
}

/// Where the answer goes and in which form.
pub struct Out {
    pub json: bool,
    /// Indented JSON: only for a person who asked for `--json` on a terminal. A pipe (an
    /// agent, a script) gets one compact line.
    pretty: bool,
}

/// Leave out what costs an agent tokens and says nothing: every `null`, at any depth. A key
/// that is absent means the same as `null` in every answer.
pub fn slim(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(slim);
        }
        Value::Array(items) => items.iter_mut().for_each(slim),
        _ => {}
    }
}

impl Out {
    pub fn new(force_json: bool, force_text: bool) -> Self {
        let terminal = std::io::stdout().is_terminal();
        Self {
            json: force_json || (!force_text && !terminal),
            pretty: terminal,
        }
    }

    fn print_json(&self, mut value: Value) {
        slim(&mut value);
        let text = if self.pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        };
        println!(
            "{}",
            text.unwrap_or_else(|e| json!({ "error": e.to_string() }).to_string())
        );
    }

    /// Print a result: the JSON of `value` (without nulls), or the text `human` makes of it.
    pub fn emit<T: Serialize>(&self, value: &T, human: impl FnOnce(&T) -> String) {
        if self.json {
            match serde_json::to_value(value) {
                Ok(v) => self.print_json(v),
                Err(e) => println!("{}", json!({ "error": e.to_string() })),
            }
        } else {
            let text = human(value);
            if !text.is_empty() {
                println!("{text}");
            }
        }
    }

    /// Print a failure and give its exit code.
    pub fn fail(&self, f: &Failure) -> u8 {
        if self.json {
            let mut error = json!({ "code": f.code, "message": f.message, "hint": f.hint() });
            if let Some(s) = f.status {
                error["status"] = json!(s);
            }
            if let Some(d) = &f.details {
                error["details"] = (**d).clone();
            }
            self.print_json(json!({ "error": error }));
        } else {
            eprintln!("error: {}", f.message);
            eprintln!("hint: {}", f.hint());
        }
        f.exit_code()
    }
}

/// A list is for finding things: leave out what the `show` of one item has,
/// such as timestamps and long descriptions.
pub fn for_finding(mut list: Value, drop: &[&str]) -> Value {
    for item in list.as_array_mut().into_iter().flatten() {
        if let Some(o) = item.as_object_mut() {
            for key in drop {
                o.remove(*key);
            }
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slim_drops_nulls_at_any_depth_and_keeps_false_and_empty() {
        let mut v = json!({
            "a": null, "b": false, "c": [], "d": [{ "e": null, "f": 0 }], "g": { "h": null }
        });
        slim(&mut v);
        assert_eq!(
            v,
            json!({ "b": false, "c": [], "d": [{ "f": 0 }], "g": {} })
        );
    }

    #[test]
    fn exit_codes_follow_the_error_kind() {
        let f = |c: &str, s: Option<u16>| Failure {
            status: s,
            ..Failure::new(c, "x")
        };
        assert_eq!(
            f("APPROVAL_PENDING", Some(202)).exit_code(),
            code::NEEDS_HUMAN
        );
        assert_eq!(f("TOKEN_SCOPE", Some(403)).exit_code(), code::REFUSED);
        assert_eq!(f("REAL_HARDWARE", Some(403)).exit_code(), code::REFUSED);
        assert_eq!(f("OWNER_ONLY", Some(403)).exit_code(), code::REFUSED);
        assert_eq!(f("CONTROL_OFF", Some(409)).exit_code(), code::REFUSED);
        assert_eq!(
            f("OUTPUT_NOT_DECLARED", Some(409)).exit_code(),
            code::REFUSED
        );
        assert_eq!(f("NOT_FOUND", Some(404)).exit_code(), code::NOT_FOUND);
        assert_eq!(f("CONFLICT", Some(409)).exit_code(), code::INVALID);
        assert_eq!(f("BAD_REQUEST", Some(400)).exit_code(), code::INVALID);
        assert_eq!(f("UNREACHABLE", None).exit_code(), code::UNAVAILABLE);
        assert_eq!(f("SOMETHING", Some(502)).exit_code(), code::UNAVAILABLE);
        assert_eq!(f("FAILED", None).exit_code(), code::FAILED);
    }

    #[test]
    fn the_real_hardware_hint_says_not_to_work_around_it() {
        assert!(
            Failure::new("REAL_HARDWARE", "x")
                .hint()
                .contains("Do not work around it")
        );
    }

    #[test]
    fn lists_leave_out_what_a_show_has() {
        let list = json!([{ "id": "a", "name": "n", "created_at": "t", "description": "long" }]);
        assert_eq!(
            for_finding(list, &["created_at", "description"]),
            json!([{ "id": "a", "name": "n" }])
        );
    }
}
