//! ChatGPT export adapter.
//!
//! Parses the JSON dump ChatGPT provides under
//! "Settings → Data Controls → Export Data" into NormalizedMessages.
//!
//! ## Format
//!
//! Top-level: a JSON array of conversation objects.
//! Each conversation: `{ id, title, create_time, current_node, mapping: { ... } }`
//! The `mapping` is a non-linear node tree (a user editing a previous message
//! creates a branch; a regenerated assistant response creates another branch).
//! The `current_node` field identifies the active branch tip; we walk the
//! parent chain backwards to the root, then forward to reconstruct the linear
//! thread in correct order.
//!
//! ## Limitations (F2 MVP)
//!
//! - The full export is materialized in memory (~2x the JSON size). For
//!   50MB exports that's ~100MB peak, fine for a developer laptop. True
//!   streaming optimization is tracked for F3+ if real exports need it.
//! - Only text content is kept. Image URLs, code interpreter artifacts, and
//!   other structured `parts` are dropped (they don't carry semantic content
//!   in a useful form for downstream RAG/Projects).
//! - `author.role` mapping: `user`/`assistant`/`system`/`tool`. Unknown roles
//!   (e.g. custom GPT `name`) are kept with role `user` if they look like user
//!   input, otherwise dropped.

use crate::adapter::{Adapter, AdapterKind};
use crate::pipeline::{NormalizedMessage, Role};
use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;

pub struct ChatGptAdapter;

impl Adapter for ChatGptAdapter {
    fn kind(&self) -> AdapterKind {
        AdapterKind::ChatGpt
    }

    fn detect(&self, header: &[u8]) -> bool {
        // Real ChatGPT exports always have `"mapping"` somewhere in the first
        // ~4KB. We also accept the (less common) object-wrapped form
        // `"conversations"` for older exports.
        match std::str::from_utf8(header) {
            Ok(s) => s.contains("\"mapping\"") || s.contains("\"conversations\""),
            Err(_) => false,
        }
    }

    fn stream_messages(
        &self,
        reader: Box<dyn Read>,
    ) -> Result<Box<dyn Iterator<Item = NormalizedMessage>>> {
        let conversations: Vec<ChatGptConversation> = serde_json::from_reader(reader)
            .context("failed to parse ChatGPT conversations.json")?;

        let messages: Vec<NormalizedMessage> = conversations
            .iter()
            .flat_map(reconstruct_messages)
            .collect();

        Ok(Box::new(messages.into_iter()))
    }
}

/// Walk the `mapping` tree from `current_node` back to the root, then yield
/// the linear thread. Empty conversations and cycles are handled defensively.
fn reconstruct_messages(conv: &ChatGptConversation) -> Vec<NormalizedMessage> {
    // Pick the start node: current_node if set, else the root (no parent).
    let start = conv.current_node.clone().or_else(|| {
        conv.mapping
            .iter()
            .find(|(_, n)| n.parent.is_none())
            .map(|(id, _)| id.clone())
    });

    let Some(start_id) = start else {
        return Vec::new();
    };

    // Walk backwards from start to root, collecting the path.
    let mut path: Vec<String> = Vec::new();
    let mut current = Some(start_id);
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(node_id) = current {
        if !visited.insert(node_id.clone()) {
            break; // cycle protection
        }
        path.push(node_id.clone());
        current = conv.mapping.get(&node_id).and_then(|n| n.parent.clone());
    }
    path.reverse();

    // Yield messages in root-first order.
    let mut out = Vec::new();
    for node_id in path {
        if let Some(node) = conv.mapping.get(&node_id) {
            if let Some(msg) = &node.message {
                if let Some(norm) = build_normalized(msg, conv) {
                    out.push(norm);
                }
            }
        }
    }
    out
}

fn build_normalized(msg: &ChatGptMessage, conv: &ChatGptConversation) -> Option<NormalizedMessage> {
    let role = map_role(&msg.author.role)?;

    // Extract text parts only. Skip structured parts (image_url, code, etc.).
    let content: String = msg
        .content
        .parts
        .iter()
        .filter_map(|p| p.as_text())
        .collect::<Vec<_>>()
        .join("\n");

    if content.trim().is_empty() {
        return None;
    }

    // Use the message's own create_time if set, else fall back to the
    // conversation's create_time. Older exports sometimes omit per-message times.
    let ts = msg.create_time.unwrap_or(conv.create_time);
    let created_at = Utc.timestamp_opt(ts as i64, 0).single()?;

    let content_hash = NormalizedMessage::compute_content_hash(&content);
    let project_hint = Some(NormalizedMessage::slugify(&conv.title));

    Some(NormalizedMessage {
        id: format!("{}:{}", conv.id, msg.id),
        source: "chatgpt".to_string(),
        role,
        content,
        original_length: 0, // set by the scrubber downstream; we don't know pre-redaction here
        created_at,
        thread_id: conv.id.clone(),
        thread_title: Some(conv.title.clone()),
        project_hint,
        content_hash,
        hits: 1,           // F3: dedup counter starts at 1 (unique)
        signal_score: 0.0, // F3: computed downstream by `signals::score`
    })
}

fn map_role(raw: &str) -> Option<Role> {
    match raw {
        "user" => Some(Role::User),
        "assistant" => Some(Role::Assistant),
        "system" => Some(Role::System),
        "tool" => Some(Role::Tool),
        _ => None,
    }
}

// --- Raw ChatGPT JSON shape (private) ---

#[derive(Debug, Deserialize)]
struct ChatGptConversation {
    id: String,
    title: String,
    create_time: f64,
    #[serde(default)]
    current_node: Option<String>,
    #[serde(default)]
    mapping: HashMap<String, ChatGptNode>,
}

#[derive(Debug, Deserialize)]
struct ChatGptNode {
    #[serde(default)]
    message: Option<ChatGptMessage>,
    #[serde(default)]
    parent: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatGptMessage {
    id: String,
    author: ChatGptAuthor,
    content: ChatGptContent,
    #[serde(default)]
    create_time: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ChatGptAuthor {
    role: String,
}

#[derive(Debug, Deserialize)]
struct ChatGptContent {
    #[serde(default)]
    parts: Vec<ChatGptPart>,
}

/// ChatGPT message parts are `untagged` enums in practice: text strings are
/// emitted as bare strings, while structured payloads (image_url, code, tool
/// calls) are JSON objects. We only care about the text ones.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ChatGptPart {
    Text(String),
    // The value is parsed (so we know it WAS structured) but never read —
    // structured parts are dropped by `as_text()`. Keep the value field to
    // make the parser future-proof: a future caller could log/inspect it.
    #[allow(dead_code)]
    Other(serde_json::Value),
}

impl ChatGptPart {
    fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_str()),
            Self::Other(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static str {
        include_str!("../../tests/fixtures/chatgpt-tiny.json")
    }

    #[test]
    fn detect_positive_on_chatgpt_export() {
        let adapter = ChatGptAdapter;
        let header = &fixture().as_bytes()[..4096.min(fixture().len())];
        assert!(
            adapter.detect(header),
            "detect should match the synthetic fixture"
        );
    }

    #[test]
    fn detect_negative_on_unrelated_json() {
        let adapter = ChatGptAdapter;
        let not_chatgpt = br#"{"some":"other","format":true,"items":[]}"#;
        assert!(!adapter.detect(not_chatgpt));
    }

    #[test]
    fn stream_messages_reconstructs_linear_thread() {
        let adapter = ChatGptAdapter;
        let reader = Box::new(fixture().as_bytes());
        let messages: Vec<NormalizedMessage> = adapter.stream_messages(reader).unwrap().collect();

        // 3 convs: conv-001 has 3 msgs, conv-002 is empty, conv-003 has 4
        assert_eq!(messages.len(), 7, "expected 3 + 0 + 4 = 7 messages");

        // First conversation: user, assistant, user, in that order
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[0].content, "How do I implement FNV-1a in Rust?");
        assert_eq!(messages[1].role, Role::Assistant);
        assert!(messages[1].content.contains("seahash"));
        assert_eq!(messages[2].role, Role::User);
        assert_eq!(messages[2].content, "Perfect, that works!");

        // Thread metadata
        assert_eq!(messages[0].thread_id, "conv-001");
        assert_eq!(
            messages[0].thread_title.as_deref(),
            Some("Rust FNV-1a hashing")
        );
        assert_eq!(
            messages[0].project_hint.as_deref(),
            Some("rust-fnv-1a-hashing")
        );

        // Source label
        assert_eq!(messages[0].source, "chatgpt");

        // Hash is non-zero and stable
        assert_ne!(messages[0].content_hash, 0);
        assert_eq!(
            messages[0].content_hash, messages[0].content_hash,
            "hash is deterministic"
        );
    }

    #[test]
    fn stream_messages_handles_empty_conversation() {
        let adapter = ChatGptAdapter;
        // conv-002 in the fixture has only a root node with no message
        let json = r#"[
            {
                "id": "empty-conv",
                "title": "Empty",
                "create_time": 1700001000.0,
                "current_node": "node-root",
                "mapping": {
                    "node-root": { "message": null, "parent": null }
                }
            }
        ]"#;
        let reader = Box::new(json.as_bytes());
        let messages: Vec<NormalizedMessage> = adapter.stream_messages(reader).unwrap().collect();
        assert!(messages.is_empty(), "empty conversation yields 0 messages");
    }

    #[test]
    fn stream_messages_preserves_system_role() {
        let adapter = ChatGptAdapter;
        // conv-003 has a system message first
        let json = r#"[
            {
                "id": "sys-conv",
                "title": "With system",
                "create_time": 1700002000.0,
                "current_node": "node-2",
                "mapping": {
                    "node-root": { "message": null, "parent": null },
                    "node-1": {
                        "message": {
                            "id": "msg-1",
                            "author": { "role": "system" },
                            "content": { "parts": ["You are helpful."] }
                        },
                        "parent": "node-root"
                    },
                    "node-2": {
                        "message": {
                            "id": "msg-2",
                            "author": { "role": "user" },
                            "content": { "parts": ["Hi"] }
                        },
                        "parent": "node-1"
                    }
                }
            }
        ]"#;
        let reader = Box::new(json.as_bytes());
        let messages: Vec<NormalizedMessage> = adapter.stream_messages(reader).unwrap().collect();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn stream_messages_skips_unknown_role() {
        let adapter = ChatGptAdapter;
        let json = r#"[
            {
                "id": "weird-conv",
                "title": "Unknown role",
                "create_time": 1700003000.0,
                "current_node": "node-2",
                "mapping": {
                    "node-root": { "message": null, "parent": null },
                    "node-1": {
                        "message": {
                            "id": "msg-1",
                            "author": { "role": "critic" },
                            "content": { "parts": ["as a critic, I find this lacking"] }
                        },
                        "parent": "node-root"
                    },
                    "node-2": {
                        "message": {
                            "id": "msg-2",
                            "author": { "role": "user" },
                            "content": { "parts": ["Thanks for the review"] }
                        },
                        "parent": "node-1"
                    }
                }
            }
        ]"#;
        let reader = Box::new(json.as_bytes());
        let messages: Vec<NormalizedMessage> = adapter.stream_messages(reader).unwrap().collect();
        // Only the user message survives
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);
    }

    #[test]
    fn stream_messages_skips_empty_content() {
        let adapter = ChatGptAdapter;
        let json = r#"[
            {
                "id": "empty-msg-conv",
                "title": "Empty msg",
                "create_time": 1700004000.0,
                "current_node": "node-2",
                "mapping": {
                    "node-root": { "message": null, "parent": null },
                    "node-1": {
                        "message": {
                            "id": "msg-1",
                            "author": { "role": "user" },
                            "content": { "parts": [""] }
                        },
                        "parent": "node-root"
                    },
                    "node-2": {
                        "message": {
                            "id": "msg-2",
                            "author": { "role": "assistant" },
                            "content": { "parts": ["\n\n  "] }
                        },
                        "parent": "node-1"
                    }
                }
            }
        ]"#;
        let reader = Box::new(json.as_bytes());
        let messages: Vec<NormalizedMessage> = adapter.stream_messages(reader).unwrap().collect();
        assert!(messages.is_empty(), "whitespace-only messages are dropped");
    }
}
