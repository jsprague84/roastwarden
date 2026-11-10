//! Every label render runs in a child process: the server's own binary started as
//! `roastwarden-server label-render`, which reads one [`Request`] (CBOR) on stdin, writes one
//! [`Reply`] on stdout and exits. Designs are code (an agent's included) and Typst has no
//! step limit, so a loop that never ends would hold a thread for good and a huge layout
//! would take the server's memory. The worker limits itself before it reads the request
//! (address space, CPU seconds), and the server kills it after [`TIME_LIMIT`]. It gets no
//! environment, no files and no network: the request carries everything (engine.rs).
//!
//! Tests and `ROASTWARDEN_LABEL_SANDBOX=0` render in the server process instead.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use super::engine::{self, Diag, Reply, Request};

/// The argument that makes the server binary a render worker
pub const WORKER_ARG: &str = "label-render";
/// Wall-clock time a render may take before the worker is killed
pub const TIME_LIMIT: Duration = Duration::from_secs(10);
/// CPU seconds the worker may use (a backstop to the wall clock)
const CPU_SECONDS: u64 = 15;
/// The worker's address space: the binary, the fonts, Typst and the document
pub const MEMORY_LIMIT: u64 = 1 << 30;
/// Renders at once; more wait their turn
const AT_ONCE: usize = 2;
/// The largest request the worker reads (the owner's fonts and artwork are in it)
const MAX_REQUEST: u64 = 128 << 20;

/// Where renders run.
#[derive(Clone)]
pub struct Renderer {
    /// The binary to start as the worker; none renders in this process
    worker: Option<PathBuf>,
    permits: Arc<Semaphore>,
    time_limit: Duration,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("worker", &self.worker)
            .finish_non_exhaustive()
    }
}

fn failed(message: impl Into<String>) -> Reply {
    Reply::Failed {
        errors: vec![Diag {
            file: None,
            line: None,
            column: None,
            from: None,
            to: None,
            message: message.into(),
            hints: vec![],
        }],
        warnings: vec![],
    }
}

impl Renderer {
    /// In this process (tests; `ROASTWARDEN_LABEL_SANDBOX=0`)
    pub fn in_process() -> Self {
        Self {
            worker: None,
            permits: Arc::new(Semaphore::new(AT_ONCE)),
            time_limit: TIME_LIMIT,
        }
    }

    /// In a child process of `exe`, which must answer [`WORKER_ARG`]
    pub fn worker(exe: PathBuf) -> Self {
        Self {
            worker: Some(exe),
            permits: Arc::new(Semaphore::new(AT_ONCE)),
            time_limit: TIME_LIMIT,
        }
    }

    /// A shorter wall clock (tests)
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_time_limit(mut self, limit: Duration) -> Self {
        self.time_limit = limit;
        self
    }

    /// The server binary as its own worker, unless `ROASTWARDEN_LABEL_SANDBOX=0`.
    pub fn from_env() -> Self {
        let off =
            std::env::var("ROASTWARDEN_LABEL_SANDBOX").is_ok_and(|v| v == "0" || v == "false");
        match std::env::current_exe() {
            Ok(exe) if !off => Self::worker(exe),
            _ => Self::in_process(),
        }
    }

    pub fn sandboxed(&self) -> bool {
        self.worker.is_some()
    }

    pub async fn render(&self, req: Request) -> Reply {
        let Ok(_permit) = self.permits.acquire().await else {
            return failed("the label renderer is shutting down");
        };
        match &self.worker {
            None => tokio::task::spawn_blocking(move || engine::execute(&req))
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(error = %e, "label render panicked");
                    failed("the render failed")
                }),
            Some(exe) => run_worker(exe, &req, self.time_limit).await,
        }
    }
}

async fn run_worker(exe: &PathBuf, req: &Request, limit: Duration) -> Reply {
    let mut input = Vec::new();
    if let Err(e) = ciborium::into_writer(req, &mut input) {
        return Reply::Bad(format!("could not encode the render: {e}"));
    }
    let spawned = tokio::process::Command::new(exe)
        .arg(WORKER_ARG)
        .env_clear()
        // Fewer malloc arenas and one helper thread: less address space to limit
        .env("MALLOC_ARENA_MAX", "2")
        .env("RAYON_NUM_THREADS", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, exe = %exe.display(), "could not start the label worker");
            return failed(format!("could not start the label renderer: {e}"));
        }
    };
    let mut stdin = child.stdin.take().expect("piped");
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
        // Dropping stdin closes it: the worker reads to the end
    });
    // On a timeout the future (and with it the child) is dropped: kill_on_drop kills it
    let out = tokio::time::timeout(limit, child.wait_with_output()).await;
    writer.abort();
    let out = match out {
        Err(_) => {
            return failed(format!(
                "the design took longer than {} s to lay out and was stopped: look for loops over huge ranges (a `for` over range(100000) inside another)",
                limit.as_secs_f64()
            ));
        }
        Ok(Err(e)) => return failed(format!("the label renderer failed: {e}")),
        Ok(Ok(o)) => o,
    };
    if out.status.success() {
        return ciborium::from_reader::<Reply, _>(out.stdout.as_slice())
            .unwrap_or_else(|e| failed(format!("the label renderer answered nonsense: {e}")));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    use std::os::unix::process::ExitStatusExt;
    let message = if stderr.contains("memory allocation") || stderr.contains("out of memory") {
        format!(
            "the design needed more than {} MB and was stopped: something grows without end (a huge repeat, an enormous image?)",
            MEMORY_LIMIT >> 20
        )
    } else if out.status.signal() == Some(24) {
        format!("the design used more than {CPU_SECONDS} s of CPU and was stopped")
    } else {
        tracing::warn!(status = %out.status, stderr = %stderr.trim(), "label worker failed");
        format!("the label renderer stopped ({})", out.status)
    };
    failed(message)
}

/// The worker: limit this process, read one request, answer it. Returns the exit code.
pub fn worker_main() -> i32 {
    // Hard limits too, so the design cannot raise them back
    let limited = rlimit::setrlimit(rlimit::Resource::AS, MEMORY_LIMIT, MEMORY_LIMIT)
        .and_then(|()| rlimit::setrlimit(rlimit::Resource::CPU, CPU_SECONDS, CPU_SECONDS + 1))
        .and_then(|()| rlimit::setrlimit(rlimit::Resource::NOFILE, 16, 16))
        .and_then(|()| rlimit::setrlimit(rlimit::Resource::FSIZE, 0, 0));
    if let Err(e) = limited {
        eprintln!("label-render: could not set limits: {e}");
        return 2;
    }
    let mut input = Vec::new();
    if let Err(e) = std::io::stdin().take(MAX_REQUEST).read_to_end(&mut input) {
        eprintln!("label-render: reading the request: {e}");
        return 2;
    }
    let reply = match ciborium::from_reader::<Request, _>(input.as_slice()) {
        Ok(req) => engine::execute(&req),
        Err(e) => Reply::Bad(format!("the render request: {e}")),
    };
    let mut out = Vec::new();
    if ciborium::into_writer(&reply, &mut out).is_err() {
        return 2;
    }
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&out)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return 2;
    }
    0
}
