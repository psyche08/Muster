//! muster-logs: automatic log upload from desktops (ADR 0228).
//!
//! Once an hour musterd sends what its log files gained since the last
//! acknowledged upload ([`collect`]), redacted on the device ([`redact`])
//! and signed with the device key ([`wire`], [`upload`]), to the collector
//! beside the relay on muster.linsheng.dev. The collector (`server` feature,
//! `muster-logs serve`) verifies each upload, drops ranges it already holds,
//! and keeps the logs grouped by network, then device, then day.

pub mod collect;
pub mod redact;
#[cfg(feature = "server")]
pub mod server;
pub mod upload;
pub mod wire;
