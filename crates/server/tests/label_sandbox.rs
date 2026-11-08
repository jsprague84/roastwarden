//! Label designs render in a worker process (labels/sandbox.rs): the real server binary as
//! `roastwarden-server label-render`, with a wall clock, a CPU limit and an address-space limit.
//! A design that loops for ever or grows without end is stopped and answered as an error;
//! the server goes on. The whole server (`test_support::Harness`).

use std::time::{Duration, Instant};

use roastwarden_server::test_support::Harness;
use serde_json::json;

fn worker() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_roastwarden-server").into()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_design_renders_in_the_worker_and_a_runaway_one_is_stopped() {
    let h = Harness::with_label_worker(worker(), Duration::from_secs(3)).await;

    // A design that works: the same answer as in the server
    let r = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({ "design": "gift", "sheet": "6874", "png": 72 })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let c = r.json();
    assert_eq!(c["ok"], true, "{c}");
    assert!(c["png"].as_str().is_some_and(|p| p.len() > 100));
    let options = h.owner("GET", "/api/labels", None).await.json();
    assert_eq!(options["sandboxed"], true);

    // Typst caps `while` loops, not `for` loops: this one would run for hours
    let started = Instant::now();
    let r = h
        .owner(
            "POST",
            "/api/labels/check",
            Some(json!({ "source": "#let label(l, w, h, round) = { for i in range(100000) { for j in range(100000) { } } }" })),
        )
        .await;
    let c = r.json();
    assert_eq!(c["ok"], false);
    let message = c["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(message.contains("longer than 3 s"), "{c}");
    assert!(started.elapsed() < Duration::from_secs(10));

    // A huge array: the worker runs out of its address space, not the server's memory
    let r = h
        .owner(
            "POST",
            "/api/labels/render",
            Some(json!({ "source": "#let big = (0,) * 100000000\n#let label(l, w, h, round) = [#big.len()]" })),
        )
        .await;
    assert_eq!(r.status.as_u16(), 422, "{:?}", r.json());
    let text = r.json().to_string();
    assert!(text.contains("more than 1024 MB"), "{text}");

    // And the server still renders
    let r = h
        .owner(
            "POST",
            "/api/labels/render",
            Some(json!({ "design": "plain", "format": "svg" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
}
