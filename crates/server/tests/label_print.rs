//! Printing labels (docs/labels.md, "Printing"): a fake IPP printer on a local port answers
//! Get-Printer-Attributes and records each Print-Job. The owner prints at once; a token's
//! print is always held for the owner's approval (even a token that does not ask first) and
//! reaches the printer only when approved; a stopped printer gets nothing.

#![allow(
    clippy::unwrap_used,
    reason = "test code: a panic is the failure, and the helpers are not #[test] functions"
)]

use std::sync::{Arc, Mutex};

use axum::{body::Bytes, extract::State, routing::post, Router};
use ipp::attribute::IppAttribute;
use ipp::model::{DelimiterTag, IppVersion, Operation, StatusCode};
use ipp::parser::IppParser;
use ipp::request::IppRequestResponse;
use ipp::value::{IppKeyword, IppName, IppValue};
use roastwarden_server::test_support::Harness;
use serde_json::json;

#[derive(Default)]
struct Printer {
    /// Each job: its attributes (name = value) and the document's size
    jobs: Vec<(Vec<String>, usize)>,
    /// Each job's document, its first 64 bytes
    heads: Vec<Vec<u8>>,
    stopped: bool,
    /// Take jobs, then give them up (as the M252dw did with a media-col without a size)
    abort: bool,
    /// No PDF: PWG Raster at 300 dpi and URF at 600 (an AirPrint-only printer)
    raster_only: bool,
}

type Shared = Arc<Mutex<Printer>>;

fn kw(s: &str) -> IppValue {
    IppValue::Keyword(IppKeyword::new(s).unwrap())
}

fn attr(name: &str, v: IppValue) -> IppAttribute {
    IppAttribute::new(IppName::new(name).unwrap(), v)
}

async fn ipp(State(p): State<Shared>, body: Bytes) -> Vec<u8> {
    answer(&p, &body)
}

/// The printer's answer to one IPP request
fn answer(p: &Shared, body: &[u8]) -> Vec<u8> {
    let req = IppParser::new(std::io::Cursor::new(body.to_vec()))
        .parse()
        .unwrap();
    let op = req.header().operation_or_status;
    let mut res = IppRequestResponse::new_response(
        IppVersion::v1_1(),
        StatusCode::SuccessfulOk,
        req.header().request_id,
    )
    .unwrap();
    let mut printer = p.lock().unwrap();
    match op {
        x if x == Operation::GetPrinterAttributes as i16 => {
            let a = res.attributes_mut();
            let g = DelimiterTag::PrinterAttributes;
            a.add(
                g,
                attr(
                    "printer-make-and-model",
                    IppValue::TextWithoutLanguage("Fake LaserJet".try_into().unwrap()),
                ),
            );
            a.add(
                g,
                attr(
                    "printer-state",
                    IppValue::Enum(if printer.stopped { 5 } else { 3 }),
                ),
            );
            a.add(
                g,
                attr(
                    "printer-state-reasons",
                    kw(if printer.stopped {
                        "door-open-error"
                    } else {
                        "none"
                    }),
                ),
            );
            let mime = |m: &str| IppValue::MimeMediaType(m.try_into().unwrap());
            if printer.raster_only {
                a.add(
                    g,
                    attr(
                        "document-format-supported",
                        IppValue::Array(vec![mime("image/pwg-raster"), mime("image/urf")]),
                    ),
                );
                a.add(
                    g,
                    attr(
                        "pwg-raster-document-resolution-supported",
                        IppValue::Resolution {
                            cross_feed: 300,
                            feed: 300,
                            units: 3,
                        },
                    ),
                );
                a.add(
                    g,
                    attr(
                        "pwg-raster-document-type-supported",
                        IppValue::Array(vec![kw("sgray_8"), kw("srgb_8")]),
                    ),
                );
                a.add(
                    g,
                    attr(
                        "urf-supported",
                        IppValue::Array(vec![kw("V1.4"), kw("W8"), kw("SRGB24"), kw("RS600")]),
                    ),
                );
            } else {
                a.add(
                    g,
                    attr(
                        "document-format-supported",
                        IppValue::Array(vec![mime("application/pdf"), mime("image/urf")]),
                    ),
                );
            }
            a.add(
                g,
                attr(
                    "media-type-supported",
                    IppValue::Array(vec![kw("stationery"), kw("labels")]),
                ),
            );
            a.add(
                g,
                attr(
                    "print-scaling-supported",
                    IppValue::Array(vec![kw("auto"), kw("none")]),
                ),
            );
            // Letter paper in tray 1
            let mut size = std::collections::BTreeMap::new();
            size.insert(
                IppName::new("x-dimension").unwrap(),
                IppValue::Integer(21590),
            );
            size.insert(
                IppName::new("y-dimension").unwrap(),
                IppValue::Integer(27940),
            );
            let mut col = std::collections::BTreeMap::new();
            col.insert(
                IppName::new("media-size").unwrap(),
                IppValue::Collection(size),
            );
            col.insert(IppName::new("media-source").unwrap(), kw("tray-1"));
            col.insert(IppName::new("media-type").unwrap(), kw("stationery"));
            a.add(g, attr("media-col-ready", IppValue::Collection(col)));
        }
        x if x == Operation::GetJobAttributes as i16 => {
            let a = res.attributes_mut();
            let j = DelimiterTag::JobAttributes;
            let (state, reason, sheets) = if printer.abort {
                (8, "aborted-by-system", 0)
            } else {
                (9, "none", 1)
            };
            a.add(j, attr("job-id", IppValue::Integer(42)));
            a.add(j, attr("job-state", IppValue::Enum(state)));
            a.add(j, attr("job-state-reasons", kw(reason)));
            a.add(
                j,
                attr("job-media-sheets-completed", IppValue::Integer(sheets)),
            );
        }
        x if x == Operation::PrintJob as i16 => {
            let attrs: Vec<String> = req
                .attributes()
                .groups()
                .iter()
                .flat_map(|g| g.attributes().iter())
                .map(|a| format!("{}={}", a.name(), a.value()))
                .collect();
            let mut doc = Vec::new();
            std::io::Read::read_to_end(&mut req.into_payload(), &mut doc).unwrap();
            printer.heads.push(doc.iter().take(64).copied().collect());
            printer.jobs.push((attrs, doc.len()));
            let a = res.attributes_mut();
            a.add(
                DelimiterTag::JobAttributes,
                attr("job-id", IppValue::Integer(42)),
            );
            a.add(
                DelimiterTag::JobAttributes,
                // Accepted, not printed yet: Roastwarden asks after it
                attr("job-state", IppValue::Enum(3)),
            );
        }
        _ => {}
    }
    res.to_bytes().to_vec()
}

/// The fake printer on a free local port
async fn start_printer() -> (Shared, u16) {
    start_printer_with(Printer::default()).await
}

async fn start_printer_with(p: Printer) -> (Shared, u16) {
    let printer: Shared = Arc::new(Mutex::new(p));
    let app = Router::new()
        .route("/ipp/print", post(ipp))
        .with_state(printer.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (printer, port)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn labels_print_at_actual_size_and_a_tokens_print_waits_for_the_owner() {
    let (printer, port) = start_printer().await;

    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "print me", "device_id": "virtual_roaster" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();

    // No printer yet
    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/labels/print"),
            Some(json!({})),
        )
        .await;
    assert_eq!(r.status.as_u16(), 409, "{:?}", r.json());
    let r = h
        .owner(
            "PUT",
            "/api/settings/label_printer_url",
            Some(json!({ "value": format!("ipp://127.0.0.1:{port}/ipp/print") })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let r = h.owner("GET", "/api/labels/printer", None).await.json();
    assert_eq!(r["printer"]["make_and_model"], "Fake LaserJet", "{r}");
    assert_eq!(r["printer"]["labels_media"], true);

    // The owner prints at once: PDF, actual size, label stock
    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/labels/print"),
            Some(json!({ "count": 2, "start": 4, "design": "cherry" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["job_id"], 42);
    assert_eq!(
        r.json()["job_state"],
        "completed",
        "followed until printed: {:?}",
        r.json()
    );
    assert_eq!(r.json()["sheets"], 1);
    assert_eq!(r.json()["tray"], "tray-1", "the tray holding Letter paper");
    {
        let p = printer.lock().unwrap();
        assert_eq!(p.jobs.len(), 1);
        let attrs = p.jobs[0].0.join(" ");
        assert!(attrs.contains("document-format=application/pdf"), "{attrs}");
        assert!(attrs.contains("print-scaling=none"), "{attrs}");
        assert!(attrs.contains("labels"), "media-type labels: {attrs}");
        // The size and the tray too: without a size the M252dw aborted every job
        assert!(
            attrs.contains("21590") && attrs.contains("27940"),
            "media-size: {attrs}"
        );
        assert!(attrs.contains("tray-1"), "media-source: {attrs}");
        assert!(p.jobs[0].1 > 1000, "a PDF was sent");
    }

    // A token that does not ask first still waits for the owner to print
    let t = h
        .owner("POST", "/api/tokens", Some(json!({ "name": "printer agent", "preset": "inventory", "require_approval": false })))
        .await
        .json()["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let r = roastwarden_server::test_support::call(
        &h.app,
        "POST",
        &format!("/api/sessions/{id}/labels/print"),
        Some(&t),
        Some(json!({ "count": 1, "start": 2 })),
    )
    .await;
    assert_eq!(r.status.as_u16(), 202, "{:?}", r.json());
    let approval = r.json()["approval"].clone();
    let summary = approval["summary"].as_str().unwrap();
    assert!(
        summary.contains("5163") && summary.contains("label 2"),
        "{summary}"
    );
    assert_eq!(
        printer.lock().unwrap().jobs.len(),
        1,
        "nothing printed before the owner says yes"
    );
    let aid = approval["id"].as_str().unwrap();
    // The same print again is the same request; another print of the roast is its own (agents
    // once got the first print's request back for a different one)
    let print = |body: serde_json::Value| {
        let (app, uri, t) = (
            h.app.clone(),
            format!("/api/sessions/{id}/labels/print"),
            t.clone(),
        );
        async move {
            roastwarden_server::test_support::call(&app, "POST", &uri, Some(&t), Some(body)).await
        }
    };
    let r = print(json!({ "count": 1, "start": 2 })).await;
    assert_eq!(r.json()["approval"]["id"], aid);
    let r = print(json!({ "count": 2, "start": 5 })).await;
    assert_eq!(r.status.as_u16(), 202, "{:?}", r.json());
    let other = r.json()["approval"].clone();
    assert_ne!(other["id"], aid, "a request of its own");
    let summary = other["summary"].as_str().unwrap();
    assert!(
        summary.contains("2 labels") && summary.contains("label 5"),
        "{summary}"
    );
    let r = h
        .owner("POST", &format!("/api/approvals/{aid}/approve"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(
        printer.lock().unwrap().jobs.len(),
        2,
        "printed once approved"
    );

    // A job the printer takes and then gives up is a failure, not "sent"
    printer.lock().unwrap().abort = true;
    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/labels/print"),
            Some(json!({})),
        )
        .await;
    assert_eq!(r.status.as_u16(), 502, "{:?}", r.json());
    assert!(
        r.json().to_string().contains("aborted-by-system"),
        "{:?}",
        r.json()
    );
    printer.lock().unwrap().abort = false;

    // A stopped printer gets nothing and says why
    printer.lock().unwrap().stopped = true;
    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{id}/labels/print"),
            Some(json!({})),
        )
        .await;
    assert_eq!(r.status.as_u16(), 409, "{:?}", r.json());
    assert!(r.json().to_string().contains("door-open-error"));
    assert_eq!(printer.lock().unwrap().jobs.len(), 3);
}

/// Print runs (docs/labels.md, "Print runs"): a gift run of three coffees on one sheet, each
/// in its own design; printed, it moves the sheet's next free label on; a token's print of a
/// run waits for the owner with what to load; a reprint of chosen labels lays them out as
/// they were; a PDF printed by hand is marked so; a roast's own print is a run too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gift_run_prints_several_coffees_on_one_sheet_and_reprints_as_it_was() {
    let (printer, port) = start_printer().await;
    let h = Harness::new().await;
    let mut ids = vec![];
    for name in ["Guji", "Palmichal", "Huila"] {
        let s = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({ "name": name, "device_id": "virtual_roaster" })),
            )
            .await
            .json();
        ids.push(s["id"].as_str().unwrap().to_string());
    }
    let r = h
        .owner(
            "PUT",
            "/api/settings/label_printer_url",
            Some(json!({ "value": format!("ipp://127.0.0.1:{port}/ipp/print") })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());

    let ask = json!({
        "sheet": "5163",
        "items": [
            { "roast": ids[0], "count": 2, "design": "cherry" },
            { "roast": ids[1], "design": "gift", "message": "for Sam" },
            { "roast": ids[2], "count": 3 },
        ],
        "note": "gift baskets",
    });
    let r = h
        .owner("POST", "/api/label-runs/preview", Some(ask.clone()))
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.content_type.as_deref(), Some("image/svg+xml"));

    let r = h.owner("POST", "/api/label-runs", Some(ask)).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    let run = r.json();
    let first = run["id"].as_str().unwrap().to_string();
    assert_eq!(run["labels"], 6);
    assert_eq!(run["start"], 1, "a new sheet starts at label 1");
    assert_eq!(run["state"], "made");
    assert_eq!(
        run["summary"],
        "Guji (#1) ×2, Palmichal (#2), Huila (#3) ×3"
    );
    assert_eq!(run["items"][0]["design"], "cherry");
    assert_eq!(run["items"][1]["data"]["message"], "for Sam");
    assert_eq!(run["items"][2]["design"], "plain");
    let r = h
        .owner("GET", &format!("/api/label-runs/{first}/pdf"), None)
        .await;
    assert_eq!(r.content_type.as_deref(), Some("application/pdf"));

    // The owner prints it: the job completes and the sheet's next free label is 7
    let r = h
        .owner("POST", &format!("/api/label-runs/{first}/print"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["job_state"], "completed");
    assert_eq!(r.json()["run"], first);
    let run = h
        .owner("GET", &format!("/api/label-runs/{first}"), None)
        .await
        .json();
    assert_eq!(run["state"], "completed", "{run}");
    assert_eq!(run["job_id"], 42);
    assert_ne!(
        run["states"].as_array().unwrap().as_slice(),
        &[] as &[serde_json::Value]
    );
    let next = |sheets: serde_json::Value| {
        sheets
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sheet"] == "5163")
            .unwrap()["next_label"]
            .clone()
    };
    assert_eq!(
        next(h.owner("GET", "/api/label-sheets", None).await.json()),
        7
    );

    // The next run starts there by default; a token's print of it waits for the owner
    let r = h
        .owner(
            "POST",
            "/api/label-runs",
            Some(json!({ "items": [{ "roast": ids[0] }] })),
        )
        .await;
    let second = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(r.json()["start"], 7);
    let t = h.token("inventory").await;
    let r = roastwarden_server::test_support::call(
        &h.app,
        "POST",
        &format!("/api/label-runs/{second}/print"),
        Some(&t),
        None,
    )
    .await;
    assert_eq!(r.status.as_u16(), 202, "{:?}", r.json());
    let approval = r.json()["approval"].clone();
    let summary = approval["summary"].as_str().unwrap();
    assert!(
        summary.contains("5163")
            && summary.contains("label 7")
            && summary.contains("labels 1–6 used"),
        "{summary}"
    );
    assert_eq!(
        printer.lock().unwrap().jobs.len(),
        1,
        "nothing before the owner says yes"
    );
    let aid = approval["id"].as_str().unwrap();
    let r = h
        .owner("POST", &format!("/api/approvals/{aid}/approve"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(printer.lock().unwrap().jobs.len(), 2);
    assert_eq!(
        next(h.owner("GET", "/api/label-sheets", None).await.json()),
        8
    );

    // Labels 3 and 4 of the first run again (Palmichal's and the first Huila), as they were
    let r = h
        .owner(
            "POST",
            &format!("/api/label-runs/{first}/reprint"),
            Some(json!({ "labels": [3, 4] })),
        )
        .await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    let again = r.json();
    let reprint = again["id"].as_str().unwrap().to_string();
    assert_eq!(again["reprint_of"], first);
    assert_eq!(again["start"], 8);
    assert_eq!(again["labels"], 2);
    assert_eq!(again["summary"], "Palmichal (#2), Huila (#3)");
    assert_eq!(again["items"][0]["design"], "gift");
    assert_eq!(again["items"][0]["data"]["message"], "for Sam");
    let bad = h
        .owner(
            "POST",
            &format!("/api/label-runs/{first}/reprint"),
            Some(json!({ "labels": [9] })),
        )
        .await;
    assert_eq!(bad.status.as_u16(), 400, "{:?}", bad.json());

    // Printed by hand from its PDF: marked so, the sheet moves on
    let r = h
        .owner("POST", &format!("/api/label-runs/{reprint}/done"), None)
        .await;
    assert_eq!(r.json()["state"], "printed", "{:?}", r.json());
    assert_eq!(
        next(h.owner("GET", "/api/label-sheets", None).await.json()),
        10
    );
    let r = h
        .owner("POST", &format!("/api/label-runs/{reprint}/done"), None)
        .await;
    assert_eq!(r.status.as_u16(), 409);

    // A roast's own print is a run in the history too
    let r = h
        .owner(
            "POST",
            &format!("/api/sessions/{}/labels/print", ids[2]),
            Some(json!({ "count": 1, "start": 10 })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let list = h.owner("GET", "/api/label-runs", None).await.json();
    assert_eq!(list.as_array().unwrap().len(), 4, "{list}");
    assert_eq!(list[0]["summary"], "Huila (#3)");
    assert_eq!(
        next(h.owner("GET", "/api/label-sheets", None).await.json()),
        1,
        "a full sheet: a new one next"
    );

    // Set by hand
    let r = h
        .owner("PUT", "/api/label-sheets/5163", Some(json!({ "next": 5 })))
        .await;
    assert_eq!(r.json()["next_label"], 5);
    let opts = h.owner("GET", "/api/labels", None).await.json();
    let s5163 = opts["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "5163")
        .unwrap()
        .clone();
    assert_eq!(s5163["next_label"], 5);
}

/// The fake printer over TLS with the test certificate (`testdata/tls`): IPP over HTTP/1.1,
/// one request a connection
async fn start_tls_printer() -> (Shared, u16) {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let cert = CertificateDer::from(include_bytes!("../testdata/tls/printer-cert.der").to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        include_bytes!("../testdata/tls/printer-key.der").to_vec(),
    ));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let printer: Shared = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let p = printer.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let p = p.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = tls.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = tls.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let out = answer(&p, &buf[head_end..head_end + len]);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
                    out.len()
                );
                let _ = tls.write_all(reply.as_bytes()).await;
                let _ = tls.write_all(&out).await;
                let _ = tls.flush().await;
            });
        }
    });
    (printer, port)
}

/// Printers (docs/labels.md, "Printers"): more than one, each asked what it takes; a run is
/// laid out for the default printer's calibration and sent as PDF to a printer that takes it,
/// as PWG Raster or URF (600 dpi from 300) to one that does not; two corners that disagree
/// mean the page scales; an `ipps` printer is reached only with its certificate pinned.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn printers_take_pdf_or_raster_and_ipps_needs_the_pinned_certificate() {
    let (laser, a) = start_printer().await;
    let (raster, b) = start_printer_with(Printer {
        raster_only: true,
        ..Printer::default()
    })
    .await;
    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Guji", "device_id": "virtual_roaster" })),
        )
        .await
        .json();
    let roast = s["id"].as_str().unwrap().to_string();

    let r = h
        .owner(
            "POST",
            "/api/label-printers",
            Some(
                json!({ "name": "Office laser", "url": format!("ipp://127.0.0.1:{a}/ipp/print") }),
            ),
        )
        .await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    assert_eq!(r.json()["id"], "office-laser", "the id from the name");
    assert_eq!(
        r.json()["is_default"],
        true,
        "the first printer is the default"
    );
    assert_eq!(r.json()["sends"], "pdf");
    let r = h
        .owner(
            "POST",
            "/api/label-printers",
            Some(json!({ "id": "airprint", "name": "No PDF", "url": format!("ipp://127.0.0.1:{b}/ipp/print") })),
        )
        .await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    assert_eq!(r.json()["is_default"], false);
    assert_eq!(r.json()["sends"], "pwg at 300 dpi");

    // Calibration per printer and sheet, from two corners; corners far apart are scaling
    let r = h
        .owner(
            "PUT",
            "/api/label-printers/office-laser/calibration/5163",
            Some(json!({ "first": [1.0, 0.4], "last": [1.2, 0.6], "tray": "tray-1" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["calibrations"][0]["x_mm"], 1.1);
    assert_eq!(r.json()["calibrations"][0]["y_mm"], 0.5);
    let r = h
        .owner(
            "PUT",
            "/api/label-printers/office-laser/calibration/5163",
            Some(json!({ "first": [0.5, 0.0], "last": [1.6, 0.1] })),
        )
        .await;
    assert_eq!(r.status.as_u16(), 400);
    assert!(r.json().to_string().contains("scaling"), "{:?}", r.json());

    // A run is laid out for the default printer; printed there as PDF
    let run = h
        .owner(
            "POST",
            "/api/label-runs",
            Some(json!({ "items": [{ "roast": roast, "count": 2 }] })),
        )
        .await
        .json();
    assert_eq!(run["printer_id"], "office-laser");
    let id = run["id"].as_str().unwrap().to_string();
    let r = h
        .owner("POST", &format!("/api/label-runs/{id}/print"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["sent_as"], "application/pdf");
    assert!(laser.lock().unwrap().heads[0].starts_with(b"%PDF"));

    // The printer without PDF gets PWG Raster, then URF at 600 dpi when asked for it
    let r = h
        .owner(
            "POST",
            &format!("/api/label-runs/{id}/print"),
            Some(json!({ "printer": "airprint", "color": "monochrome" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(r.json()["sent_as"], "image/pwg-raster");
    {
        let p = raster.lock().unwrap();
        assert!(p.heads[0].starts_with(b"RaS2"), "{:?}", &p.heads[0][..8]);
        assert!(p.jobs[0]
            .0
            .join(" ")
            .contains("document-format=image/pwg-raster"));
    }
    let r = h
        .owner(
            "PUT",
            "/api/label-printers/airprint",
            Some(json!({ "name": "No PDF", "url": format!("ipp://127.0.0.1:{b}/ipp/print"), "send": "urf" })),
        )
        .await;
    assert_eq!(r.json()["sends"], "urf at 600 dpi", "{:?}", r.json());
    let r = h
        .owner(
            "POST",
            &format!("/api/label-runs/{id}/print"),
            Some(json!({ "printer": "airprint" })),
        )
        .await;
    assert_eq!(r.json()["sent_as"], "image/urf", "{:?}", r.json());
    {
        let p = raster.lock().unwrap();
        let head = &p.heads[1];
        assert!(head.starts_with(b"UNIRAST\0"));
        // One page, 600 dpi: the page header after the 12-byte file header
        assert_eq!(u32::from_be_bytes(head[8..12].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(head[32..36].try_into().unwrap()), 600);
    }

    let r = h
        .owner("POST", "/api/label-printers/airprint/default", None)
        .await;
    assert_eq!(r.json()["is_default"], true);
    let list = h.owner("GET", "/api/label-printers", None).await.json();
    assert_eq!(list[0]["id"], "airprint", "the default first: {list}");

    // ipps: the certificate is read for the owner to trust; only that one is accepted
    let (tls, c) = start_tls_printer().await;
    let url = format!("ipps://127.0.0.1:{c}/ipp/print");
    let r = h
        .owner(
            "POST",
            "/api/label-printers/certificate",
            Some(json!({ "url": url })),
        )
        .await;
    let fingerprint = "f5d22034e288e86a2de0f79e3863517aeffb56e8832ffe43bd068c866147a35b";
    assert_eq!(r.json()["fingerprint"], fingerprint, "{:?}", r.json());
    let wrong = "0".repeat(64);
    let r = h
        .owner(
            "POST",
            "/api/label-printers",
            Some(json!({ "id": "secure", "name": "Secure", "url": url, "fingerprint": wrong })),
        )
        .await;
    assert_eq!(r.status.as_u16(), 201, "saved, though it does not answer");
    assert!(
        r.json()["problem"]
            .as_str()
            .unwrap_or("")
            .contains("another certificate"),
        "{:?}",
        r.json()
    );
    let r = h
        .owner(
            "PUT",
            "/api/label-printers/secure",
            Some(json!({ "name": "Secure", "url": url, "fingerprint": fingerprint })),
        )
        .await;
    assert!(r.json()["problem"].is_null(), "{:?}", r.json());
    assert_eq!(r.json()["sends"], "pdf");
    let r = h
        .owner(
            "POST",
            &format!("/api/label-runs/{id}/print"),
            Some(json!({ "printer": "secure" })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    assert_eq!(tls.lock().unwrap().jobs.len(), 1, "printed over TLS");

    let r = h.owner("DELETE", "/api/label-printers/secure", None).await;
    assert_eq!(r.status.as_u16(), 204);
}

/// A repeat Download (docs/labels.md, "Print runs"): the same labels as a run made and not
/// printed answer that run, so the history keeps one copy and "Printed it" moves the sheet on
/// once; other labels, `new`, or a run already printed make a new one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_labels_again_answer_the_run_not_yet_printed() {
    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Guji", "device_id": "virtual_roaster" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    let ask = json!({ "sheet": "5163", "items": [
        { "roast": id, "count": 2, "design": "gift", "message": "for Sam" } ] });

    let r = h.owner("POST", "/api/label-runs", Some(ask.clone())).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    let first = r.json()["id"].as_str().unwrap().to_string();
    assert!(r.json().get("reused").is_none(), "{:?}", r.json());

    // The same again: that run, not a second one
    let r = h.owner("POST", "/api/label-runs", Some(ask.clone())).await;
    assert_eq!(r.status.as_u16(), 200, "{:?}", r.json());
    assert_eq!(r.json()["id"], first);
    assert_eq!(r.json()["reused"], true);
    let list = h.owner("GET", "/api/label-runs", None).await.json();
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");

    // Other labels (one more copy), or `new`: a new run
    let mut more = ask.clone();
    more["items"][0]["count"] = json!(3);
    let r = h.owner("POST", "/api/label-runs", Some(more)).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    assert_ne!(r.json()["id"], first);
    let mut fresh = ask.clone();
    fresh["new"] = json!(true);
    let r = h.owner("POST", "/api/label-runs", Some(fresh)).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    assert_ne!(r.json()["id"], first);

    // Printed it: the sheet moves on to label 3, and the same labels again are a new run
    // there
    let r = h
        .owner("POST", &format!("/api/label-runs/{first}/done"), None)
        .await;
    assert_eq!(r.json()["state"], "printed", "{:?}", r.json());
    let r = h.owner("POST", "/api/label-runs", Some(ask)).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    assert_ne!(r.json()["id"], first);
    assert_eq!(r.json()["start"], 3);
}

/// Packed bags from a print run (docs/roasted.md, "Packed bags"): `pack` takes
/// copies × the label's weight off the roast, ground with its grind; the same labels again
/// (the run reused) take nothing more; more than is left is refused before a run is made; a
/// token that may print but not record moves cannot pack.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_takes_its_bags_off_the_roast_once() {
    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "Guji", "device_id": "bench_roaster",
                "roast_purpose": "espresso", "target_roast_level": "light" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    h.owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    h.owner("POST", &format!("/api/sessions/{id}/complete"), None)
        .await;
    let r = h
        .owner(
            "PUT",
            &format!("/api/sessions/{id}"),
            Some(json!({ "roasted_weight": 450.0 })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let ask = json!({ "sheet": "5163", "items": [
        { "roast": id, "count": 2, "weight_g": 100, "grind": "espresso", "pack": true } ] });

    let r = h.owner("POST", "/api/label-runs", Some(ask.clone())).await;
    assert_eq!(r.status.as_u16(), 201, "{:?}", r.json());
    let run = r.json();
    let packed = &run["packed"][0];
    assert_eq!(
        (
            &packed["bags"],
            &packed["bag_g"],
            &packed["form"],
            &packed["grind"]
        ),
        (
            &json!(2),
            &json!(100.0),
            &json!("ground"),
            &json!("espresso")
        ),
        "{run}"
    );
    assert_eq!(packed["label_run"], run["id"]);
    // The label says so
    assert_eq!(
        run["items"][0]["data"]["form"], "Ground · espresso",
        "{run}"
    );
    let lot = h
        .owner("GET", &format!("/api/roasted/{id}"), None)
        .await
        .json();
    assert_eq!(lot["left_g"], 250.0, "{lot}");
    assert_eq!(lot["packed_g"], 200.0, "{lot}");

    // The same labels again: the same run, nothing more taken
    let r = h.owner("POST", "/api/label-runs", Some(ask.clone())).await;
    assert_eq!(r.status.as_u16(), 200, "{:?}", r.json());
    assert_eq!(r.json()["reused"], true);
    let lot = h
        .owner("GET", &format!("/api/roasted/{id}"), None)
        .await
        .json();
    assert_eq!(lot["left_g"], 250.0, "{lot}");

    // More than is left: refused, no run made
    let runs = h.owner("GET", "/api/label-runs", None).await.json();
    let mut big = ask.clone();
    big["items"][0]["count"] = json!(3);
    big["new"] = json!(true);
    let r = h.owner("POST", "/api/label-runs", Some(big)).await;
    assert_eq!(r.status.as_u16(), 400, "{:?}", r.json());
    let after = h.owner("GET", "/api/label-runs", None).await.json();
    assert_eq!(
        runs.as_array().unwrap().len(),
        after.as_array().unwrap().len()
    );

    // A roast token may print but not take coffee off: refused
    let token = h.token("roast").await;
    let mut one = ask.clone();
    one["items"][0]["count"] = json!(1);
    one["new"] = json!(true);
    let r = roastwarden_server::test_support::call(
        &h.app,
        "POST",
        "/api/label-runs",
        Some(&token),
        Some(one),
    )
    .await;
    assert_eq!(r.status.as_u16(), 403, "{:?}", r.json());
}
