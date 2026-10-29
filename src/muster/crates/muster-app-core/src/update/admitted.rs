//! Acknowledging a send against the conversation the reader is actually
//! looking at. ADR 0046 settles a send's *outcome* from the host's receipt
//! ledger; this module answers the separate question the reader asks: has the
//! text I sent appeared in this conversation? Nothing here dispatches, retries
//! or releases a send, and an unacknowledged record never becomes evidence
//! that one was delivered or lost.
use super::*;
use crate::state::{AdmittedSend, Chat};

/// At most this many unaccounted sends are kept per conversation. One send is
/// in flight at a time and ADR 0046 refuses a new one behind an unconfirmed
/// one, so reaching this bound means the conversation has stopped accounting
/// for sends entirely.
const MAX_ADMITTED: usize = 8;

/// The op_id inside a send's `<op>:<draft>` origin tag.
pub(crate) fn op_of(tag: &str) -> &str {
    tag.split_once(':').map_or(tag, |(op, _)| op)
}

pub(crate) fn key(device: &str, session: &str) -> String {
    format!("{device}\u{1}{session}")
}

/// How often each of `texts` appears as a user message, and as a row of the
/// provider's queue. One pass: this runs on every streamed event while a send
/// is still unaccounted for, over a transcript of any length.
fn counts(chat: &Chat, texts: &[String]) -> Vec<(usize, usize)> {
    let mut found = vec![(0usize, 0usize); texts.len()];
    for message in &chat.messages {
        if message.role != "user" || message.kind != "text" {
            continue;
        }
        let recorded = muster_agent::queue::normalize(&message.text);
        for (index, text) in texts.iter().enumerate() {
            if text == &recorded
                || (chat.session.provider == "codex"
                    && muster_agent::receipt::text_matches(
                        muster_agent::model::Provider::Codex,
                        text,
                        &recorded,
                    ))
            {
                found[index].0 += 1;
            }
        }
    }
    for interaction in &chat.interactions {
        if interaction.kind != muster_agent::queue::KIND {
            continue;
        }
        for (index, text) in texts.iter().enumerate() {
            if muster_agent::queue::display_matches(
                muster_agent::model::Provider::parse(&chat.session.provider),
                text,
                &interaction.prompt,
            ) {
                found[index].1 += 1;
            }
        }
    }
    found
}

/// Record a send this client is about to dispatch, before its outcome exists.
pub(crate) fn admit(state: &mut State, op_id: &str, text: &str) {
    let Some(chat) = &state.chat else { return };
    if text.trim().is_empty() {
        return;
    }
    let normalized = muster_agent::queue::normalize(text);
    let (prior_messages, prior_queued) = counts(chat, std::slice::from_ref(&normalized))[0];
    let key = key(&chat.device, &chat.session.id);
    let list = state.admitted.entry(key).or_default();
    list.retain(|a| a.op_id != op_id);
    list.push(AdmittedSend {
        user_confirmed: false,
        op_id: op_id.to_owned(),
        text: text.to_owned(),
        prior_messages,
        prior_queued,
        sending: true,
        queued: false,
        unknown: false,
        error: String::new(),
    });
    if list.len() > MAX_ADMITTED {
        // The oldest goes. By then the reader has been told eight times over
        // that this conversation is not accounting for what they send.
        list.remove(0);
    }
    bump(state);
}

/// What the host said about one dispatched send. An outcome never removes a
/// record: only the conversation's own records do that.
pub(crate) fn outcome(state: &mut State, op_id: &str, unknown: bool, error: &str) {
    let Some(chat) = &state.chat else { return };
    let key = key(&chat.device, &chat.session.id);
    let Some(list) = state.admitted.get_mut(&key) else {
        return;
    };
    let Some(record) = list.iter_mut().find(|a| a.op_id == op_id) else {
        return;
    };
    record.sending = false;
    record.unknown = unknown;
    record.error = error.to_owned();
    bump(state);
}

pub(crate) fn user_confirmed(state: &mut State, device: &str, session: &str, op: &str) {
    if let Some(records) = state.admitted.get_mut(&key(device, session)) {
        if let Some(record) = records.iter_mut().find(|r| r.op_id == op) {
            record.user_confirmed = true;
            record.sending = false;
            record.unknown = false;
            record.error.clear();
            bump(state);
        }
    }
}

/// Drop the records this conversation now accounts for, and mark the ones its
/// provider queue holds. Called whenever its messages or interactions change.
pub(crate) fn settle(state: &mut State) {
    let Some(chat) = &state.chat else { return };
    let key = key(&chat.device, &chat.session.id);
    let Some(current) = state.admitted.get(&key) else {
        return;
    };
    if current.is_empty() {
        return;
    }
    let texts: Vec<String> = current
        .iter()
        .map(|r| muster_agent::queue::normalize(&r.text))
        .collect();
    let found = counts(chat, &texts);
    let mut changed = false;
    let mut kept: Vec<AdmittedSend> = Vec::with_capacity(current.len());
    for (record, (messages, queued)) in current.iter().zip(found) {
        if messages > record.prior_messages {
            // The conversation itself now carries the text: the reader can see
            // it in the transcript, so this record has nothing left to say.
            changed = true;
            continue;
        }
        let queued = queued > record.prior_queued;
        changed |= queued != record.queued;
        kept.push(AdmittedSend {
            queued,
            ..record.clone()
        });
    }
    if !changed {
        return;
    }
    if kept.is_empty() {
        state.admitted.remove(&key);
    } else {
        state.admitted.insert(key, kept);
    }
    bump(state);
}

/// A new desktop chat's first send becomes a conversation of its own. Its
/// records belong to that conversation, not to the placeholder it was sent from.
pub(crate) fn rekey(state: &mut State, device: &str, from: &str, to: &str) {
    if from == to {
        return;
    }
    if let Some(records) = state.admitted.remove(&key(device, from)) {
        state.admitted.insert(key(device, to), records);
    }
}

/// Forget this conversation's records because the user released them after
/// checking the provider themselves. Never called by an automatic outcome.
pub(crate) fn release(state: &mut State) {
    let Some(chat) = &state.chat else { return };
    let key = key(&chat.device, &chat.session.id);
    if state.admitted.remove(&key).is_some() {
        bump(state);
    }
}

pub(crate) fn open(state: &State) -> &[AdmittedSend] {
    state
        .chat
        .as_ref()
        .and_then(|chat| state.admitted.get(&key(&chat.device, &chat.session.id)))
        .map_or(&[], |list| list.as_slice())
}

fn bump(state: &mut State) {
    if let Some(chat) = &mut state.chat {
        chat.content_revision = crate::state::next_content_revision();
    }
}
