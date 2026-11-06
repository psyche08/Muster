//! Read-only, content-free diagnostic context. Never used for authorization.
use crate::v1::request::Req;
pub fn valid_id(id: &str) -> bool {
    (16..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
}
pub fn request_id(req: &Req) -> Option<&str> {
    let id = match req {
        Req::AgentWatch(r) => &r.diagnostic_id,
        Req::AgentMessages(r) => &r.diagnostic_id,
        _ => return None,
    };
    valid_id(id).then_some(id.as_str())
}
