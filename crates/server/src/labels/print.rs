//! Printing labels over IPP (docs/labels.md, "Printing"): the owner's printer
//! (`label_printer_url`, e.g. `ipp://192.168.1.143:631/ipp/print`), asked what it can do
//! (Get-Printer-Attributes), then sent the PDF (Print-Job) at actual size on label stock.
//! Roastwarden holds a token's print for the owner's approval before it gets here
//! (`approvals.rs`); the printer itself is never asked to hold a job (`job-hold-until` is
//! optional in IPP, and the owner's LaserJet does not offer it).
//!
//! The `ipp` crate builds and reads the messages; Roastwarden writes them over HTTP/1.1 itself
//! (the whole request before reading the answer, see `post_whole`), over TLS for `ipps` with
//! the one certificate the owner trusted (its SHA-256 pinned; printers' certificates are
//! self-signed). A printer without PDF gets PWG Raster or Apple URF (`raster.rs`).

use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use ipp::attribute::IppAttribute;
use ipp::model::DelimiterTag;
use ipp::operation::IppOperation;
use ipp::operation::builder::IppOperationBuilder;
use ipp::parser::IppParser;
use ipp::payload::IppPayload;
use ipp::request::IppRequestResponse;
use ipp::value::{IppKeyword, IppName, IppValue};
use serde::Serialize;
use utoipa::ToSchema;

/// What the printer says about itself
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, ToSchema, PartialEq)]
pub struct PrinterInfo {
    /// The printer's address as set (`label_printer_url`)
    pub url: String,
    pub make_and_model: Option<String>,
    /// idle, processing or stopped
    pub state: Option<String>,
    /// e.g. `door-open-error`, `media-empty-error`; `none` when all is well
    pub state_reasons: Vec<String>,
    /// Takes a PDF as it is
    pub pdf: bool,
    /// Offers the "labels" paper type (a hotter fuser)
    pub labels_media: bool,
    /// Can print at actual size (`print-scaling` `none`)
    pub actual_size: bool,
    pub color: bool,
    pub media_sources: Vec<String>,
    /// What is loaded in each tray, as the printer reports it
    pub media_ready: Vec<String>,
    /// Each tray with paper in it: its name and the paper's size and type (`media-col-ready`)
    pub trays: Vec<Tray>,
    /// Every document format it takes (`document-format-supported`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<String>,
    /// PWG Raster resolutions, dots per inch
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pwg_resolutions: Vec<u32>,
    /// PWG Raster colour spaces (`sgray_8`, `srgb_8`, …)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pwg_types: Vec<String>,
    /// What it says of URF (`urf-supported`: `SRGB24`, `W8`, `RS600`, …)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urf: Vec<String>,
    /// `print-color-mode-supported`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

/// A tray with paper in it.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, ToSchema, PartialEq)]
pub struct Tray {
    /// e.g. `tray-1`, `manual`
    pub source: Option<String>,
    /// The paper's size, hundredths of a mm (Letter is 21590 × 27940)
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// e.g. `stationery`, `labels`
    pub media_type: Option<String>,
}

impl PrinterInfo {
    /// Why it cannot print now, if it cannot: a stopped printer, or nothing it can be sent
    pub fn problem(&self) -> Option<String> {
        if let Err(e) = choose(self, "auto") {
            return Some(e);
        }
        let errors: Vec<&str> = self
            .state_reasons
            .iter()
            .map(String::as_str)
            .filter(|r| r.ends_with("-error"))
            .collect();
        if self.state.as_deref() == Some("stopped") || !errors.is_empty() {
            return Some(format!(
                "the printer is {}: {}",
                self.state.as_deref().unwrap_or("not ready"),
                if errors.is_empty() {
                    self.state_reasons.join(", ")
                } else {
                    errors.join(", ")
                }
            ));
        }
        None
    }
}

/// Where a printer is: its address and, for `ipps`, the certificate the owner trusted
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Target {
    pub url: String,
    /// The SHA-256 of the trusted certificate, hex
    pub pin: Option<String>,
}

impl Target {
    pub fn new(url: &str, pin: Option<&str>) -> Self {
        Self {
            url: url.trim().to_string(),
            pin: pin.map(str::to_string),
        }
    }
}

/// How a document goes to a printer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doc {
    Pdf,
    /// Raster rendered at `dpi` and sent at `dpi × scale`
    Raster {
        format: super::raster::Format,
        dpi: u32,
        scale: u32,
    },
}

/// The resolution to render at for a printer that offers `offered` (dpi): the lowest at
/// 300 or more, else the highest; 600 and up is rendered at 300 and doubled (a quarter of
/// the memory, the same marks on paper)
fn resolution(offered: &[u32]) -> Option<(u32, u32)> {
    let best = offered
        .iter()
        .copied()
        .filter(|d| *d >= 300)
        .min()
        .or_else(|| offered.iter().copied().max())?;
    Some(if best >= 600 && best % 300 == 0 {
        (300, best / 300)
    } else {
        (best, 1)
    })
}

/// URF's resolutions: `RS300-600` → [300, 600]
fn urf_resolutions(urf: &[String]) -> Vec<u32> {
    urf.iter()
        .filter_map(|k| k.strip_prefix("RS"))
        .flat_map(|r| {
            r.split('-')
                .filter_map(|d| d.parse().ok())
                .collect::<Vec<u32>>()
        })
        .collect()
}

/// What to send a printer: PDF when it takes it (or `send` says), else PWG Raster, else URF;
/// or why nothing fits
pub fn choose(info: &PrinterInfo, send: &str) -> Result<Doc, String> {
    let takes =
        |m: &str| info.formats.iter().any(|f| f == m) || (m == "application/pdf" && info.pdf);
    let pwg = || {
        (takes("image/pwg-raster") && !info.pwg_resolutions.is_empty())
            .then(|| resolution(&info.pwg_resolutions))
            .flatten()
            .map(|(dpi, scale)| Doc::Raster {
                format: super::raster::Format::Pwg,
                dpi,
                scale,
            })
    };
    let urf = || {
        let rs = urf_resolutions(&info.urf);
        (takes("image/urf") && !rs.is_empty())
            .then(|| resolution(&rs))
            .flatten()
            .map(|(dpi, scale)| Doc::Raster {
                format: super::raster::Format::Urf,
                dpi,
                scale,
            })
    };
    let found = match send {
        "pdf" => takes("application/pdf").then_some(Doc::Pdf),
        "pwg" => pwg(),
        "urf" => urf(),
        _ => takes("application/pdf")
            .then_some(Doc::Pdf)
            .or_else(pwg)
            .or_else(urf),
    };
    found.ok_or_else(|| {
        let has = if info.formats.is_empty() {
            "nothing it said".to_string()
        } else {
            info.formats.join(", ")
        };
        match send {
            "pdf" | "pwg" | "urf" => format!("the printer cannot be sent {send} (it takes {has})"),
            _ => format!("the printer takes none of PDF, PWG Raster or URF (it takes {has})"),
        }
    })
}

/// Whether the printer prints colour in `doc`
pub fn prints_colour(info: &PrinterInfo, doc: Doc) -> bool {
    match doc {
        Doc::Pdf => info.color,
        Doc::Raster {
            format: super::raster::Format::Pwg,
            ..
        } => info.pwg_types.iter().any(|t| t.starts_with("srgb")),
        Doc::Raster {
            format: super::raster::Format::Urf,
            ..
        } => info.urf.iter().any(|k| k == "SRGB24"),
    }
}

/// The HTTP address an `ipp://` or `ipps://` URL is reached at (port 631 when none is given).
pub fn http_url(url: &str) -> Result<String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| anyhow!("a printer's address is ipp://host[:port]/path"))?;
    let http = match scheme {
        "ipp" | "http" => "http",
        "ipps" | "https" => "https",
        other => bail!("a printer's address is ipp:// or ipps://, not {other}://"),
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host.is_empty() {
        bail!("a printer's address names a host: ipp://192.168.1.143/ipp/print");
    }
    let has_port = if host.starts_with('[') {
        host.contains("]:")
    } else {
        host.contains(':')
    };
    // IPP's own port when none is given; plain http(s) keeps its default
    let host = if has_port || !matches!(scheme, "ipp" | "ipps") {
        host.to_string()
    } else {
        format!("{host}:631")
    };
    Ok(format!("{http}://{host}/{path}"))
}

/// Check the `label_printer_url` setting's value (settings.rs).
pub fn check_url(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    http_url(value.trim())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn uri(url: &str) -> Result<http::Uri> {
    url.parse::<http::Uri>()
        .with_context(|| format!("the printer's address {url:?}"))
}

/// The client for `ipps://` printers (plain `ipp://` goes through [`post_whole`])
async fn send(t: &Target, mut req: IppRequestResponse) -> Result<IppRequestResponse> {
    // IPP/2.0, as IPP Everywhere printers expect
    req.header_mut().version = ipp::model::IppVersion::v2_0();
    let mut body = Vec::new();
    req.into_read()
        .read_to_end(&mut body)
        .context("building the IPP request")?;
    let target = http_url(&t.url)?;
    let bytes = post_whole(&target, &body, t.pin.as_deref())
        .await
        .with_context(|| format!("could not reach the printer at {}", t.url))?;
    let reply = IppParser::new(std::io::Cursor::new(bytes))
        .parse()
        .context("the printer's answer is not IPP")?;
    let status = reply.header().status_code();
    if !status.is_success() {
        bail!("the printer refused: {status:?}");
    }
    Ok(reply)
}

/// POST `body` over plain HTTP/1.1, writing all of it before reading the answer, and return
/// the answer's body. The owner's M252dw answers a Print-Job before it has read the whole
/// document; an HTTP client that stops sending once the answer is in leaves the printer a
/// cut-off PDF, which it takes and then aborts (0 pages). curl and ipptool write it all,
/// and so does this.
async fn post_whole(target: &str, body: &[u8], pin: Option<&str>) -> Result<Vec<u8>> {
    let (tls, rest) = match target.strip_prefix("https://") {
        Some(r) => (true, r),
        None => (false, target.strip_prefix("http://").unwrap_or(target)),
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let tcp = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(host))
        .await
        .map_err(|_| anyhow!("no answer from {host} in 5 s"))??;
    let head = format!(
        "POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\nConnection: close\r\nUser-Agent: Roastwarden\r\n\r\n",
        body.len()
    );
    let done = if tls {
        let stream = tls::connect(tcp, host, pin).await?;
        tokio::time::timeout(Duration::from_secs(90), exchange(stream, &head, body)).await
    } else {
        tokio::time::timeout(Duration::from_secs(90), exchange(tcp, &head, body)).await
    };
    done.map_err(|_| anyhow!("the printer did not answer in 90 s"))?
}

async fn exchange<S>(mut stream: S, head: &str, body: &[u8]) -> Result<Vec<u8>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    stream.write_all(head.as_bytes()).await?;
    // A printer that answered early and closed its side makes the rest fail: the answer it
    // sent is still read below
    let _ = stream.write_all(body).await;
    let _ = stream.flush().await;
    read_answer(&mut stream).await
}

/// TLS to a printer: its certificate is trusted when its SHA-256 is the one the owner
/// pinned (name, expiry and issuer are not checked: printers' certificates are self-signed,
/// name no address and expire); the handshake's signatures are checked as usual.
pub mod tls {
    use std::fmt::Write as _;
    use std::sync::{Arc, Mutex};

    use anyhow::{Result, anyhow};
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::CryptoProvider;
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};
    use sha2::{Digest, Sha256};

    #[derive(Debug)]
    struct Pinned {
        want: Option<[u8; 32]>,
        seen: Mutex<Option<String>>,
        provider: Arc<CryptoProvider>,
    }

    pub fn hex(b: &[u8]) -> String {
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
    }

    fn unhex(s: &str) -> Option<[u8; 32]> {
        let s = s.replace(':', "");
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(out)
    }

    impl ServerCertVerifier for Pinned {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            let got: [u8; 32] = Sha256::digest(end_entity.as_ref()).into();
            if let Ok(mut s) = self.seen.lock() {
                *s = Some(hex(&got));
            }
            // In constant time
            let same = self
                .want
                .is_some_and(|w| w.iter().zip(got.iter()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0);
            if same {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General(
                    "not the printer certificate the owner trusted".into(),
                ))
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.provider
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    fn server_name(host: &str) -> Result<ServerName<'static>> {
        // host:port, [v6]:port
        let h = host
            .rsplit_once(':')
            .filter(|(h, p)| p.chars().all(|c| c.is_ascii_digit()) && !h.ends_with(':'))
            .map_or(host, |(h, _)| h);
        let h = h.trim_start_matches('[').trim_end_matches(']');
        ServerName::try_from(h.to_string()).map_err(|e| anyhow!("{h}: {e}"))
    }

    /// The handshake over `tcp`, trusting only the pinned certificate. Without a pin, or
    /// with another certificate, it fails and says the fingerprint the printer showed.
    pub async fn connect(
        tcp: tokio::net::TcpStream,
        host: &str,
        pin: Option<&str>,
    ) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Arc::new(Pinned {
            want: pin.and_then(unhex),
            seen: Mutex::new(None),
            provider: provider.clone(),
        });
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(verifier.clone())
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        match connector.connect(server_name(host)?, tcp).await {
            Ok(s) => Ok(s),
            Err(e) => {
                let seen = verifier.seen.lock().ok().and_then(|s| s.clone());
                Err(match (seen, pin) {
                    (Some(f), None) => anyhow!(
                        "the printer's certificate is not trusted yet: its SHA-256 is {f}; trust it (Label Studio → Printers) if that is your printer's"
                    ),
                    (Some(f), Some(_)) => anyhow!(
                        "the printer showed another certificate than the one trusted (its SHA-256 is now {f}): if the printer was reset, trust the new one"
                    ),
                    (None, _) => anyhow!(
                        "no TLS with the printer: {e} (printers that offer only RSA key exchange, as the HP M252dw does, cannot be reached over ipps: use ipp://)"
                    ),
                })
            }
        }
    }

    /// The fingerprint of the certificate a printer shows, for the owner to trust (nothing
    /// is sent)
    pub async fn fingerprint(url: &str) -> Result<String> {
        let target = super::http_url(url)?;
        let rest = target
            .strip_prefix("https://")
            .ok_or_else(|| anyhow!("only an ipps:// printer has a certificate"))?;
        let host = rest.split_once('/').map_or(rest, |(h, _)| h);
        let tcp = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(host),
        )
        .await
        .map_err(|_| anyhow!("no answer from {host} in 5 s"))??;
        match connect(tcp, host, None).await {
            Ok(_) => Err(anyhow!("the printer's certificate could not be read")),
            Err(e) => {
                let text = e.to_string();
                text.split("SHA-256 is ")
                    .nth(1)
                    .map(|f| f.split(';').next().unwrap_or(f).trim().to_string())
                    .ok_or(e)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fingerprints_read_with_or_without_colons() {
            let f = "ab".repeat(32);
            assert_eq!(unhex(&f), Some([0xab; 32]));
            let colons: Vec<String> = (0..32).map(|_| "AB".to_string()).collect();
            assert_eq!(unhex(&colons.join(":")), Some([0xab; 32]));
            assert_eq!(unhex("abc"), None);
            assert_eq!(hex(&[0, 255, 16]), "00ff10");
        }

        #[test]
        fn a_host_names_its_server() {
            assert!(server_name("192.168.1.143:443").is_ok());
            assert!(server_name("printer.local:631").is_ok());
            assert!(server_name("[fe80::1]:631").is_ok());
        }
    }
}

/// One HTTP/1.1 answer: its header, then as much body as it says (`Content-Length`, the
/// chunks up to the last, or to the end). A printer keeps the connection open after it
/// answers, `Connection: close` or not, so reading to the end would wait for its timeout.
async fn read_answer<S: tokio::io::AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 << 10];
    let split = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            bail!("the printer closed the connection without an answer");
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let header = String::from_utf8_lossy(&buf[..split]).to_string();
    let status_line = header.lines().next().unwrap_or("").to_string();
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    // A "100 Continue" first: skip it, the answer follows
    if code == 100 {
        let rest = buf[split + 4..].to_vec();
        let mut again = rest;
        loop {
            if again.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                bail!("the printer closed the connection without an answer");
            }
            again.extend_from_slice(&chunk[..n]);
        }
        buf = again;
        return Box::pin(finish(stream, buf)).await;
    }
    finish(stream, buf).await
}

async fn finish<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
    mut buf: Vec<u8>,
) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut chunk = [0u8; 16 << 10];
    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow!("the printer's answer has no HTTP header"))?;
    let header = String::from_utf8_lossy(&buf[..split]).to_ascii_lowercase();
    let status_line = header.lines().next().unwrap_or("").to_string();
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    if !(200..300).contains(&code) {
        bail!("the printer answered {status_line}");
    }
    let field = |name: &str| {
        header.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == name).then(|| v.trim().to_string())
        })
    };
    let mut body = buf.split_off(split + 4);
    if field("transfer-encoding").is_some_and(|t| t.contains("chunked")) {
        while !body.windows(5).any(|w| w == b"0\r\n\r\n") {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        return dechunk(&body);
    }
    if let Some(len) = field("content-length").and_then(|l| l.parse::<usize>().ok()) {
        while body.len() < len {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                bail!("a cut-off answer from the printer");
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body.truncate(len);
        return Ok(body);
    }
    stream.read_to_end(&mut body).await?;
    Ok(body)
}

/// An HTTP/1.1 chunked body, joined
fn dechunk(mut b: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = b
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| anyhow!("a cut-off chunked answer"))?;
        let size_text = String::from_utf8_lossy(&b[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| anyhow!("a bad chunk size {size_text:?}"))?;
        b = &b[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size {
            bail!("a cut-off chunked answer");
        }
        out.extend_from_slice(&b[..size]);
        b = b.get(size + 2..).unwrap_or(&[]);
    }
}

fn strings(v: &IppValue) -> Vec<String> {
    match v {
        IppValue::Array(a) => a.iter().flat_map(strings).collect(),
        IppValue::Collection(_) => vec![],
        other => vec![other.to_string()],
    }
}

/// Ask the printer what it can do and how it is.
pub async fn info(t: &Target) -> Result<PrinterInfo> {
    let url = t.url.as_str();
    let op = IppOperationBuilder::get_printer_attributes(uri(url)?)
        .attributes([
            "printer-make-and-model",
            "printer-state",
            "printer-state-reasons",
            "document-format-supported",
            "media-type-supported",
            "print-scaling-supported",
            "print-color-mode-supported",
            "media-source-supported",
            "media-ready",
            "media-col-ready",
            "pwg-raster-document-resolution-supported",
            "pwg-raster-document-type-supported",
            "urf-supported",
            "printer-uuid",
        ])
        .build()
        .map_err(|e| anyhow!("{e}"))?;
    let reply = send(t, op.into_ipp_request()).await?;
    let mut info = PrinterInfo {
        url: url.to_string(),
        ..PrinterInfo::default()
    };
    let Some(group) = reply.attributes().first_of(DelimiterTag::PrinterAttributes) else {
        return Ok(info);
    };
    let get = |name: &str| {
        group
            .get(name)
            .map(|a| strings(a.value()))
            .unwrap_or_default()
    };
    info.make_and_model = get("printer-make-and-model").into_iter().next();
    info.state = get("printer-state")
        .into_iter()
        .next()
        .map(|s| match s.as_str() {
            "3" => "idle".into(),
            "4" => "processing".into(),
            "5" => "stopped".into(),
            other => other.to_string(),
        });
    info.state_reasons = get("printer-state-reasons");
    info.formats = get("document-format-supported");
    info.pdf = info.formats.iter().any(|f| f == "application/pdf");
    info.pwg_resolutions = group
        .get("pwg-raster-document-resolution-supported")
        .map(|a| resolutions(a.value()))
        .unwrap_or_default();
    info.pwg_types = get("pwg-raster-document-type-supported");
    info.urf = get("urf-supported");
    info.color_modes = get("print-color-mode-supported");
    info.uuid = get("printer-uuid").into_iter().next();
    info.labels_media = get("media-type-supported").iter().any(|m| m == "labels");
    info.actual_size = get("print-scaling-supported").iter().any(|m| m == "none");
    info.color = get("print-color-mode-supported")
        .iter()
        .any(|m| m == "color");
    info.media_sources = get("media-source-supported");
    info.media_ready = get("media-ready");
    info.trays = group
        .get("media-col-ready")
        .map(|a| match a.value() {
            IppValue::Array(v) => v.iter().filter_map(tray).collect(),
            one => tray(one).into_iter().collect(),
        })
        .unwrap_or_default();
    Ok(info)
}

/// Resolutions as dots per inch (a printer may give them per cm)
fn resolutions(v: &IppValue) -> Vec<u32> {
    let one = |v: &IppValue| match v {
        IppValue::Resolution {
            cross_feed, units, ..
        } => Some(if *units == 4 {
            (f64::from(*cross_feed) * 2.54).round() as u32
        } else {
            *cross_feed as u32
        }),
        _ => None,
    };
    match v {
        IppValue::Array(list) => list.iter().filter_map(one).collect(),
        single => one(single).into_iter().collect(),
    }
}

fn member<'a>(
    c: &'a std::collections::BTreeMap<IppName, IppValue>,
    name: &str,
) -> Option<&'a IppValue> {
    c.iter().find(|(k, _)| k.as_str() == name).map(|(_, v)| v)
}

fn tray(v: &IppValue) -> Option<Tray> {
    let IppValue::Collection(c) = v else {
        return None;
    };
    let text = |name: &str| member(c, name).map(std::string::ToString::to_string);
    let size = match member(c, "media-size") {
        Some(IppValue::Collection(s)) => {
            let int = |n: &str| match member(s, n) {
                Some(IppValue::Integer(i)) => Some(*i),
                _ => None,
            };
            (int("x-dimension"), int("y-dimension"))
        }
        _ => (None, None),
    };
    Some(Tray {
        source: text("media-source"),
        width: size.0,
        height: size.1,
        media_type: text("media-type"),
    })
}

/// What a sent job became
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct Sent {
    pub job_id: Option<i32>,
    /// pending, processing, completed, … as the printer last said
    pub job_state: Option<String>,
    /// The printer's reasons (`none` when all is well)
    pub job_state_reasons: Vec<String>,
    /// Sheets printed so far
    pub sheets: Option<i32>,
    /// The tray the job asked for (the one holding paper of the sheet's size)
    pub tray: Option<String>,
}

fn job_state(v: &IppValue) -> String {
    match v {
        IppValue::Enum(3) => "pending".into(),
        IppValue::Enum(4) => "held".into(),
        IppValue::Enum(5) => "processing".into(),
        IppValue::Enum(6) => "stopped".into(),
        IppValue::Enum(7) => "canceled".into(),
        IppValue::Enum(8) => "aborted".into(),
        IppValue::Enum(9) => "completed".into(),
        other => other.to_string(),
    }
}

/// The paper description for a page of `w` × `h` inches: its size (the printer substitutes,
/// and the M252dw then aborts the job, when a `media-col` has a type but no size), the
/// "labels" type when offered, and the tray that holds paper of that size.
fn media_col(
    info: &PrinterInfo,
    w_in: f64,
    h_in: f64,
    asked: Option<&str>,
) -> Result<(IppValue, Option<String>)> {
    let name = |s: &str| IppName::new(s).map_err(|e| anyhow!("{e}"));
    let kw = |s: &str| {
        IppKeyword::new(s)
            .map(IppValue::Keyword)
            .map_err(|e| anyhow!("{e}"))
    };
    let (x, y) = (
        (w_in * 2540.0).round() as i32,
        (h_in * 2540.0).round() as i32,
    );
    let mut size = std::collections::BTreeMap::new();
    size.insert(name("x-dimension")?, IppValue::Integer(x));
    size.insert(name("y-dimension")?, IppValue::Integer(y));
    let mut col = std::collections::BTreeMap::new();
    col.insert(name("media-size")?, IppValue::Collection(size));
    if info.labels_media {
        col.insert(name("media-type")?, kw("labels")?);
    }
    // The tray asked for, when the printer has it; else the one holding paper of the size
    // (within 2 mm: printers round their sizes)
    let tray = asked
        .filter(|a| info.media_sources.iter().any(|s| s == a))
        .map(str::to_string)
        .or_else(|| {
            info.trays
                .iter()
                .find(|t| {
                    t.width.is_some_and(|w| (w - x).abs() <= 200)
                        && t.height.is_some_and(|h| (h - y).abs() <= 200)
                })
                .and_then(|t| t.source.clone())
        });
    if let Some(t) = &tray {
        col.insert(name("media-source")?, kw(t)?);
    }
    Ok((IppValue::Collection(col), tray))
}

/// What a print asks of the printer beyond the document
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A tray (`media-source`); none: the tray holding paper of the page's size
    pub tray: Option<String>,
    /// `monochrome` or `color` (`print-color-mode`), when the printer offers it
    pub color_mode: Option<String>,
}

/// Send a document of `w` × `h` inch pages (PDF, PWG Raster or URF, `mime`) to print at
/// actual size, on label stock when the printer offers it, then follow the job until the
/// printer has printed or given it up (a printer accepts a job before it reads it:
/// "accepted" is not "printed").
pub async fn print_doc(
    t: &Target,
    info: &PrinterInfo,
    doc: Vec<u8>,
    mime: &str,
    job_name: &str,
    (w_in, h_in): (f64, f64),
    opts: &Options,
) -> Result<Sent> {
    let kw = |s: &str| {
        IppKeyword::new(s)
            .map(IppValue::Keyword)
            .map_err(|e| anyhow!("{e}"))
    };
    let name = |s: &str| IppName::new(s).map_err(|e| anyhow!("{e}"));
    let mut attrs = Vec::new();
    if info.actual_size {
        attrs.push(IppAttribute::new(name("print-scaling")?, kw("none")?));
    }
    if let Some(mode) = opts
        .color_mode
        .as_deref()
        .filter(|m| info.color_modes.iter().any(|c| c == m))
    {
        attrs.push(IppAttribute::new(name("print-color-mode")?, kw(mode)?));
    }
    let (col, tray) = media_col(info, w_in, h_in, opts.tray.as_deref())?;
    attrs.push(IppAttribute::new(name("media-col")?, col));
    let op =
        IppOperationBuilder::print_job(uri(&t.url)?, IppPayload::new(std::io::Cursor::new(doc)))
            .document_format(mime)
            .job_title(job_name)
            .user_name("Roastwarden")
            .attributes(attrs)
            .build()
            .map_err(|e| anyhow!("{e}"))?;
    let reply = send(t, op.into_ipp_request()).await?;
    let job = reply.attributes().first_of(DelimiterTag::JobAttributes);
    let job_id = job
        .and_then(|g| g.get("job-id"))
        .and_then(|a| match a.value() {
            IppValue::Integer(i) => Some(*i),
            _ => None,
        });
    let mut sent = Sent {
        job_id,
        job_state: job
            .and_then(|g| g.get("job-state"))
            .map(|a| job_state(a.value())),
        job_state_reasons: vec![],
        sheets: None,
        tray,
    };
    if let Some(id) = job_id {
        follow(t, id, &mut sent).await;
    }
    match sent.job_state.as_deref() {
        Some("aborted" | "canceled") => bail!(
            "the printer took the job (number {}) and then {} it ({}): nothing printed",
            job_id.unwrap_or_default(),
            sent.job_state.as_deref().unwrap_or("dropped"),
            sent.job_state_reasons.join(", ")
        ),
        _ => Ok(sent),
    }
}

/// How long a print is followed before Roastwarden answers with the job's state so far
pub const FOLLOW_FOR: Duration = Duration::from_secs(45);

/// A job's state now, as its printer says: (state, reasons, sheets printed)
#[derive(Debug, Clone, PartialEq)]
pub struct JobNow {
    pub state: Option<String>,
    pub reasons: Vec<String>,
    pub sheets: Option<i32>,
}

impl JobNow {
    /// The printer is done with it, one way or another
    pub fn finished(&self) -> bool {
        matches!(
            self.state.as_deref(),
            Some("completed" | "aborted" | "canceled")
        )
    }
}

/// Get-Job-Attributes: how a job is now
pub async fn job_now(t: &Target, id: i32) -> Result<JobNow> {
    let op = IppOperationBuilder::get_job_attributes(uri(&t.url)?, id)
        .build()
        .map_err(|e| anyhow!("{e}"))?;
    let reply = send(t, op.into_ipp_request()).await?;
    let g = reply
        .attributes()
        .first_of(DelimiterTag::JobAttributes)
        .ok_or_else(|| anyhow!("the printer said nothing of job {id}"))?;
    Ok(JobNow {
        state: g.get("job-state").map(|a| job_state(a.value())),
        reasons: g
            .get("job-state-reasons")
            .map(|a| strings(a.value()))
            .unwrap_or_default(),
        sheets: match g
            .get("job-media-sheets-completed")
            .map(ipp::attribute::IppAttribute::value)
        {
            Some(IppValue::Integer(n)) => Some(*n),
            _ => None,
        },
    })
}

/// Ask after a job until it is done (completed, aborted, canceled) or stopped, or for
/// [`FOLLOW_FOR`].
async fn follow(t: &Target, id: i32, sent: &mut Sent) {
    let started = std::time::Instant::now();
    while started.elapsed() < FOLLOW_FOR {
        if matches!(
            sent.job_state.as_deref(),
            Some("completed" | "aborted" | "canceled" | "stopped")
        ) {
            return;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        let Ok(now) = job_now(t, id).await else {
            return;
        };
        if now.state.is_some() {
            sent.job_state = now.state;
        }
        if !now.reasons.is_empty() {
            sent.job_state_reasons = now.reasons;
        }
        if now.sheets.is_some() {
            sent.sheets = now.sheets;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_printer_address_is_reached_over_http() {
        assert_eq!(
            http_url("ipp://192.168.1.143/ipp/print").unwrap(),
            "http://192.168.1.143:631/ipp/print"
        );
        assert_eq!(
            http_url("ipp://192.168.1.143:8631/ipp/print").unwrap(),
            "http://192.168.1.143:8631/ipp/print"
        );
        assert_eq!(
            http_url("ipps://printer.local/ipp/print").unwrap(),
            "https://printer.local:631/ipp/print"
        );
        assert_eq!(
            http_url("ipp://[fe80::1]/ipp/print").unwrap(),
            "http://[fe80::1]:631/ipp/print"
        );
        assert!(http_url("lpd://x/queue").is_err());
        assert!(check_url("ipp:///ipp/print").is_err());
        assert!(check_url("").is_ok());
    }

    #[test]
    fn a_chunked_answer_is_joined() {
        assert_eq!(
            dechunk(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n").unwrap(),
            b"Wikipedia"
        );
        assert!(dechunk(b"9\r\nshort").is_err());
    }

    #[test]
    fn a_stopped_printer_says_why() {
        let mut p = PrinterInfo {
            pdf: true,
            state: Some("idle".into()),
            state_reasons: vec!["none".into()],
            ..PrinterInfo::default()
        };
        assert_eq!(p.problem(), None);
        p.state = Some("stopped".into());
        p.state_reasons = vec!["door-open-error".into()];
        assert!(p.problem().unwrap().contains("door-open-error"));
        p.pdf = false;
        assert!(p.problem().unwrap().contains("PDF"));
    }
}
