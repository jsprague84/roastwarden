//! Setting `roast` up on a machine (docs/agents.md, "Setting up an agent"): the server and a
//! token kept in a file only you can read (`roast login`), so an agent's shell needs no
//! environment, and the Agent Skill written where an agent finds it (`roast skill install`).
//! The skill is built into the binary, so the two always match.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::Api;
use crate::out::{code, Failure, Out};

pub const SKILL_MD: &str = include_str!("../../../../../skills/roastwarden/SKILL.md");
pub const LABEL_DESIGN_MD: &str = include_str!("../../../../../skills/roastwarden/label-design.md");

pub const DEFAULT_URL: &str = "http://localhost:8080";

/// What `roast login` keeps: `~/.config/roastwarden/roast.json`, mode 600.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// `$ROASTWARDEN_CONFIG`, else `$XDG_CONFIG_HOME/roastwarden/roast.json`, else
/// `~/.config/roastwarden/roast.json`.
pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ROASTWARDEN_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".config")))?;
    Some(base.join("roastwarden").join("roast.json"))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The saved server and token; nothing when there is no file. A file that does not read is
/// said once on stderr and ignored.
pub fn load() -> Saved {
    let Some(path) = config_path() else {
        return Saved::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("roast: {} does not read ({e}); ignoring it", path.display());
            Saved::default()
        }),
        Err(_) => Saved::default(),
    }
}

fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let tmp = path.with_extension("json.new");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(serde_json::to_string_pretty(saved)?.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

fn io_failure(what: &str, path: &Path, e: std::io::Error) -> Failure {
    Failure::new("IO", format!("{what} {}: {e}", path.display()))
}

/// The token, typed (not echoed) on a terminal, else the first line of stdin.
fn read_token() -> Result<String, Failure> {
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    if terminal {
        eprint!("API token (rr_…, from Settings → Tokens): ");
        let _ = std::io::stderr().flush();
        let _ = std::process::Command::new("stty").arg("-echo").status();
    }
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if terminal {
        let _ = std::process::Command::new("stty").arg("echo").status();
        eprintln!();
    }
    read.map_err(|e| Failure::new("IO", format!("reading the token: {e}")))?;
    Ok(line.trim().to_string())
}

/// `roast login [URL]`: check the token against the server, then keep both.
pub async fn login(out: &Out, url: Option<String>, token: Option<String>) -> Result<u8, Failure> {
    let url = url.unwrap_or_else(|| DEFAULT_URL.to_string());
    let url = url.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(Failure::new(
            "BAD_INPUT",
            format!("{url} is not an address: give the server as https://… or http://…"),
        ));
    }
    let token = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) => t.trim().to_string(),
        None => read_token()?,
    };
    if !token.starts_with("rr_") {
        return Err(Failure::new(
            "BAD_INPUT",
            "roast keeps an API token (rr_…, Settings → Tokens), never the owner key: make a token for this machine",
        ));
    }
    let api = Api::new(&url, Some(token.clone()));
    // A read every token may make: a token the server does not know is refused here
    let about = api.get("/api/about").await?;
    let auth = api.get("/api/auth/status").await.unwrap_or_default();
    let path = config_path().ok_or_else(|| {
        Failure::new(
            "IO",
            "no home directory: set ROASTWARDEN_CONFIG to a file path",
        )
    })?;
    save(
        &path,
        &Saved {
            url: Some(url.clone()),
            token: Some(token),
        },
    )
    .map_err(|e| io_failure("cannot write", &path, e))?;
    let answer = json!({
        "url": url,
        "saved": path.display().to_string(),
        "credential": auth["credential"],
        "version": about["version"],
    });
    out.emit(&answer, |a| {
        format!(
            "Signed in to {} (Roastwarden {}) as {}; kept in {}",
            a["url"].as_str().unwrap_or("?"),
            a["version"].as_str().unwrap_or("?"),
            a["credential"].as_str().unwrap_or("a token"),
            a["saved"].as_str().unwrap_or("?"),
        )
    });
    Ok(code::OK)
}

/// `roast logout`: forget the saved server and token.
pub fn logout(out: &Out) -> Result<u8, Failure> {
    let path = config_path().ok_or_else(|| Failure::new("IO", "no home directory"))?;
    let removed = match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(io_failure("cannot remove", &path, e)),
    };
    let answer = json!({"removed": removed, "file": path.display().to_string()});
    out.emit(&answer, |a| {
        if a["removed"].as_bool() == Some(true) {
            format!(
                "Forgot the saved server and token ({})",
                a["file"].as_str().unwrap_or("?")
            )
        } else {
            "Nothing was saved".to_string()
        }
    });
    Ok(code::OK)
}

/// Where each agent reads skills from.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    /// Claude Code: ~/.claude/skills/roastwarden
    ClaudeCode,
    /// Hermes Agent: ~/.hermes/skills/devops/roastwarden ($HERMES_HOME respected)
    Hermes,
    /// Codex and other agents that read the shared Agent Skills folder: ~/.agents/skills/roastwarden
    Agents,
}

pub fn skill_dir(agent: Agent) -> Option<PathBuf> {
    let home = home()?;
    Some(match agent {
        Agent::ClaudeCode => home.join(".claude/skills/roastwarden"),
        Agent::Hermes => std::env::var_os("HERMES_HOME")
            .filter(|h| !h.is_empty())
            .map_or_else(|| home.join(".hermes"), PathBuf::from)
            .join("skills/devops/roastwarden"),
        Agent::Agents => home.join(".agents/skills/roastwarden"),
    })
}

/// The skill with a "This deployment:" paragraph after its Setup block naming the server, when
/// one is known. The paragraph's form is the one the agent host's sync keeps.
pub fn skill_for(url: Option<&str>, saved: bool) -> String {
    let Some(url) = url else {
        return SKILL_MD.to_string();
    };
    let how = if saved {
        "`roast login` saved the server and a token on this machine, so `roast` needs no environment: run it as is."
    } else {
        "Set ROASTWARDEN_TOKEN (or run `roast login` once) before the first command."
    };
    let paragraph = format!("This deployment: the server is {url}. {how}");
    let mut out = String::with_capacity(SKILL_MD.len() + paragraph.len() + 4);
    let mut in_setup = false;
    let mut done = false;
    for line in SKILL_MD.lines() {
        out.push_str(line);
        out.push('\n');
        if done {
            continue;
        }
        if line.starts_with("## Setup") {
            in_setup = true;
        } else if in_setup && line.starts_with("```") && line.trim() == "```" {
            out.push('\n');
            out.push_str(&paragraph);
            out.push('\n');
            done = true;
        }
    }
    out
}

/// `roast skill install`: SKILL.md and label-design.md into the agent's folder (the old
/// SKILL.md kept as SKILL.md.bak).
pub fn install(
    out: &Out,
    agent: Option<Agent>,
    dir: Option<PathBuf>,
    url: Option<&str>,
    saved: bool,
) -> Result<u8, Failure> {
    let dir = match (dir, agent) {
        (Some(d), _) => d,
        (None, Some(a)) => {
            skill_dir(a).ok_or_else(|| Failure::new("IO", "no home directory: give --dir"))?
        }
        (None, None) => {
            return Err(Failure::new(
                "BAD_INPUT",
                "say which agent (claude-code, hermes, agents) or give --dir",
            ))
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| io_failure("cannot create", &dir, e))?;
    let skill = dir.join("SKILL.md");
    if skill.is_file() {
        let _ = std::fs::copy(&skill, dir.join("SKILL.md.bak"));
    }
    std::fs::write(&skill, skill_for(url, saved))
        .map_err(|e| io_failure("cannot write", &skill, e))?;
    let design = dir.join("label-design.md");
    std::fs::write(&design, LABEL_DESIGN_MD).map_err(|e| io_failure("cannot write", &design, e))?;
    let answer = json!({
        "installed": dir.display().to_string(),
        "files": ["SKILL.md", "label-design.md"],
        "server": url,
        "roast": env!("CARGO_PKG_VERSION"),
    });
    out.emit(&answer, |a| {
        format!(
            "The Roastwarden skill ({}) is in {}{}",
            a["roast"].as_str().unwrap_or("?"),
            a["installed"].as_str().unwrap_or("?"),
            if a["server"].is_null() {
                "; no server saved yet: run `roast login <address>`".to_string()
            } else {
                format!(", for {}", a["server"].as_str().unwrap_or("?"))
            }
        )
    });
    Ok(code::OK)
}

/// `roast skill show`: the skill as it is, for an agent with no skills folder (paste it into
/// its instructions, AGENTS.md or a system prompt).
pub fn show(label_design: bool) -> Result<u8, Failure> {
    let text = if label_design {
        LABEL_DESIGN_MD
    } else {
        SKILL_MD
    };
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .map_err(|e| Failure::new("IO", e.to_string()))?;
    Ok(code::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deployment_paragraph_follows_the_setup_block() {
        let s = skill_for(Some("https://roast.example"), true);
        let setup = s.find("## Setup").unwrap();
        let para = s
            .find("This deployment: the server is https://roast.example.")
            .unwrap();
        assert!(para > setup);
        let fence_close = s[setup..]
            .match_indices("\n```\n")
            .next()
            .map(|(i, _)| setup + i);
        assert!(
            fence_close.is_some_and(|f| f < para),
            "after the Setup block's code"
        );
        assert_eq!(s.matches("This deployment:").count(), 1);
        assert_eq!(skill_for(None, false), SKILL_MD);
    }

    #[test]
    fn the_skill_is_the_one_in_the_repository() {
        assert!(SKILL_MD.starts_with("---\nname: roastwarden"));
        assert!(
            SKILL_MD.contains("roast login"),
            "the skill tells an agent about roast login"
        );
    }

    #[test]
    fn a_saved_file_is_for_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("roast-setup-{}", std::process::id()));
        let path = dir.join("roastwarden/roast.json");
        save(
            &path,
            &Saved {
                url: Some("http://x".into()),
                token: Some("rr_x".into()),
            },
        )
        .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let back: Saved = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.token.as_deref(), Some("rr_x"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
