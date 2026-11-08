//! HTTP plumbing shared by every route: the JSON error body, optional JSON bodies, request ids,
//! security headers and signed-in browsers.

pub(crate) mod errors;
pub(crate) mod logins;
pub(crate) mod opt_json;
pub(crate) mod patch;
pub(crate) mod request_id;
pub(crate) mod security;
