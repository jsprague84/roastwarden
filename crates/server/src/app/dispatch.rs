//! A control command sent through the transport registry, its outcome as an HTTP answer.

use std::time::Duration;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use roastwarden_core::DeviceCommand;
use serde::Deserialize;

use crate::app::state::AppState;
use crate::transports::transport::{SendOptions, TransportError};

/// How a command is sent (docs/control.md)
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct PublishOpts {
    /// Wait for the transport to acknowledge the command
    pub(crate) wait_ack: Option<bool>,
    /// How long to wait, ms
    pub(crate) timeout_ms: Option<u64>,
}

/// Send a control command through the transport registry and map the outcome to HTTP.
pub(crate) async fn dispatch(
    state: &AppState,
    device_id: &str,
    cmd: DeviceCommand,
    opts: &PublishOpts,
) -> Response {
    let send_opts = SendOptions {
        wait_ack: opts.wait_ack.unwrap_or(false),
        timeout: Duration::from_millis(opts.timeout_ms.unwrap_or(1000)),
    };
    match state.transports.send(device_id, &cmd, send_opts).await {
        Ok(via) => {
            if cmd.is_safety_critical() {
                tracing::info!(
                    device_id,
                    command = cmd.name(),
                    transport = via,
                    "safety command sent"
                );
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => transport_error_response(e),
    }
}

fn transport_error_response(e: TransportError) -> Response {
    let status = match &e {
        TransportError::NoTransport { .. } => StatusCode::SERVICE_UNAVAILABLE,
        TransportError::Unsupported { .. } | TransportError::NeedsTemplate { .. } => {
            StatusCode::NOT_IMPLEMENTED
        }
        TransportError::Send { .. } => StatusCode::BAD_GATEWAY,
        TransportError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        TransportError::ControlOff { .. } => {
            let mut r = (StatusCode::CONFLICT, e.to_string()).into_response();
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode("CONTROL_OFF"));
            return r;
        }
        TransportError::NotDeclared { .. } => {
            let mut r = (StatusCode::CONFLICT, e.to_string()).into_response();
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode("OUTPUT_NOT_DECLARED"));
            return r;
        }
    };
    tracing::warn!(error = %e, "command dispatch failed");
    (status, e.to_string()).into_response()
}
