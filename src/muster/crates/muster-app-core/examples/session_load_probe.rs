//! Read-only live acceptance probe. Uses a disposable cache and the desktop socket.
//! Usage: session_load_probe SOCKET TARGET_DEVICE [SESSION]
//! Reports counts and opaque timings; never prints transcript content or titles.
use muster_app_core::{AppConfig, AppCore, Mode, Shell, app};
use muster_proto::Message;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Capture {
    latest: Mutex<app::ViewModel>,
}
impl Shell for Capture {
    fn on_view(&self, bytes: &[u8]) {
        *self.latest.lock().unwrap() = app::ViewModel::decode(bytes).unwrap();
    }
    fn on_effect(&self, _: u64, bytes: &[u8]) {
        if let Some(app::effect::Ef::SendDiagnostic(record)) =
            app::Effect::decode(bytes).unwrap().ef
        {
            println!("{}", String::from_utf8_lossy(&record.json));
        }
    }
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        None
    }
    fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}
fn wait(shell: &Capture, check: impl Fn(&app::ViewModel) -> bool) -> app::ViewModel {
    wait_within(shell, Duration::from_secs(30), check)
        .expect("Timed out waiting for live core state")
}
fn wait_within(
    shell: &Capture,
    timeout: Duration,
    check: impl Fn(&app::ViewModel) -> bool,
) -> Option<app::ViewModel> {
    let until = Instant::now() + timeout;
    loop {
        let view = shell.latest.lock().unwrap().clone();
        if check(&view) {
            return Some(view);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn wait_history(
    shell: &Capture,
    previous: Option<&str>,
    timeout: Duration,
) -> Option<app::ViewModel> {
    wait_within(shell, timeout, |v| {
        v.screen == app::Screen::Chat as i32
            && v.session_load
                .as_ref()
                .is_some_and(|m| m.history_ready && previous != Some(m.trace_id.as_str()))
    })
}
fn counts(view: &app::ViewModel) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    if let Some(agents) = &view.agents {
        for row in agents
            .sections
            .iter()
            .flat_map(|s| &s.rows)
            .filter(|r| r.depth == 0)
        {
            if let Some(s) = &row.session {
                *counts.entry(s.provider.clone()).or_default() += 1;
            }
        }
    }
    counts
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "Usage: session_load_probe SOCKET TARGET_DEVICE [SESSION]"
    );
    let cache = tempfile::tempdir().unwrap();
    let shell = Arc::new(Capture::default());
    let core = AppCore::new(
        AppConfig {
            dir: cache.path().into(),
            mode: Mode::Socket {
                path: args[1].clone().into(),
            },
            allow_loopback: false,
        },
        shell.clone(),
    )
    .unwrap();
    wait(&shell, |v| v.screen == app::Screen::Agents as i32);
    core.intent(app::Intent {
        it: Some(app::intent::It::OpenAgents(app::OpenAgents {
            device_id: args[2].clone(),
        })),
    });
    let first = wait(&shell, |v| {
        v.agents
            .as_ref()
            .is_some_and(|a| a.device_id == args[2] && !a.loading && !a.sessions.is_empty())
    });
    let initial = counts(&first);
    assert!(initial.values().all(|n| *n <= 10));
    println!("{}", json!({"probe":"catalog_initial","counts":initial}));
    if let Some(section) = first
        .agents
        .as_ref()
        .unwrap()
        .sections
        .iter()
        .find(|s| s.has_more)
    {
        core.intent(app::Intent {
            it: Some(app::intent::It::AgentCatalogAction(
                app::AgentCatalogAction {
                    kind: app::agent_catalog_action::Kind::LoadMore as i32,
                    value: section.key.clone(),
                },
            )),
        });
        let next = wait(&shell, |v| {
            counts(v).values().sum::<usize>() > initial.values().sum::<usize>()
                && v.agents
                    .as_ref()
                    .is_some_and(|a| !a.loading && a.sections.iter().all(|s| !s.loading_more))
        });
        let more = counts(&next);
        assert!(more.values().all(|n| *n <= 20));
        println!("{}", json!({"probe":"catalog_more","counts":more}));
    }
    let session = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| first.agents.as_ref().unwrap().sessions[0].id.clone());
    for round in 0..3 {
        let old_trace = shell
            .latest
            .lock()
            .unwrap()
            .session_load
            .as_ref()
            .map(|m| m.trace_id.clone());
        core.intent(app::Intent {
            it: Some(app::intent::It::OpenAgent(app::OpenAgent {
                device_id: args[2].clone(),
                session: session.clone(),
            })),
        });
        let view = wait_history(&shell, old_trace.as_deref(), Duration::from_secs(30))
            .expect("Session entry did not apply fresh history");
        println!(
            "{}",
            json!({"probe":"history_ready","round":round,
                "trace_id":view.session_load.as_ref().map(|m| &m.trace_id),
                "messages":view.chat.as_ref().map(|c|c.message_count)})
        );
        core.lifecycle(app::Lifecycle::Background);
        std::thread::sleep(Duration::from_millis(200));
        let before_foreground = shell.latest.lock().unwrap().session_load.clone();
        core.lifecycle(app::Lifecycle::Foreground);
        let refreshed = wait_history(
            &shell,
            before_foreground.as_ref().map(|m| m.trace_id.as_str()),
            Duration::from_secs(30),
        )
        .expect("Foreground recovery did not apply fresh history");
        println!(
            "{}",
            json!({"probe":"foreground_history_ready","round":round,
            "trace_id":refreshed.session_load.as_ref().map(|m| &m.trace_id),
            "messages":refreshed.chat.as_ref().map(|c| c.message_count)})
        );
        core.intent(app::Intent {
            it: Some(app::intent::It::OpenAgents(app::OpenAgents {
                device_id: args[2].clone(),
            })),
        });
        wait(&shell, |v| v.screen == app::Screen::Agents as i32);
    }
    core.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(trace: &str, ready: bool) -> app::ViewModel {
        app::ViewModel {
            screen: app::Screen::Chat as i32,
            session_load: Some(app::SessionLoadMarker {
                trace_id: trace.into(),
                history_ready: ready,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn foreground_waits_for_delayed_fresh_history() {
        let shell = Arc::new(Capture {
            latest: Mutex::new(view("old", true)),
        });
        let capture = shell.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            tx.send(wait_history(&capture, Some("old"), Duration::from_secs(2)))
                .unwrap();
        });
        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        *shell.latest.lock().unwrap() = view("recovery", false);
        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        *shell.latest.lock().unwrap() = view("recovery", true);
        let finished = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(finished.session_load.unwrap().trace_id, "recovery");
        worker.join().unwrap();
    }

    #[test]
    fn unfinished_or_left_refresh_times_out() {
        for mut view in [
            view("old", true),
            view("recovery", false),
            view("recovery", true),
        ] {
            if view.session_load.as_ref().unwrap().trace_id == "recovery"
                && view.session_load.as_ref().unwrap().history_ready
            {
                view.screen = app::Screen::Agents as i32;
            }
            let shell = Capture {
                latest: Mutex::new(view),
            };
            assert!(wait_history(&shell, Some("old"), Duration::from_millis(10)).is_none());
        }
    }
}
