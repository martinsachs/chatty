//! SQLite-Cache für Spaces und Nachrichten (schneller Start, offline lesbar).
//! Objekte liegen als JSON; abgefragt wird nur nach Konto/Space/Zeit.

use std::path::Path;

use rusqlite::{Connection, params};

use crate::error::Result;
use crate::model::{Message, Space};

/// Höchstzahl gecachter Nachrichten je Space.
const KEEP_PER_SPACE: i64 = 500;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA secure_delete=ON;
             CREATE TABLE IF NOT EXISTS spaces (
                 account TEXT NOT NULL, id TEXT NOT NULL, json TEXT NOT NULL,
                 PRIMARY KEY (account, id));
             CREATE TABLE IF NOT EXISTS messages (
                 account TEXT NOT NULL, space TEXT NOT NULL, id TEXT NOT NULL,
                 created INTEGER NOT NULL, is_reply INTEGER NOT NULL, json TEXT NOT NULL,
                 PRIMARY KEY (account, id));
             CREATE INDEX IF NOT EXISTS messages_space ON messages (account, space, created);
             CREATE TABLE IF NOT EXISTS flags (
                 account TEXT NOT NULL, id TEXT NOT NULL,
                 favorite INTEGER NOT NULL DEFAULT 0, muted INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (account, id));",
        )?;
        Ok(Self { conn })
    }

    pub fn put_spaces(&mut self, account: &str, spaces: &[Space]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM spaces WHERE account = ?1", params![account])?;
        for s in spaces {
            tx.execute(
                "INSERT INTO spaces (account, id, json) VALUES (?1, ?2, ?3)",
                params![account, s.id, serde_json::to_string(s)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn put_space(&self, account: &str, space: &Space) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO spaces (account, id, json) VALUES (?1, ?2, ?3)",
            params![account, space.id, serde_json::to_string(space)?],
        )?;
        Ok(())
    }

    /// Lokaler Favorit/Stumm für Backends ohne eigene Unterstützung (Google).
    pub fn set_flag(
        &self,
        account: &str,
        id: &str,
        favorite: Option<bool>,
        muted: Option<bool>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO flags (account, id) VALUES (?1, ?2)",
            params![account, id],
        )?;
        self.conn.execute(
            "UPDATE flags SET favorite = COALESCE(?3, favorite), muted = COALESCE(?4, muted)
             WHERE account = ?1 AND id = ?2",
            params![account, id, favorite, muted],
        )?;
        Ok(())
    }

    /// Lokale Flags über die Backend-Werte legen.
    pub fn apply_flags(&self, account: &str, spaces: &mut [Space]) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, favorite, muted FROM flags WHERE account = ?1")?;
        let rows = stmt.query_map(params![account], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?;
        let mut flags = std::collections::HashMap::new();
        for row in rows {
            let (id, fav, muted) = row?;
            flags.insert(id, (fav, muted));
        }
        for s in spaces {
            if let Some(&(fav, muted)) = flags.get(&s.id) {
                s.favorite = fav;
                s.muted = muted;
            }
        }
        Ok(())
    }

    /// Einzelnen Space lesen (ohne alle zu deserialisieren).
    pub fn space(&self, account: &str, id: &str) -> Result<Option<Space>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT json FROM spaces WHERE account = ?1 AND id = ?2")?;
        let mut rows = stmt.query(params![account, id])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?)),
            None => Ok(None),
        }
    }

    pub fn spaces(&self, account: &str) -> Result<Vec<Space>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM spaces WHERE account = ?1")?;
        let rows = stmt.query_map(params![account], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            out.push(serde_json::from_str(&json?)?);
        }
        Ok(out)
    }

    pub fn put_messages(&mut self, account: &str, messages: &[Message]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for m in messages {
            tx.execute(
                "INSERT OR REPLACE INTO messages (account, space, id, created, is_reply, json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account,
                    m.space_id,
                    m.id,
                    m.created,
                    m.is_reply,
                    serde_json::to_string(m)?
                ],
            )?;
        }
        // Älteste über dem Limit verwerfen, damit der Cache klein bleibt.
        let mut spaces: Vec<&str> = messages.iter().map(|m| m.space_id.as_str()).collect();
        spaces.sort_unstable();
        spaces.dedup();
        for space in spaces {
            tx.execute(
                "DELETE FROM messages WHERE account = ?1 AND space = ?2 AND id NOT IN (
                     SELECT id FROM messages WHERE account = ?1 AND space = ?2
                     ORDER BY created DESC LIMIT ?3)",
                params![account, space, KEEP_PER_SPACE],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_message(&self, account: &str, id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM messages WHERE account = ?1 AND id = ?2",
            params![account, id],
        )?;
        Ok(())
    }

    /// Letzte `limit` Hauptverlaufs-Nachrichten, aufsteigend sortiert.
    /// Neueste `limit` Nachrichten eines Space, aufsteigend; `with_replies`: Thread-Antworten mit (Mattermost).
    pub fn messages(
        &self,
        account: &str,
        space: &str,
        limit: i64,
        with_replies: bool,
    ) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT json FROM messages WHERE account = ?1 AND space = ?2 AND (?4 OR is_reply = 0)
             ORDER BY created DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![account, space, limit, with_replies], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for json in rows {
            out.push(serde_json::from_str::<Message>(&json?)?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn delete_account(&self, account: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for table in ["messages", "spaces", "flags"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE account = ?1"),
                params![account],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SpaceKind, User};

    fn msg(id: &str, created: i64, is_reply: bool) -> Message {
        Message {
            id: id.into(),
            space_id: "s1".into(),
            thread_id: None,
            is_reply,
            sender: User {
                id: "u".into(),
                name: "U".into(),
                avatar_url: None,
            },
            text: format!("text {id}"),
            created,
            edited: None,
            reactions: vec![],
            attachments: vec![],
            reply_count: 0,
            mentions_me: false,
            mine: false,
        }
    }

    #[test]
    fn spaces_roundtrip() {
        let mut st = Store::in_memory().unwrap();
        let s = Space {
            id: "s1".into(),
            name: "Allgemein".into(),
            kind: SpaceKind::Channel,
            unread: true,
            mention_count: 2,
            last_active: 10,
            msg_count: 0,
            favorite: false,
            muted: false,
            status_emoji: None,
            presence: None,
            header: String::new(),
        };
        st.put_spaces("a", std::slice::from_ref(&s)).unwrap();
        assert_eq!(st.spaces("a").unwrap(), vec![s.clone()]);
        assert!(st.spaces("b").unwrap().is_empty());

        let mut s2 = s.clone();
        s2.unread = false;
        st.put_space("a", &s2).unwrap();
        assert_eq!(st.spaces("a").unwrap(), vec![s2.clone()]);
        assert_eq!(st.space("a", "s1").unwrap(), Some(s2.clone()));
        assert_eq!(st.space("a", "nope").unwrap(), None);
        assert_eq!(st.space("b", "s1").unwrap(), None);

        let mut list = vec![s2];
        st.set_flag("a", "s1", Some(true), None).unwrap();
        st.set_flag("a", "s1", None, Some(true)).unwrap();
        st.apply_flags("b", &mut list).unwrap();
        assert!(!list[0].favorite && !list[0].muted);
        st.apply_flags("a", &mut list).unwrap();
        assert!(list[0].favorite && list[0].muted);
        st.set_flag("a", "s1", Some(false), None).unwrap();
        st.apply_flags("a", &mut list).unwrap();
        assert!(!list[0].favorite && list[0].muted);
        st.delete_account("a").unwrap();
        assert!(st.spaces("a").unwrap().is_empty());
        list[0].muted = false;
        st.apply_flags("a", &mut list).unwrap();
        assert!(!list[0].muted, "Flags mit dem Konto gelöscht");
    }

    #[test]
    fn messages_sorted_without_replies_and_trimmed() {
        let mut st = Store::in_memory().unwrap();
        st.put_messages(
            "a",
            &[msg("m2", 2, false), msg("m1", 1, false), msg("r1", 3, true)],
        )
        .unwrap();
        let got: Vec<_> = st
            .messages("a", "s1", 10, false)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(got, ["m1", "m2"]);

        st.delete_message("a", "m1").unwrap();
        assert_eq!(st.messages("a", "s1", 10, false).unwrap().len(), 1);

        let many: Vec<_> = (0..KEEP_PER_SPACE + 20)
            .map(|i| msg(&format!("x{i}"), 100 + i, false))
            .collect();
        st.put_messages("a", &many).unwrap();
        let kept = st.messages("a", "s1", 10_000, false).unwrap();
        assert_eq!(kept.len() as i64, KEEP_PER_SPACE);
        assert_eq!(kept.last().unwrap().id, format!("x{}", KEEP_PER_SPACE + 19));

        st.delete_account("a").unwrap();
        assert!(st.messages("a", "s1", 10, false).unwrap().is_empty());
    }
}
