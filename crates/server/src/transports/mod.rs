//! How the server talks to roasters: the transport registry, Modbus, TC4 serial, Phidget22,
//! a device's own field mapping and the pollers that run a connection each.

pub(crate) mod ingest;
pub(crate) mod modbus;
pub(crate) mod modbus_ascii;
pub(crate) mod phidget;
pub(crate) mod pollers;
pub(crate) mod tc4;
pub(crate) mod transport;
pub(crate) mod types;
