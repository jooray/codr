//! Read-only OpenCode v1 projections, advanced by its atomic per-session event log.
use super::{Export, Info, Message, Part, SessionSnapshot};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, limits::Limit};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_EVENTS: usize = 10_000;

struct CachedMessage {
    created: i64,
    bytes: usize,
    message: Message,
}

#[derive(Default)]
struct Cache {
    cursor: Option<(i64, String)>,
    messages: BTreeMap<String, CachedMessage>,
    bytes: usize,
    snapshot: SessionSnapshot,
}

#[derive(Default)]
pub struct Reader {
    sessions: BTreeMap<(PathBuf, String), Cache>,
}

pub struct Read {
    pub snapshot: SessionSnapshot,
    pub rebuilt: bool,
    pub events_read: usize,
    pub messages_read: usize,
    pub parts_read: usize,
}

impl Reader {
    pub(crate) fn retain(&mut self, sessions: &HashSet<String>) {
        self.sessions
            .retain(|(_, session), _| sessions.contains(session));
    }

    pub fn read(&mut self, path: &Path, session: &str) -> Result<Read> {
        let mut db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_millis(100))?;
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024)?;
        db.execute_batch("PRAGMA query_only=ON")?;
        let tx = db.transaction()?;
        ensure!(
            tx.query_row("SELECT 1 FROM session WHERE id=?1", [session], |_| Ok(()))
                .optional()?
                .is_some(),
            "OpenCode session unavailable in configured database"
        );
        let latest: Option<(i64, String)> = tx
            .query_row(
                "SELECT seq,id FROM event WHERE aggregate_id=?1 ORDER BY seq DESC LIMIT 1",
                [session],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        // This reader requires the durable v1 event log. Do not infer change cursors
        // from message IDs or wall-clock timestamps, which miss in-place updates.
        ensure!(
            latest.is_some(),
            "OpenCode session has no durable event log"
        );
        let key = (path.to_owned(), session.to_owned());
        let cached = self.sessions.get(&key);
        let cursor = cached.and_then(|cache| cache.cursor.as_ref());
        let valid = if let Some((seq, id)) = cursor {
            tx.query_row(
                "SELECT id FROM event WHERE aggregate_id=?1 AND seq=?2",
                (session, seq),
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .as_ref()
                == Some(id)
        } else {
            false
        };
        let mut rebuilt = !valid;
        let mut events_read = 0;
        let mut changed = BTreeSet::new();
        if valid && cursor != latest.as_ref() {
            let after = cursor.unwrap().0;
            let mut statement = tx.prepare(
                "SELECT seq,type,CASE type
                 WHEN 'message.updated.1' THEN json_extract(data,'$.info.id')
                 WHEN 'message.part.updated.1' THEN json_extract(data,'$.part.messageID')
                 WHEN 'message.removed.1' THEN json_extract(data,'$.messageID')
                 WHEN 'message.part.removed.1' THEN json_extract(data,'$.messageID') END
                 FROM event WHERE aggregate_id=?1 AND seq>?2 ORDER BY seq LIMIT ?3",
            )?;
            let mut rows = statement.query((session, after, (MAX_EVENTS + 1) as i64))?;
            let mut expected = after + 1;
            while let Some(row) = rows.next()? {
                events_read += 1;
                if events_read > MAX_EVENTS || row.get::<_, i64>(0)? != expected {
                    rebuilt = true;
                    break;
                }
                expected += 1;
                let kind: String = row.get(1)?;
                if kind.starts_with("message.") {
                    ensure!(
                        matches!(
                            kind.as_str(),
                            "message.updated.1"
                                | "message.part.updated.1"
                                | "message.removed.1"
                                | "message.part.removed.1"
                        ),
                        "Unsupported OpenCode message event version"
                    );
                    let id: String = row.get(2)?;
                    ensure!(!id.is_empty(), "OpenCode change has no message identity");
                    changed.insert(id);
                }
            }
            if !rebuilt {
                ensure!(
                    expected - 1 == latest.as_ref().unwrap().0,
                    "OpenCode event cursor did not reach its snapshot"
                );
            }
        }
        if rebuilt {
            let unsupported = tx.query_row(
                "SELECT 1 FROM event WHERE aggregate_id=?1 AND type GLOB 'message.*'
                 AND type NOT IN ('message.updated.1','message.part.updated.1','message.removed.1','message.part.removed.1') LIMIT 1",
                [session], |_| Ok(())
            ).optional()?.is_some();
            ensure!(!unsupported, "Unsupported OpenCode message event version");
            let mut statement =
                tx.prepare("SELECT id FROM message WHERE session_id=?1 ORDER BY time_created,id")?;
            changed = statement
                .query_map([session], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?;
        }
        let mut updates = Vec::new();
        let mut parts_read = 0;
        let mut bytes = if rebuilt {
            0
        } else {
            cached.map_or(0, |cache| cache.bytes)
        };
        for id in &changed {
            let update = read_message(&tx, session, id, &mut parts_read)?;
            if !rebuilt {
                bytes -= cached
                    .and_then(|cache| cache.messages.get(id))
                    .map_or(0, |message| message.bytes);
            }
            bytes += update.as_ref().map_or(0, |message| message.bytes);
            ensure!(
                bytes <= MAX_BYTES,
                "OpenCode session exceeds 128 MiB of selected content"
            );
            updates.push((id.clone(), update));
        }
        tx.commit()?;
        // A failed/locked/malformed read never advances the in-memory checkpoint.
        let cache = self.sessions.entry(key).or_default();
        if rebuilt {
            *cache = Cache::default();
        }
        cache.bytes = bytes;
        for (id, update) in updates {
            if let Some(message) = update {
                cache.messages.insert(id, message);
            } else {
                cache.messages.remove(&id);
            }
        }
        if rebuilt || !changed.is_empty() {
            let mut messages: Vec<_> = cache.messages.values().collect();
            messages.sort_by(|a, b| {
                (a.created, &a.message.info.id).cmp(&(b.created, &b.message.info.id))
            });
            let export = Export {
                messages: messages
                    .into_iter()
                    .map(|entry| entry.message.clone())
                    .collect(),
            };
            cache.snapshot = SessionSnapshot {
                history: super::history_from_data(&export),
                tasks: super::tasks::from_data(&export),
                decision: super::decision::from_data(&export).ok().flatten(),
            };
        }
        cache.cursor = latest;
        Ok(Read {
            snapshot: cache.snapshot.clone(),
            rebuilt,
            events_read,
            messages_read: changed.len(),
            parts_read,
        })
    }
}

fn read_message(
    tx: &Transaction<'_>,
    session: &str,
    id: &str,
    parts_read: &mut usize,
) -> Result<Option<CachedMessage>> {
    let row: Option<(i64, String)> = tx
        .query_row(
            "SELECT time_created,data FROM message WHERE session_id=?1 AND id=?2",
            (session, id),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((created, data)) = row else {
        return Ok(None);
    };
    let mut bytes = data.len();
    let mut info: Info = serde_json::from_str(&data)?;
    info.id = id.into();
    let mut parts = Vec::new();
    if info.role == "assistant" {
        let mut statement = tx.prepare(
            "SELECT id,data FROM part WHERE session_id=?1 AND message_id=?2 AND
             (json_extract(data,'$.type')='text' OR
              (json_extract(data,'$.type')='tool' AND json_extract(data,'$.tool') IN ('todowrite','question')))
             ORDER BY id")?;
        let mut rows = statement.query((session, id))?;
        while let Some(row) = rows.next()? {
            let data: String = row.get(1)?;
            bytes += data.len();
            ensure!(
                bytes <= MAX_BYTES,
                "OpenCode message exceeds selected-content limit"
            );
            let mut part: Part = serde_json::from_str(&data)?;
            part.id = Some(row.get(0)?);
            parts.push(part);
            *parts_read += 1;
        }
    }
    Ok(Some(CachedMessage {
        created,
        bytes,
        message: Message { info, parts },
    }))
}
