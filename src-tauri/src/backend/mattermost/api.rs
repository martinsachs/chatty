//! Mattermost-REST-Typen (nur benötigte Felder) und Abbildung auf das Modell.

use std::collections::HashMap;

use serde::Deserialize;

use super::emoji;
use crate::model::{Attachment, Message, Reaction, Space, SpaceKind, User};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiUser {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiTeam {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiChannel {
    pub id: String,
    pub team_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub display_name: String,
    pub name: String,
    pub delete_at: i64,
    pub last_post_at: i64,
    pub total_msg_count: i64,
    pub total_msg_count_root: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiMember {
    pub channel_id: String,
    pub msg_count: i64,
    pub msg_count_root: Option<i64>,
    pub mention_count: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiReaction {
    pub user_id: String,
    pub emoji_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiMetadata {
    pub files: Vec<ApiFile>,
    pub reactions: Vec<ApiReaction>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiPost {
    pub id: String,
    pub create_at: i64,
    pub edit_at: i64,
    pub user_id: String,
    pub channel_id: String,
    pub root_id: String,
    pub message: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub reply_count: u32,
    pub metadata: ApiMetadata,
}

impl ApiPost {
    pub fn is_system(&self) -> bool {
        self.kind.starts_with("system_")
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PostList {
    /// Neueste zuerst.
    pub order: Vec<String>,
    pub posts: HashMap<String, ApiPost>,
}

impl PostList {
    /// Posts in Reihenfolge von `order` (neueste zuerst), ohne Systemnachrichten.
    pub fn ordered(&self) -> Vec<&ApiPost> {
        self.order
            .iter()
            .filter_map(|id| self.posts.get(id))
            .filter(|p| !p.is_system())
            .collect()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UploadResponse {
    pub file_infos: Vec<ApiFile>,
}

/// Angemeldeter Nutzer für die Abbildung.
#[derive(Debug, Clone)]
pub struct Me {
    pub id: String,
    pub username: String,
}

/// Erwähnt der Text den Nutzer (direkt oder per @channel/@all/@here)?
pub fn text_mentions(text: &str, username: &str) -> bool {
    let lower = text.to_lowercase();
    let own = format!("@{}", username.to_lowercase());
    [own.as_str(), "@channel", "@all", "@here"]
        .iter()
        .any(|tag| {
            lower.match_indices(tag).any(|(i, _)| {
                // Wortgrenze nach dem Namen, damit "@max" nicht "@maxi" trifft.
                lower[i + tag.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'))
            })
        })
}

fn aggregate_reactions(list: &[ApiReaction], me: &str) -> Vec<Reaction> {
    let mut out: Vec<(String, Reaction)> = Vec::new();
    for r in list {
        let emoji = emoji::to_unicode(&r.emoji_name);
        let mine = r.user_id == me;
        match out.iter_mut().find(|(e, _)| *e == emoji) {
            Some((_, agg)) => {
                agg.count += 1;
                agg.me |= mine;
            }
            None => out.push((
                emoji.clone(),
                Reaction {
                    emoji,
                    count: 1,
                    me: mine,
                },
            )),
        }
    }
    out.into_iter().map(|(_, r)| r).collect()
}

/// Post → Message. `users` liefert Anzeigenamen; fehlt einer, steht die ID da.
pub fn map_post(post: &ApiPost, me: &Me, users: &HashMap<String, ApiUser>) -> Message {
    let name = users
        .get(&post.user_id)
        .map_or_else(|| post.user_id.clone(), |u| u.username.clone());
    let root = if post.root_id.is_empty() {
        &post.id
    } else {
        &post.root_id
    };
    Message {
        id: post.id.clone(),
        space_id: post.channel_id.clone(),
        thread_id: Some(root.clone()),
        is_reply: !post.root_id.is_empty(),
        sender: User {
            id: post.user_id.clone(),
            name,
            avatar_url: None,
        },
        text: post.message.clone(),
        created: post.create_at,
        edited: (post.edit_at > 0).then_some(post.edit_at),
        reactions: aggregate_reactions(&post.metadata.reactions, &me.id),
        attachments: post
            .metadata
            .files
            .iter()
            .map(|f| Attachment {
                id: f.id.clone(),
                name: f.name.clone(),
                mime: (!f.mime_type.is_empty()).then(|| f.mime_type.clone()),
                size: Some(f.size),
                url: None,
            })
            .collect(),
        reply_count: post.reply_count,
        mentions_me: post.user_id != me.id && text_mentions(&post.message, &me.username),
        mine: post.user_id == me.id,
    }
}

/// Bei DMs heißt der Kanal "uidA__uidB": die ID des Gegenübers (bei Selbst-DM die eigene).
pub fn dm_partner<'a>(channel_name: &'a str, me: &str) -> Option<&'a str> {
    let (a, b) = channel_name.split_once("__")?;
    Some(if a == me { b } else { a })
}

/// Channel (+ Mitgliedschaft) → Space. `name` ist der fertige Anzeigename.
pub fn map_channel(ch: &ApiChannel, member: Option<&ApiMember>, name: String) -> Space {
    let kind = match ch.kind.as_str() {
        "D" => SpaceKind::Direct,
        "G" => SpaceKind::Group,
        _ => SpaceKind::Channel,
    };
    let unread = member.is_some_and(|m| match (ch.total_msg_count_root, m.msg_count_root) {
        (Some(total), Some(read)) => total > read,
        _ => ch.total_msg_count > m.msg_count,
    });
    Space {
        id: ch.id.clone(),
        name,
        kind,
        unread,
        mention_count: member.map_or(0, |m| m.mention_count),
        last_active: ch.last_post_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Me {
        Me {
            id: "me1".into(),
            username: "martin".into(),
        }
    }

    fn users() -> HashMap<String, ApiUser> {
        let mut m = HashMap::new();
        m.insert(
            "u2".into(),
            ApiUser {
                id: "u2".into(),
                username: "anna".into(),
            },
        );
        m
    }

    const POST: &str = r#"{
        "id": "p1", "create_at": 1700000000000, "update_at": 1700000001000,
        "edit_at": 1700000001000, "delete_at": 0, "user_id": "u2", "channel_id": "c1",
        "root_id": "", "message": "Hallo @martin, siehe Datei", "type": "",
        "reply_count": 3, "props": {},
        "metadata": {
            "files": [{"id": "f1", "name": "a.pdf", "mime_type": "application/pdf", "size": 1234}],
            "reactions": [
                {"user_id": "me1", "post_id": "p1", "emoji_name": "+1", "create_at": 1},
                {"user_id": "u2", "post_id": "p1", "emoji_name": "thumbsup", "create_at": 2},
                {"user_id": "u2", "post_id": "p1", "emoji_name": "partyparrot", "create_at": 3}
            ]
        }
    }"#;

    #[test]
    fn post_to_message() -> serde_json::Result<()> {
        let post: ApiPost = serde_json::from_str(POST)?;
        let m = map_post(&post, &me(), &users());
        assert_eq!(m.id, "p1");
        assert_eq!(m.space_id, "c1");
        assert_eq!(m.thread_id.as_deref(), Some("p1"));
        assert!(!m.is_reply);
        assert_eq!(m.sender.name, "anna");
        assert_eq!(m.edited, Some(1_700_000_001_000));
        assert_eq!(m.reply_count, 3);
        assert!(m.mentions_me);
        assert!(!m.mine);
        assert_eq!(
            m.reactions,
            vec![
                Reaction {
                    emoji: "👍".into(),
                    count: 2,
                    me: true
                },
                Reaction {
                    emoji: ":partyparrot:".into(),
                    count: 1,
                    me: false
                },
            ]
        );
        assert_eq!(m.attachments.len(), 1);
        assert_eq!(m.attachments[0].mime.as_deref(), Some("application/pdf"));
        assert_eq!(m.attachments[0].size, Some(1234));
        Ok(())
    }

    #[test]
    fn reply_and_minimal_post() -> serde_json::Result<()> {
        let post: ApiPost = serde_json::from_str(
            r#"{"id":"p2","create_at":5,"user_id":"me1","channel_id":"c1","root_id":"p1","message":"ok"}"#,
        )?;
        let m = map_post(&post, &me(), &users());
        assert!(m.is_reply);
        assert!(m.mine);
        assert_eq!(m.thread_id.as_deref(), Some("p1"));
        assert_eq!(m.edited, None);
        assert_eq!(m.sender.name, "me1");
        assert!(m.reactions.is_empty() && m.attachments.is_empty());
        Ok(())
    }

    #[test]
    fn post_list_order_skips_system() -> serde_json::Result<()> {
        let list: PostList = serde_json::from_str(
            r#"{"order":["b","a","s"],"posts":{
                "a":{"id":"a","create_at":1},
                "b":{"id":"b","create_at":2},
                "s":{"id":"s","create_at":0,"type":"system_join_channel"}}}"#,
        )?;
        let ids: Vec<&str> = list.ordered().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        Ok(())
    }

    #[test]
    fn mentions() {
        assert!(text_mentions("hi @Martin!", "martin"));
        assert!(text_mentions("@here bitte", "martin"));
        assert!(!text_mentions("hi @martina", "martin"));
        assert!(!text_mentions("mail an martin@x.de", "martin"));
        assert!(!text_mentions("@channelx", "martin"));
    }

    #[test]
    fn channel_to_space() -> serde_json::Result<()> {
        let ch: ApiChannel = serde_json::from_str(
            r#"{"id":"c1","type":"O","display_name":"Town Square","name":"town-square",
                "delete_at":0,"last_post_at":42,"total_msg_count":10,"total_msg_count_root":7}"#,
        )?;
        let read: ApiMember = serde_json::from_str(
            r#"{"channel_id":"c1","msg_count":8,"msg_count_root":7,"mention_count":0}"#,
        )?;
        let s = map_channel(&ch, Some(&read), "T / Town Square".into());
        assert_eq!(s.kind, SpaceKind::Channel);
        assert!(!s.unread, "Antworten in Threads zählen nicht als ungelesen");
        assert_eq!(s.last_active, 42);
        assert_eq!(s.name, "T / Town Square");

        let unread: ApiMember = serde_json::from_str(
            r#"{"channel_id":"c1","msg_count":5,"msg_count_root":5,"mention_count":2}"#,
        )?;
        let s = map_channel(&ch, Some(&unread), String::new());
        assert!(s.unread);
        assert_eq!(s.mention_count, 2);
        Ok(())
    }

    #[test]
    fn channel_without_root_counts_and_kinds() -> serde_json::Result<()> {
        let ch: ApiChannel =
            serde_json::from_str(r#"{"id":"d","type":"D","name":"me1__u2","total_msg_count":3}"#)?;
        let m: ApiMember = serde_json::from_str(r#"{"channel_id":"d","msg_count":2}"#)?;
        let s = map_channel(&ch, Some(&m), "anna".into());
        assert_eq!(s.kind, SpaceKind::Direct);
        assert!(s.unread);
        assert_eq!(dm_partner(&ch.name, "me1"), Some("u2"));
        assert_eq!(dm_partner("me1__me1", "me1"), Some("me1"));
        assert_eq!(dm_partner("u2__me1", "me1"), Some("u2"));
        let g: ApiChannel = serde_json::from_str(r#"{"id":"g","type":"G"}"#)?;
        assert_eq!(map_channel(&g, None, String::new()).kind, SpaceKind::Group);
        Ok(())
    }
}
