//! SQLite message storage for the gateway.
//!
//! Stores all chat messages flowing through the gateway so the frontend
//! can load history. Uses WAL mode for concurrent read/write.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use super::protocol::AgentEventPayload;

/// A stored chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub instance_id: String,
    pub session_key: String,
    pub role: String, // user | assistant | thinking | tool | tool_result | system | error
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u32>,
    pub created_at: String,
}

/// History query response.
#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub messages: Vec<StoredMessage>,
    pub has_more: bool,
}

/// SQLite-backed message store.
pub struct MessageStore {
    conn: Mutex<Connection>,
}

impl MessageStore {
    /// Lock the connection, mapping poisoned locks to a typed error.
    ///
    /// A poisoned mutex means a prior call panicked while holding the lock.
    /// We surface that as an error rather than propagating the panic.
    fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow!("message store mutex poisoned"))
    }

    /// Open or create the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;

        // WAL mode for concurrent reads
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        // Create table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                instance_id TEXT NOT NULL,
                session_key TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_name TEXT,
                tokens_in INTEGER,
                tokens_out INTEGER,
                created_at TIMESTAMP DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(instance_id, session_key, created_at);",
        )?;

        tracing::info!(path = %path.display(), "gateway: message store opened");

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Store a user message (sent from web frontend). Returns the new row id.
    pub fn insert_user_message(
        &self,
        instance_id: &str,
        session_key: &str,
        content: &str,
        from: &str,
    ) -> Result<i64> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO messages (instance_id, session_key, role, content, tool_name)
             VALUES (?1, ?2, 'user', ?3, ?4)",
            params![instance_id, session_key, content, from],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Store an agent event as a message.
    pub fn insert_event(
        &self,
        instance_id: &str,
        session_key: &str,
        event: &AgentEventPayload,
    ) -> Result<()> {
        let conn = self.lock_conn()?;

        match event {
            AgentEventPayload::Text { content } => {
                // Append to the last assistant message if it exists and was recent
                let last_id: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM messages
                         WHERE instance_id=?1 AND session_key=?2 AND role='assistant'
                         ORDER BY id DESC LIMIT 1",
                        params![instance_id, session_key],
                        |row| row.get(0),
                    )
                    .ok();

                if let Some(id) = last_id {
                    conn.execute(
                        "UPDATE messages SET content = content || ?1 WHERE id = ?2",
                        params![content, id],
                    )?;
                } else {
                    conn.execute(
                        "INSERT INTO messages (instance_id, session_key, role, content)
                         VALUES (?1, ?2, 'assistant', ?3)",
                        params![instance_id, session_key, content],
                    )?;
                }
            }
            AgentEventPayload::Thinking { content } => {
                conn.execute(
                    "INSERT INTO messages (instance_id, session_key, role, content)
                     VALUES (?1, ?2, 'thinking', ?3)",
                    params![instance_id, session_key, content],
                )?;
            }
            AgentEventPayload::ToolUse { tool, input, .. } => {
                conn.execute(
                    "INSERT INTO messages (instance_id, session_key, role, content, tool_name)
                     VALUES (?1, ?2, 'tool', ?3, ?4)",
                    params![instance_id, session_key, input, tool],
                )?;
            }
            AgentEventPayload::ToolResult { output, .. } => {
                conn.execute(
                    "INSERT INTO messages (instance_id, session_key, role, content)
                     VALUES (?1, ?2, 'tool_result', ?3)",
                    params![instance_id, session_key, output],
                )?;
            }
            AgentEventPayload::Result {
                content,
                input_tokens,
                output_tokens,
            } => {
                // Result finalizes the turn — update last assistant or insert new
                let last_id: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM messages
                         WHERE instance_id=?1 AND session_key=?2 AND role='assistant'
                         ORDER BY id DESC LIMIT 1",
                        params![instance_id, session_key],
                        |row| row.get(0),
                    )
                    .ok();

                if let Some(id) = last_id {
                    if !content.is_empty() {
                        conn.execute(
                            "UPDATE messages SET content=?1, tokens_in=?2, tokens_out=?3 WHERE id=?4",
                            params![content, input_tokens, output_tokens, id],
                        )?;
                    } else {
                        conn.execute(
                            "UPDATE messages SET tokens_in=?1, tokens_out=?2 WHERE id=?3",
                            params![input_tokens, output_tokens, id],
                        )?;
                    }
                } else if !content.is_empty() {
                    conn.execute(
                        "INSERT INTO messages (instance_id, session_key, role, content, tokens_in, tokens_out)
                         VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
                        params![instance_id, session_key, content, input_tokens, output_tokens],
                    )?;
                }
            }
            AgentEventPayload::Error { message } => {
                conn.execute(
                    "INSERT INTO messages (instance_id, session_key, role, content)
                     VALUES (?1, ?2, 'error', ?3)",
                    params![instance_id, session_key, message],
                )?;
            }
            AgentEventPayload::PermissionRequest { tool, .. } => {
                conn.execute(
                    "INSERT INTO messages (instance_id, session_key, role, content, tool_name)
                     VALUES (?1, ?2, 'system', ?3, ?4)",
                    params![
                        instance_id,
                        session_key,
                        format!("Permission request: {}", tool),
                        tool
                    ],
                )?;
            }
        }

        Ok(())
    }

    /// Query message history for a session.
    pub fn history(
        &self,
        instance_id: &str,
        session_key: &str,
        limit: usize,
        before_id: Option<i64>,
    ) -> Result<HistoryResponse> {
        let conn = self.lock_conn()?;

        let (query, query_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(bid) = before_id {
            (
                "SELECT id, instance_id, session_key, role, content, tool_name, tokens_in, tokens_out, created_at
                 FROM messages
                 WHERE instance_id=?1 AND session_key=?2 AND id < ?3
                 ORDER BY id DESC LIMIT ?4".to_string(),
                vec![
                    Box::new(instance_id.to_string()),
                    Box::new(session_key.to_string()),
                    Box::new(bid),
                    Box::new((limit + 1) as i64),
                ],
            )
        } else {
            (
                "SELECT id, instance_id, session_key, role, content, tool_name, tokens_in, tokens_out, created_at
                 FROM messages
                 WHERE instance_id=?1 AND session_key=?2
                 ORDER BY id DESC LIMIT ?3".to_string(),
                vec![
                    Box::new(instance_id.to_string()),
                    Box::new(session_key.to_string()),
                    Box::new((limit + 1) as i64),
                ],
            )
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(params_refs.as_slice())?;

        let mut messages = Vec::new();
        while let Some(row) = rows.next()? {
            messages.push(StoredMessage {
                id: row.get(0)?,
                instance_id: row.get(1)?,
                session_key: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                tool_name: row.get(5)?,
                tokens_in: row.get::<_, Option<i64>>(6)?.map(|v| v as u32),
                tokens_out: row.get::<_, Option<i64>>(7)?.map(|v| v as u32),
                created_at: row.get(8)?,
            });
        }

        let has_more = messages.len() > limit;
        if has_more {
            messages.pop();
        }

        // Reverse to chronological order
        messages.reverse();

        Ok(HistoryResponse { messages, has_more })
    }
}
