//! Read-only Claude desktop worker registrations; never an input authority.
use crate::{controller::ProcessIdentity, model::SessionState};
use serde_json::Value;

pub struct Registration {
    pub session: String,
    pub process: ProcessIdentity,
    pub activity: SessionState,
    pub status_unix: u64,
}

pub fn parse(bytes: &[u8], pid: u32) -> Option<Registration> {
    let r: Value = serde_json::from_slice(bytes).ok()?;
    if r["pid"].as_u64()? != u64::from(pid)
        || r["entrypoint"] != "claude-desktop"
        || r["pidDomain"] != "darwin"
    {
        return None;
    }
    let session = r["sessionId"].as_str()?;
    if !crate::viewer::is_uuid(session) {
        return None;
    }
    // Claude records ps lstart in UTC, independent of the desktop timezone.
    let format = time::format_description::parse("[weekday repr:short] [month repr:short] [day padding:space] [hour]:[minute]:[second] [year]").ok()?;
    let started = time::PrimitiveDateTime::parse(r["procStart"].as_str()?, &format)
        .ok()?
        .assume_utc()
        .unix_timestamp();
    let start = u64::try_from(started).ok()?.checked_mul(1_000_000)?;
    let status_unix = r["statusUpdatedAt"].as_u64().unwrap_or(0) / 1000;
    let activity = match r["status"].as_str() {
        Some("busy") => SessionState::Running,
        Some("idle") => SessionState::Idle,
        Some("waiting") => SessionState::WaitingApproval,
        _ => SessionState::Unknown,
    };
    Some(Registration {
        session: session.to_ascii_lowercase(),
        process: ProcessIdentity { pid, start },
        activity,
        status_unix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ties_session_to_pid_and_utc_launch_not_desktop_uptime() {
        let mut r = serde_json::json!({"pid":42,"sessionId":"77d21975-c3ee-410a-b515-a4929b11cc2b", "entrypoint":"claude-desktop", "pidDomain":"darwin", "procStart":"Tue Sep 29 01:28:03 2026", "status":"busy", "statusUpdatedAt":1790650649543_u64});
        let parse_value = |r: &Value, pid| parse(&serde_json::to_vec(r).unwrap(), pid);
        let found = parse_value(&r, 42).unwrap();
        assert_eq!(
            found.process.start,
            crate::model::parse_ts("2026-09-29T01:28:03Z") * 1_000_000
        );
        assert_eq!(found.activity, SessionState::Running);
        assert!(parse_value(&r, 43).is_none());
        r["entrypoint"] = "cli".into();
        assert!(parse_value(&r, 42).is_none());
    }
}
