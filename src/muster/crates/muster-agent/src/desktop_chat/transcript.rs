//! The messages known for one conversation across snapshots.

use super::*;

/// The messages known for one conversation. Message ids are positional
/// (`ax<generation>-<index>`) so a reply that grows keeps its id; content
/// that no longer matches the known prefix starts a new generation.
#[derive(Clone, Debug, Default)]
pub struct ChatTranscript {
    pub generation: u64,
    pub lines: Vec<(ChatLine, u64)>,
}

/// What applying a snapshot changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatDelta {
    /// Everything before is void: a new generation was started.
    pub reset: bool,
    /// Indices whose message is new or has new text.
    pub changed: Vec<usize>,
}

impl ChatTranscript {
    pub fn message_id(&self, index: usize) -> String {
        format!("ax{}-{index:05}", self.generation)
    }

    /// Takes the page's messages. A snapshot that is a prefix of what is
    /// known (a partial walk, an unrendered tail) changes nothing; a reply
    /// may grow or be rewritten in place while it is the last message.
    pub fn apply(&mut self, lines: Vec<ChatLine>, now: u64) -> ChatDelta {
        let old = &self.lines;
        let common = old
            .iter()
            .zip(&lines)
            .take_while(|((a, _), b)| a == *b)
            .count();
        let partial = lines.len() <= old.len()
            && lines.iter().enumerate().all(|(i, line)| {
                let known = &old[i].0;
                known.role == line.role
                    && (known.text == line.text
                        || (i + 1 == lines.len() && known.text.starts_with(&line.text)))
            });
        if partial {
            return ChatDelta::default();
        }
        let in_place = common == old.len()
            || (common + 1 == old.len() && old[common].0.role == lines[common].role);
        let mut delta = ChatDelta::default();
        // A reply rewritten in place keeps the time it first appeared.
        let mut kept = None;
        if in_place {
            kept = self.lines.get(common).map(|(_, seen)| *seen);
            self.lines.truncate(common);
        } else {
            self.generation += 1;
            self.lines.clear();
            delta.reset = true;
        }
        let start = self.lines.len();
        for (offset, line) in lines.into_iter().enumerate().skip(start) {
            let seen = if offset == start {
                kept.unwrap_or(now)
            } else {
                now
            };
            self.lines.push((line, seen));
            delta.changed.push(offset);
        }
        delta
    }
}
