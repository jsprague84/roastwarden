//! `roastwarden-server`: the server, configured from the environment (`.env` read first).

use tracing_subscriber::EnvFilter;

fn main() {
    // A label render in a child process (labels/sandbox.rs): no runtime, no .env, no logs
    if std::env::args().nth(1).as_deref() == Some(roastwarden_server::LABEL_WORKER_ARG) {
        std::process::exit(roastwarden_server::label_worker());
    }
    // The server takes no arguments. `--version` or `--help` typed into a running container
    // must not start a second server: its last will would tell every roaster the server is
    // gone (`roastwarden/server/status` offline) when it exits.
    if let Some(arg) = std::env::args().nth(1) {
        let version = env!("CARGO_PKG_VERSION");
        match arg.as_str() {
            "--version" | "-V" => println!("roastwarden-server {version}"),
            "--help" | "-h" => println!(
                "roastwarden-server {version}: the Roastwarden server, configured from the \
                 environment and .env (README, the variables table).\nIt takes no arguments. \
                 To manage a running server, use `roast` (roast --help)."
            ),
            other => {
                eprintln!("roastwarden-server takes no arguments (got {other:?}); see --help");
                std::process::exit(2);
            }
        }
        return;
    }
    dotenvy::dotenv().ok();
    // RUSTROAST_* from before the rename read as ROASTWARDEN_*
    // SAFETY: the process has one thread, the runtime is built below
    let adopted = unsafe { roastwarden_server::legacy::adopt_env() };
    init_tracing();
    if let Some(w) = roastwarden_server::legacy::env_warning(&adopted) {
        tracing::warn!("{w}");
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the async runtime")
        .block_on(roastwarden_server::run(shutdown_signal()));
}

fn init_tracing() {
    let mut filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info,axum=info,hyper=info,rumqttc=warn"))
        .expect("the default log filter parses");
    // The built-in broker (broker.rs) never logs passwords, whatever RUST_LOG asks: its ACL
    // plugin prints its rules, passwords included, at info, and rmqtt's debug lines can carry a
    // client's CONNECT.
    for d in [
        "rmqtt_acl=warn",
        "rmqtt=info",
        "rmqtt_net=info",
        "rmqtt_codec=info",
    ] {
        filter = filter.add_directive(d.parse().expect("a log directive"));
    }
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false).compact())
        // The same lines, kept in memory for the support bundle (logbuf.rs)
        .with(
            fmt::layer()
                .with_target(false)
                .compact()
                .with_ansi(false)
                .with_writer(roastwarden_server::logbuf::Ring),
        )
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("failed to install signal handler");
        term.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
