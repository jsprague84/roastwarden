//! The server's assembly: its services started, the router built, the state they share, the
//! WebSockets, the MQTT consumer and command dispatch.

pub(crate) mod build;
pub(crate) mod consumer;
pub(crate) mod dispatch;
pub(crate) mod state;
pub(crate) mod ws;
