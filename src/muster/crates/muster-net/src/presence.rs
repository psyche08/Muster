//! Whether each member is online, from evidence rather than from the instant
//! state of one link (ARCHITECTURE §3.5, Presence).
//!
//! - **Online evidence**: a link comes up (adopted), traffic arrives from the
//!   member, or the relay reports that it registered (`PRESENT`).
//! - **A lost link** is not yet offline. The member stays shown online while
//!   at most [`MAX_RETRIES`] reconnects run; only when they all fail is it
//!   offline (`Unreachable`).
//! - **The relay's authority** (`ABSENT`: not registered past its
//!   re-registration grace) makes it offline at once, unless the member's own
//!   traffic is fresher. An absent member is not dialed again until new
//!   evidence (`PRESENT`, its traffic, a link it opens) or a user refresh.
//! - **This device's own outage** (no relay, or the app suspended) is not
//!   evidence about anyone: failures then count for nothing.
//!
//! Pure and clock-free: every input carries its `Instant`, so tests drive
//! time. The node feeds it and asks [`Presence::may_dial`] before dialing.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use muster_core::DeviceId;

/// Reconnects after a lost link (or a first contact) before offline.
pub const MAX_RETRIES: u32 = 2;
/// The member's own traffic this recent outranks a relay's absence report
/// (the two can cross on different carriers).
pub const FRESH_ACTIVITY: Duration = Duration::from_secs(2);
/// An unreachable member (no relay authority) is tried once this often.
pub const UNREACHABLE_PROBE: Duration = Duration::from_secs(60);
/// A member this device never dials (a phone, seen from a desktop) that does
/// not link again within this long after losing its link is unreachable.
pub const PASSIVE_GRACE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The relay says it is not registered.
    Absent,
    /// Every bounded reconnect failed while this device was connected.
    Unreachable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not established yet, or asked again (refresh, relay presence).
    Unknown {
        failures: u32,
    },
    Online,
    /// The link dropped: shown online while bounded reconnects run.
    Reconnecting {
        failures: u32,
    },
    Offline {
        reason: Reason,
    },
}

/// This device's own connection, as far as presence cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Local {
    /// Registered with the relay (or no relay is configured).
    #[default]
    Up,
    /// Registering: failures do not count yet, nothing is shown as lost.
    Connecting,
    /// The relay cannot be reached: nothing is known about anyone.
    Down,
}

/// What a member list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Online,
    Reconnecting,
    /// Not known yet (being connected).
    Connecting,
    /// The relay says it is not registered.
    Offline,
    /// It did not answer its bounded reconnects.
    Unreachable,
    /// This device has no relay connection: nothing is known about others.
    LocalOffline,
}

impl View {
    pub fn as_str(self) -> &'static str {
        match self {
            View::Online => "online",
            View::Reconnecting => "reconnecting",
            View::Connecting => "connecting",
            View::Offline => "offline",
            View::Unreachable => "unreachable",
            View::LocalOffline => "local_offline",
        }
    }
    /// Shown and used as online: a reconnect within its budget is not a
    /// disconnection.
    pub fn online(self) -> bool {
        matches!(self, View::Online | View::Reconnecting)
    }
    /// Its sessions are hidden and it is not polled.
    pub fn offline(self) -> bool {
        matches!(self, View::Offline | View::Unreachable)
    }
}

#[derive(Clone, Copy, Debug)]
struct Peer {
    state: State,
    /// When it entered `state`.
    since: Instant,
    /// The newest traffic seen from the member.
    activity: Option<Instant>,
    /// When an unreachable member may be tried again.
    next_probe: Option<Instant>,
}

/// One device's view of every member it talks to.
#[derive(Debug, Default)]
pub struct Presence {
    peers: HashMap<DeviceId, Peer>,
    local: Local,
    /// The app is suspended (a phone in the background).
    suspended: bool,
}

/// A change worth telling the member list about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub peer: DeviceId,
    pub from: State,
    pub to: State,
}

impl Presence {
    pub fn new(local: Local) -> Self {
        Self {
            local,
            ..Default::default()
        }
    }

    fn peer(&mut self, id: DeviceId) -> &mut Peer {
        self.peers.entry(id).or_insert(Peer {
            state: State::Unknown { failures: 0 },
            since: Instant::now(),
            activity: None,
            next_probe: None,
        })
    }

    fn set(&mut self, id: DeviceId, to: State, now: Instant) -> Option<Change> {
        let peer = self.peer(id);
        let from = peer.state;
        if from == to {
            return None;
        }
        peer.state = to;
        peer.since = now;
        peer.next_probe = matches!(
            to,
            State::Offline {
                reason: Reason::Unreachable
            }
        )
        .then(|| now + UNREACHABLE_PROBE);
        Some(Change { peer: id, from, to })
    }

    fn saw(&mut self, id: DeviceId, now: Instant) {
        let peer = self.peer(id);
        peer.activity = Some(peer.activity.map_or(now, |a| a.max(now)));
    }

    pub fn state(&self, id: &DeviceId) -> State {
        self.peers
            .get(id)
            .map_or(State::Unknown { failures: 0 }, |p| p.state)
    }

    /// A member link was adopted: the strongest online evidence.
    pub fn link_up(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        self.saw(id, now);
        self.set(id, State::Online, now)
    }

    /// The current link closed. A reconnect is due; the member stays shown
    /// online for its bounded retries.
    pub fn link_lost(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        match self.state(&id) {
            State::Online => self.set(id, State::Reconnecting { failures: 0 }, now),
            _ => None,
        }
    }

    /// Traffic from the member: it is there.
    pub fn activity(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        self.saw(id, now);
        match self.state(&id) {
            State::Offline { .. } => self.set(id, State::Unknown { failures: 0 }, now),
            _ => None,
        }
    }

    /// A dial (first contact or reconnect) failed. Counted only while this
    /// device is connected and running: our own outage says nothing about
    /// the member. A late failure after a link came up changes nothing.
    pub fn dial_failed(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        if self.local != Local::Up || self.suspended {
            return None;
        }
        let next = match self.state(&id) {
            State::Unknown { failures } | State::Reconnecting { failures }
                if failures >= MAX_RETRIES =>
            {
                State::Offline {
                    reason: Reason::Unreachable,
                }
            }
            State::Unknown { failures } => State::Unknown {
                failures: failures + 1,
            },
            State::Reconnecting { failures } => State::Reconnecting {
                failures: failures + 1,
            },
            State::Offline {
                reason: Reason::Unreachable,
            } => {
                // Its periodic probe failed: wait for the next one.
                self.peer(id).next_probe = Some(now + UNREACHABLE_PROBE);
                return None;
            }
            State::Online | State::Offline { .. } => return None,
        };
        self.set(id, next, now)
    }

    /// The relay's report that the member is not registered. Fresher
    /// traffic from the member outranks it (the report may be stale).
    pub fn absent(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        if self
            .peer(id)
            .activity
            .is_some_and(|a| now.saturating_duration_since(a) < FRESH_ACTIVITY)
        {
            return None;
        }
        self.set(
            id,
            State::Offline {
                reason: Reason::Absent,
            },
            now,
        )
    }

    /// The relay saw the member register.
    pub fn present(&mut self, id: DeviceId, now: Instant) -> Option<Change> {
        match self.state(&id) {
            State::Offline { .. } => self.set(id, State::Unknown { failures: 0 }, now),
            _ => None,
        }
    }

    /// The user asked to refresh: every offline member gets one more round
    /// of bounded attempts.
    pub fn refresh(&mut self, now: Instant) -> Vec<Change> {
        let offline: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, p)| matches!(p.state, State::Offline { .. }))
            .map(|(id, _)| *id)
            .collect();
        offline
            .into_iter()
            .filter_map(|id| self.set(id, State::Unknown { failures: 0 }, now))
            .collect()
    }

    /// Back from the background: members found unreachable meanwhile get one
    /// more round (they may only have missed our suspended reconnects). An
    /// absent one still waits for the relay's word.
    pub fn resume(&mut self, now: Instant) -> Vec<Change> {
        self.suspended = false;
        self.retry_unreachable(now)
    }

    /// This device's network changed: members it could not reach get one
    /// more round, as our old path may have been the problem. The relay's
    /// absence reports stand.
    pub fn retry_unreachable(&mut self, now: Instant) -> Vec<Change> {
        let unreachable: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, p)| {
                p.state
                    == State::Offline {
                        reason: Reason::Unreachable,
                    }
            })
            .map(|(id, _)| *id)
            .collect();
        unreachable
            .into_iter()
            .filter_map(|id| self.set(id, State::Unknown { failures: 0 }, now))
            .collect()
    }

    /// Members this device never dials (`dials` says false) cannot run out
    /// of retries: one that lost its link and has not linked again within
    /// [`PASSIVE_GRACE`] is unreachable.
    pub fn expire(&mut self, now: Instant, dials: impl Fn(&DeviceId) -> bool) -> Vec<Change> {
        if self.local != Local::Up || self.suspended {
            return Vec::new();
        }
        let lapsed: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(id, p)| {
                matches!(p.state, State::Reconnecting { .. })
                    && now.saturating_duration_since(p.since) >= PASSIVE_GRACE
                    && !dials(id)
            })
            .map(|(id, _)| *id)
            .collect();
        lapsed
            .into_iter()
            .filter_map(|id| {
                self.set(
                    id,
                    State::Offline {
                        reason: Reason::Unreachable,
                    },
                    now,
                )
            })
            .collect()
    }

    pub fn suspend(&mut self) {
        self.suspended = true;
    }

    pub fn suspended(&self) -> bool {
        self.suspended
    }

    pub fn set_local(&mut self, local: Local) {
        self.local = local;
    }

    pub fn local(&self) -> Local {
        self.local
    }

    /// Whether a (re)connect may be tried now. An absent member is never
    /// dialed; an unreachable one only at its probe time.
    pub fn may_dial(&self, id: &DeviceId, now: Instant) -> bool {
        let Some(peer) = self.peers.get(id) else {
            return true;
        };
        match peer.state {
            State::Offline {
                reason: Reason::Absent,
            } => false,
            State::Offline {
                reason: Reason::Unreachable,
            } => peer.next_probe.is_some_and(|at| now >= at),
            _ => true,
        }
    }

    /// Members the relay has declared absent: the ones to WATCH.
    pub fn absent_members(&self) -> Vec<DeviceId> {
        self.peers
            .iter()
            .filter(|(_, p)| {
                p.state
                    == State::Offline {
                        reason: Reason::Absent,
                    }
            })
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn view(&self, id: &DeviceId) -> View {
        match self.state(id) {
            State::Online => View::Online,
            State::Reconnecting { .. } => View::Reconnecting,
            State::Offline {
                reason: Reason::Absent,
            } => View::Offline,
            State::Offline {
                reason: Reason::Unreachable,
            } => View::Unreachable,
            State::Unknown { .. } if self.local == Local::Down => View::LocalOffline,
            State::Unknown { .. } => View::Connecting,
        }
    }
}

/// [`Presence`] shared by a node and its socket, on the real clock. Each
/// change is queued for the node's presence task (logging, closing a link
/// the relay declared absent, watching it).
#[derive(Debug)]
pub struct Board {
    inner: std::sync::Mutex<Presence>,
    changes: tokio::sync::mpsc::UnboundedSender<Change>,
}

impl Board {
    pub fn new(
        local: Local,
    ) -> (
        std::sync::Arc<Self>,
        tokio::sync::mpsc::UnboundedReceiver<Change>,
    ) {
        let (changes, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            std::sync::Arc::new(Self {
                inner: std::sync::Mutex::new(Presence::new(local)),
                changes,
            }),
            rx,
        )
    }

    fn apply(&self, f: impl FnOnce(&mut Presence, Instant) -> Option<Change>) {
        let change = f(&mut self.inner.lock().unwrap(), Instant::now());
        if let Some(change) = change {
            let _ = self.changes.send(change);
        }
    }

    fn apply_all(&self, f: impl FnOnce(&mut Presence, Instant) -> Vec<Change>) {
        for change in f(&mut self.inner.lock().unwrap(), Instant::now()) {
            let _ = self.changes.send(change);
        }
    }

    pub fn link_up(&self, id: DeviceId) {
        self.apply(|p, now| p.link_up(id, now));
    }
    pub fn link_lost(&self, id: DeviceId) {
        self.apply(|p, now| p.link_lost(id, now));
    }
    pub fn activity(&self, id: DeviceId) {
        self.apply(|p, now| p.activity(id, now));
    }
    pub fn dial_failed(&self, id: DeviceId) {
        self.apply(|p, now| p.dial_failed(id, now));
    }
    pub fn absent(&self, id: DeviceId) {
        self.apply(|p, now| p.absent(id, now));
    }
    pub fn present(&self, id: DeviceId) {
        self.apply(|p, now| p.present(id, now));
    }
    pub fn refresh(&self) {
        self.apply_all(|p, now| p.refresh(now));
    }
    pub fn resume(&self) {
        self.apply_all(|p, now| p.resume(now));
    }
    pub fn retry_unreachable(&self) {
        self.apply_all(|p, now| p.retry_unreachable(now));
    }
    pub fn expire(&self, dials: impl Fn(&DeviceId) -> bool) {
        self.apply_all(|p, now| p.expire(now, dials));
    }
    pub fn suspend(&self) {
        self.inner.lock().unwrap().suspend();
    }
    pub fn suspended(&self) -> bool {
        self.inner.lock().unwrap().suspended()
    }
    pub fn set_local(&self, local: Local) {
        self.inner.lock().unwrap().set_local(local);
    }
    pub fn may_dial(&self, id: &DeviceId) -> bool {
        self.inner.lock().unwrap().may_dial(id, Instant::now())
    }
    pub fn view(&self, id: &DeviceId) -> View {
        self.inner.lock().unwrap().view(id)
    }
    pub fn state(&self, id: &DeviceId) -> State {
        self.inner.lock().unwrap().state(id)
    }
    pub fn absent_members(&self) -> Vec<DeviceId> {
        self.inner.lock().unwrap().absent_members()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> DeviceId {
        DeviceId([n; 32])
    }

    fn secs(t: Instant, s: u64) -> Instant {
        t + Duration::from_secs(s)
    }

    #[test]
    fn a_dropped_link_that_comes_back_is_never_shown_offline() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let mac = id(1);
        assert_eq!(p.view(&mac), View::Connecting);
        p.link_up(mac, t);
        assert_eq!(p.view(&mac), View::Online);
        // One missed heartbeat (the link timed out), then one failed retry.
        p.link_lost(mac, secs(t, 20));
        assert!(p.view(&mac).online());
        p.dial_failed(mac, secs(t, 23));
        assert_eq!(p.view(&mac), View::Reconnecting);
        assert!(p.may_dial(&mac, secs(t, 24)));
        p.link_up(mac, secs(t, 25));
        assert_eq!(p.view(&mac), View::Online);
        // The budget starts over with the next loss.
        p.link_lost(mac, secs(t, 40));
        assert_eq!(p.state(&mac), State::Reconnecting { failures: 0 });
    }

    #[test]
    fn two_failed_retries_after_a_loss_make_it_unreachable() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let mac = id(1);
        p.link_up(mac, t);
        p.link_lost(mac, t);
        // The first reconnect, then MAX_RETRIES more.
        for n in 0..MAX_RETRIES {
            p.dial_failed(mac, secs(t, u64::from(n) + 1));
            assert_eq!(p.view(&mac), View::Reconnecting);
        }
        let change = p.dial_failed(mac, secs(t, 9)).unwrap();
        assert_eq!(
            change.to,
            State::Offline {
                reason: Reason::Unreachable
            }
        );
        assert_eq!(p.view(&mac), View::Unreachable);
        assert!(p.view(&mac).offline());
        // Tried again only once a minute; a failed probe waits another.
        assert!(!p.may_dial(&mac, secs(t, 30)));
        let probe = secs(t, 9) + UNREACHABLE_PROBE;
        assert!(p.may_dial(&mac, probe));
        assert!(p.dial_failed(mac, probe).is_none());
        assert!(!p.may_dial(&mac, probe + Duration::from_secs(1)));
        assert_eq!(p.view(&mac), View::Unreachable);
        // A link it opens (or a probe that works) brings it back.
        p.link_up(mac, probe + Duration::from_secs(70));
        assert_eq!(p.view(&mac), View::Online);
    }

    #[test]
    fn the_relays_absence_report_stops_dialing_until_new_evidence() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let mac = id(1);
        p.link_up(mac, t);
        p.link_lost(mac, secs(t, 10));
        let change = p.absent(mac, secs(t, 11)).unwrap();
        assert_eq!(change.from, State::Reconnecting { failures: 0 });
        assert_eq!(p.view(&mac), View::Offline);
        assert_eq!(p.absent_members(), [mac]);
        // No dial, now or later: not even a periodic probe.
        for s in [12, 100, 1000] {
            assert!(!p.may_dial(&mac, secs(t, s)));
        }
        // A failure from a dial started earlier changes nothing.
        assert!(p.dial_failed(mac, secs(t, 12)).is_none());
        // Back on the relay's PRESENT: dialed again, shown as connecting.
        p.present(mac, secs(t, 1000));
        assert_eq!(p.view(&mac), View::Connecting);
        assert!(p.may_dial(&mac, secs(t, 1000)));
        assert!(p.absent_members().is_empty());
    }

    #[test]
    fn a_late_absence_report_cannot_override_newer_traffic_or_a_new_link() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let mac = id(1);
        p.link_up(mac, t);
        // The report crossed fresher traffic: stale.
        p.activity(mac, secs(t, 5));
        assert!(p.absent(mac, t + Duration::from_millis(6500)).is_none());
        assert_eq!(p.view(&mac), View::Online);
        // Long after the last traffic it stands.
        assert!(p.absent(mac, secs(t, 30)).is_some());
        // A link adopted afterwards is newer still.
        p.link_up(mac, secs(t, 31));
        assert_eq!(p.view(&mac), View::Online);
        assert!(p.absent(mac, secs(t, 32)).is_none());
    }

    #[test]
    fn traffic_or_a_refresh_brings_an_offline_member_back_to_trying() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let (a, b) = (id(1), id(2));
        p.absent(a, t);
        p.absent(b, t);
        p.activity(a, secs(t, 5));
        assert_eq!(p.view(&a), View::Connecting);
        let changes = p.refresh(secs(t, 6));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].peer, b);
        assert!(p.may_dial(&b, secs(t, 6)));
    }

    #[test]
    fn our_own_outage_or_suspension_is_no_evidence_about_others() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let mac = id(1);
        p.link_up(mac, t);
        p.link_lost(mac, t);
        p.set_local(Local::Connecting);
        assert!(p.dial_failed(mac, secs(t, 1)).is_none());
        assert_eq!(
            p.view(&id(3)),
            View::Connecting,
            "registering is not offline"
        );
        p.set_local(Local::Down);
        for n in 1..10 {
            assert!(p.dial_failed(mac, secs(t, n)).is_none());
        }
        assert_eq!(p.view(&mac), View::Reconnecting);
        let never = id(2);
        assert_eq!(p.view(&never), View::LocalOffline);
        p.set_local(Local::Up);
        // In the background a phone's dials fail by design.
        p.suspend();
        for n in 10..20 {
            assert!(p.dial_failed(mac, secs(t, n)).is_none());
        }
        assert_eq!(p.view(&mac), View::Reconnecting);
    }

    #[test]
    fn a_member_never_dialed_is_unreachable_after_the_passive_grace() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let (phone, mac) = (id(1), id(2));
        for member in [phone, mac] {
            p.link_up(member, t);
            p.link_lost(member, secs(t, 1));
        }
        let dials = |member: &DeviceId| *member == mac;
        assert!(p.expire(secs(t, 30), dials).is_empty());
        let changes = p.expire(secs(t, 31), dials);
        assert_eq!(changes.len(), 1);
        assert_eq!(p.view(&phone), View::Unreachable);
        // The one this device dials stays with its retry budget.
        assert_eq!(p.view(&mac), View::Reconnecting);
        // The phone links again: online.
        p.link_up(phone, secs(t, 40));
        assert_eq!(p.view(&phone), View::Online);
    }

    #[test]
    fn the_foreground_retries_unreachable_but_not_absent_members() {
        let t = Instant::now();
        let mut p = Presence::new(Local::Up);
        let (gone, quiet) = (id(1), id(2));
        p.absent(gone, t);
        for n in 0..=MAX_RETRIES {
            p.dial_failed(quiet, secs(t, u64::from(n)));
        }
        assert_eq!(p.view(&quiet), View::Unreachable);
        p.suspend();
        let changes = p.resume(secs(t, 5));
        assert_eq!(changes.len(), 1);
        assert_eq!(p.view(&quiet), View::Connecting);
        assert_eq!(p.view(&gone), View::Offline);
        assert!(!p.may_dial(&gone, secs(t, 5)));
    }
}
