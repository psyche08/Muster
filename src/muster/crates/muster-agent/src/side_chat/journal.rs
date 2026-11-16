//! One daemon owns this journal. State and transcript changes commit together;
//! the cached catalog contains summaries, never message bodies.
use super::{Exchange, Question};
use crate::{AgentSession, Error};
use muster_proto::v1;
use rusqlite::{Connection, params};
use std::{
    collections::HashMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn io(e: rusqlite::Error) -> Error {
    Error::Io(format!("Side chat journal: {e}"))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) struct Store {
    db: Connection,
    pub(crate) rows: HashMap<String, AgentSession>,
}

impl Store {
    pub(crate) fn open(path: Option<&Path>) -> Result<Self, Error> {
        let mut db = match path {
            Some(p) => Connection::open(p),
            None => Connection::open_in_memory(),
        }
        .map_err(io)?;
        db.execute_batch("PRAGMA locking_mode=EXCLUSIVE; PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS chats(id TEXT PRIMARY KEY,parent TEXT NOT NULL,provider TEXT NOT NULL,title TEXT NOT NULL,state TEXT NOT NULL,updated INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS turns(id INTEGER PRIMARY KEY,chat TEXT NOT NULL,question TEXT NOT NULL,answer TEXT,failed INTEGER NOT NULL DEFAULT 0,created INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS chat_turns ON turns(chat,id);").map_err(io)?;
        let tx = db.transaction().map_err(io)?;
        tx.execute("UPDATE turns SET answer='Host restarted before the answer arrived',failed=1 WHERE answer IS NULL",[]).map_err(io)?;
        tx.execute("UPDATE chats SET state='idle' WHERE state='running'", [])
            .map_err(io)?;
        tx.commit().map_err(io)?;
        let rows = db
            .prepare("SELECT id,parent,provider,title,state,updated FROM chats")
            .map_err(io)?
            .query_map([], |r| {
                let state: String = r.get(4)?;
                Ok(AgentSession {
                    id: r.get(0)?,
                    parent_session_id: r.get(1)?,
                    provider: r.get(2)?,
                    title: r.get(3)?,
                    read_only: state == "ended",
                    state,
                    updated_unix: r.get::<_, i64>(5)?.max(0) as u64,
                    mode: "sidechat".into(),
                    relationship: "sidechat".into(),
                    ..Default::default()
                })
            })
            .map_err(io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io)?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        Ok(Self { db, rows })
    }
    pub(super) fn admit(
        &mut self,
        parent: &AgentSession,
        child: Option<&str>,
        question: &str,
    ) -> Result<(String, Option<(i64, Question)>), Error> {
        if !super::available(parent) {
            return Err(Error::Conflict(
                "Source session is unavailable or ended".into(),
            ));
        }
        if question.len() > super::MAX_QUESTION || (child.is_some() && question.trim().is_empty()) {
            return Err(Error::Invalid(
                "Enter a side question of up to 8 KiB".into(),
            ));
        }
        let created = child.is_none();
        let mut s = if let Some(child) = child {
            self.rows
                .get(child)
                .cloned()
                .filter(|s| s.parent_session_id == parent.id && s.provider == parent.provider)
                .ok_or_else(|| Error::NotFound("Side chat".into()))?
        } else {
            AgentSession {
                id: format!("btw:{}:{:032x}", parent.id, rand::random::<u128>()),
                mode: "sidechat".into(),
                provider: parent.provider.clone(),
                parent_session_id: parent.id.clone(),
                relationship: "sidechat".into(),
                title: if question.is_empty() {
                    "Side chat".into()
                } else {
                    question
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(80)
                        .collect()
                },
                state: "idle".into(),
                ..Default::default()
            }
        };
        if s.state != "idle" {
            return Err(Error::Conflict(
                if s.read_only {
                    "This side chat has ended"
                } else {
                    "A side question is already running"
                }
                .into(),
            ));
        }
        let tx = self.db.transaction().map_err(io)?;
        s.updated_unix = now();
        if created {
            tx.execute(
                "INSERT INTO chats VALUES(?,?,?,?,?,?)",
                params![
                    s.id,
                    s.parent_session_id,
                    s.provider,
                    s.title,
                    s.state,
                    s.updated_unix as i64
                ],
            )
            .map_err(io)?;
        }
        let work = if question.is_empty() {
            None
        } else {
            // Bound a child transcript without deleting any old messages.
            let count: u32 = tx
                .query_row("SELECT count(*) FROM turns WHERE chat=?", [&s.id], |r| {
                    r.get(0)
                })
                .map_err(io)?;
            if count >= 256 {
                return Err(Error::Conflict(
                    "This side chat is full; start another from its parent".into(),
                ));
            }
            if count == 0 && !created {
                s.title = question
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect();
            }
            let mut previous=tx.prepare("SELECT question,answer FROM turns WHERE chat=? AND answer IS NOT NULL AND failed=0 ORDER BY id DESC LIMIT ?").map_err(io)?
                .query_map(params![s.id,super::MAX_EXCHANGES as i64],|r|Ok(Exchange{question:r.get(0)?,answer:r.get(1)?})).map_err(io)?
                .collect::<Result<Vec<_>,_>>().map_err(io)?;
            previous.reverse();
            tx.execute(
                "INSERT INTO turns(chat,question,created) VALUES(?,?,?)",
                params![s.id, question, s.updated_unix as i64],
            )
            .map_err(io)?;
            let turn = tx.last_insert_rowid();
            s.state = "running".into();
            tx.execute(
                "UPDATE chats SET state=?,updated=?,title=? WHERE id=?",
                params![s.state, s.updated_unix as i64, s.title, s.id],
            )
            .map_err(io)?;
            Some((
                turn,
                Question {
                    session: parent.id.clone(),
                    question: question.into(),
                    previous,
                },
            ))
        };
        tx.commit().map_err(io)?;
        let id = s.id.clone();
        self.rows.insert(id.clone(), s);
        Ok((id, work))
    }
    pub(super) fn finish(
        &mut self,
        id: &str,
        turn: i64,
        result: Result<String, Error>,
    ) -> Result<bool, Error> {
        let s = self
            .rows
            .get_mut(id)
            .ok_or_else(|| Error::NotFound("Side chat".into()))?;
        if s.state != "running" {
            return Ok(false);
        }
        let (text, failed) = match result {
            Ok(answer) => (answer, false),
            Err(e) => (e.to_string(), true),
        };
        let tx = self.db.transaction().map_err(io)?;
        let n = tx
            .execute(
                "UPDATE turns SET answer=?,failed=? WHERE id=? AND chat=? AND answer IS NULL",
                params![text, failed, turn, id],
            )
            .map_err(io)?;
        if n != 1 {
            return Ok(false);
        }
        let ts = now();
        tx.execute(
            "UPDATE chats SET state='idle',updated=? WHERE id=?",
            params![ts as i64, id],
        )
        .map_err(io)?;
        tx.commit().map_err(io)?;
        s.state = "idle".into();
        s.updated_unix = ts;
        Ok(true)
    }
    pub(super) fn end(&mut self, id: &str) -> Result<(), Error> {
        let s = self
            .rows
            .get_mut(id)
            .ok_or_else(|| Error::NotFound("Side chat".into()))?;
        let tx = self.db.transaction().map_err(io)?;
        tx.execute("UPDATE turns SET answer='Side chat ended before the answer arrived',failed=1 WHERE chat=? AND answer IS NULL",[id]).map_err(io)?;
        tx.execute("UPDATE chats SET state='ended' WHERE id=?", [id])
            .map_err(io)?;
        tx.commit().map_err(io)?;
        s.state = "ended".into();
        s.read_only = true;
        Ok(())
    }
    pub(crate) fn history(
        &self,
        req: &v1::AgentMessagesRequest,
    ) -> Result<v1::AgentMessagePage, Error> {
        if !self.rows.contains_key(&req.session) {
            return Err(Error::NotFound("Side chat".into()));
        }
        let mut q = self
            .db
            .prepare("SELECT id,question,answer,failed,created FROM turns WHERE chat=? ORDER BY id")
            .map_err(io)?;
        let mut rows = q.query([&req.session]).map_err(io)?;
        let mut messages = Vec::new();
        while let Some(row) = rows.next().map_err(io)? {
            let id: i64 = row.get(0).map_err(io)?;
            let ts = row.get::<_, i64>(4).map_err(io)?.max(0) as u64;
            let message = |role: &str, text: String| v1::AgentMessage {
                id: format!("{id}:{role}"),
                role: role.into(),
                text,
                ts_unix: ts,
                kind: "text".into(),
                ..Default::default()
            };
            messages.push(message("user", row.get(1).map_err(io)?));
            if let Some(answer) = row.get::<_, Option<String>>(2).map_err(io)? {
                messages.push(message(
                    if row.get::<_, bool>(3).map_err(io)? {
                        "system"
                    } else {
                        "assistant"
                    },
                    answer,
                ));
            }
        }
        Ok(if req.turns > 0 || !req.work_id.is_empty() {
            muster_proto::outline::Outline::default().page(&messages, req, Clone::clone)
        } else {
            muster_proto::history::page(&messages, req)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(provider: &str) -> AgentSession {
        AgentSession {
            id: "11111111-1111-4111-8111-111111111111".into(),
            provider: provider.into(),
            mode: "code".into(),
            state: "running".into(),
            ..Default::default()
        }
    }
    #[test]
    fn sides_group_with_each_provider_and_end_is_durable() {
        for provider in ["codex", "claude"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal.db");
            let mut store = Store::open(Some(&path)).unwrap();
            let parent = source(provider);
            let (child, first) = store.admit(&parent, None, "why?").unwrap();
            assert!(store.admit(&parent, Some(&child), "duplicate").is_err());
            assert!(
                store
                    .finish(&child, first.unwrap().0, Ok("because".into()))
                    .unwrap()
            );
            let row = store.rows[&child].clone();
            let roots = crate::catalog::family_roots(&[parent.clone(), row.clone()]);
            assert_eq!(
                roots[&crate::catalog::session_key(&row)],
                crate::catalog::session_key(&parent)
            );
            let (_, second) = store.admit(&parent, Some(&child), "more?").unwrap();
            let (turn, context) = second.unwrap();
            assert_eq!(context.previous[0].answer, "because");
            store.end(&child).unwrap();
            assert!(!store.finish(&child, turn, Ok("late reply".into())).unwrap());
            drop(store);
            let mut store = Store::open(Some(&path)).unwrap();
            assert!(super::super::ended(&store.rows[&child]));
            assert!(store.admit(&parent, Some(&child), "try again").is_err());
            let page = store
                .history(&v1::AgentMessagesRequest {
                    session: child.clone(),
                    turns: 10,
                    ..Default::default()
                })
                .unwrap();
            assert!(page.messages.iter().any(|m| m.text == "because"));
            assert!(!page.messages.iter().any(|m| m.text == "late reply"));
            let ended = AgentSession {
                state: "ended".into(),
                ..parent.clone()
            };
            assert!(store.admit(&ended, None, "new side").is_err());
        }
    }
}
