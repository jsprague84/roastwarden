//! `roast login`, the skill and the environment variables from before the rename.

use std::process::Command;

use roastwarden_server::test_support::{Harness, OWNER_KEY};
use serde_json::Value;

/// An install from before the rename: `RUSTROAST_URL` and `RUSTROAST_TOKEN` still reach the
/// server, with a note naming them on stderr (legacy.rs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_old_variable_names_still_work() {
    let h = Harness::new().await;
    let url = h.serve().await;
    let ran = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_roast"));
        for var in [
            "ROASTWARDEN_URL",
            "ROASTWARDEN_TOKEN",
            "ROASTWARDEN_API_KEY",
        ] {
            cmd.env_remove(var);
        }
        cmd.env(
            "ROASTWARDEN_CONFIG",
            "/nonexistent/roastwarden-test/roast.json",
        );
        cmd.env("RUSTROAST_URL", &url)
            .env("RUSTROAST_TOKEN", OWNER_KEY)
            .args(["--json", "status"])
            .output()
            .expect("the roast binary")
    })
    .await
    .unwrap();
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(0), "{stderr}");
    let json: Value = serde_json::from_slice(&ran.stdout).unwrap();
    assert_eq!(json["credential"], "owner key", "{json}");
    assert!(
        stderr.contains("RUSTROAST_TOKEN, RUSTROAST_URL read under their new names"),
        "{stderr}"
    );
}

/// `roast login` keeps the server and a checked token in a file only its owner reads; after it
/// `roast` needs no environment, `skill install` names the server in the skill, and `logout`
/// forgets both. The owner key and a token the server does not know are never saved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_keeps_the_server_and_token_and_the_skill_names_it() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::new().await;
    let url = h.serve().await;
    let token = h.token("read_only").await;
    let dir = std::env::temp_dir().join(format!("roast-login-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = dir.join("cfg/roast.json");
    let skills = dir.join("skills");
    let run = |env: Vec<(&'static str, String)>, args: Vec<String>, stdin: Option<String>| {
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_roast"));
            for var in [
                "ROASTWARDEN_URL",
                "ROASTWARDEN_TOKEN",
                "ROASTWARDEN_API_KEY",
                "RUSTROAST_URL",
                "RUSTROAST_TOKEN",
            ] {
                cmd.env_remove(var);
            }
            cmd.env("ROASTWARDEN_CONFIG", &config);
            for (k, v) in env {
                cmd.env(k, v);
            }
            cmd.arg("--json").args(&args);
            cmd.stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = cmd.spawn().expect("the roast binary");
            if let Some(s) = stdin {
                child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
            } else {
                drop(child.stdin.take());
            }
            let out = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let json = serde_json::from_str(stdout.trim()).unwrap_or(Value::String(stdout));
            (
                out.status.code().unwrap_or(-1),
                json,
                String::from_utf8_lossy(&out.stderr).to_string(),
            )
        })
    };

    // The owner key and an unknown token are refused, and nothing is written
    let (code, _, _) = run(
        vec![],
        vec!["login".into(), url.clone()],
        Some(format!("{OWNER_KEY}\n")),
    )
    .await
    .unwrap();
    assert_ne!(code, 0);
    let (code, _, _) = run(
        vec![],
        vec!["login".into(), url.clone()],
        Some("rr_not-a-token\n".into()),
    )
    .await
    .unwrap();
    assert_ne!(code, 0);
    assert!(!config.exists(), "nothing saved on a failed login");

    // The token on stdin, checked, then kept with mode 600
    let (code, json, err) = run(
        vec![],
        vec!["login".into(), url.clone()],
        Some(format!("{token}\n")),
    )
    .await
    .unwrap();
    assert_eq!(code, 0, "{err} {json}");
    assert_eq!(
        std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // No environment now: status answers with the saved token
    let (code, json, err) = run(vec![], vec!["status".into()], None).await.unwrap();
    assert_eq!(code, 0, "{err}");
    assert!(json["credential"].is_string(), "{json}");

    // The skill names the saved server after its Setup block
    let (code, json, err) = run(
        vec![],
        vec![
            "skill".into(),
            "install".into(),
            "--dir".into(),
            skills.display().to_string(),
        ],
        None,
    )
    .await
    .unwrap();
    assert_eq!(code, 0, "{err}");
    assert_eq!(json["server"], Value::String(url.clone()));
    let skill = std::fs::read_to_string(skills.join("SKILL.md")).unwrap();
    assert!(skill.contains(&format!("This deployment: the server is {url}.")));
    assert!(skills.join("label-design.md").is_file());

    // A flag still wins over the saved server
    let (code, _, _) = run(
        vec![("ROASTWARDEN_URL", "http://127.0.0.1:9".into())],
        vec!["status".into()],
        None,
    )
    .await
    .unwrap();
    assert_ne!(code, 0, "the environment's server, not the saved one");

    // logout forgets them
    let (code, json, _) = run(vec![], vec!["logout".into()], None).await.unwrap();
    assert_eq!(code, 0);
    assert_eq!(json["removed"], Value::Bool(true));
    assert!(!config.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
