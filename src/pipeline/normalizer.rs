//! Defensive normalization pass. Runs after the scrubber. Its job is
//! to backfill any fields that an adapter forgot to populate, and to
//! keep the pipeline robust against new adapters that don't follow the
//! F1 contract perfectly.
//!
//! Currently:
//! - If `original_length == 0`, set it to the current `content.len()`.
//!   (Adapters are supposed to set it to the pre-scrub length; the
//!   scrubber does this. If neither ran, we still want a sensible
//!   stat for the writer.)
//! - Trim trailing whitespace from `content` (defensive; scrubber
//!   redactions can leave a stray newline at the end).

use crate::pipeline::NormalizedMessage;

pub fn normalize(msg: &mut NormalizedMessage) {
    if msg.original_length == 0 {
        msg.original_length = msg.content.len() as u32;
    }
    if msg.content.ends_with(char::is_whitespace) {
        msg.content = msg.content.trim_end().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Role;
    use chrono::TimeZone;

    fn make_msg(content: &str, original_length: u32) -> NormalizedMessage {
        NormalizedMessage {
            id: "m".into(),
            source: "test".into(),
            role: Role::User,
            content: content.into(),
            original_length,
            created_at: chrono::Utc.timestamp_opt(1, 0).unwrap(),
            thread_id: "t".into(),
            thread_title: None,
            project_hint: None,
            content_hash: 0,
            hits: 1,
            signal_score: 0.0,
        }
    }

    #[test]
    fn backfills_original_length_when_zero() {
        let mut msg = make_msg("hello", 0);
        normalize(&mut msg);
        assert_eq!(msg.original_length, 5);
    }

    #[test]
    fn preserves_explicit_original_length() {
        let mut msg = make_msg("hi", 999);
        normalize(&mut msg);
        assert_eq!(
            msg.original_length, 999,
            "explicit value must not be overwritten"
        );
    }

    #[test]
    fn trims_trailing_whitespace() {
        let mut msg = make_msg("hello world  \n\t", 0);
        normalize(&mut msg);
        assert_eq!(msg.content, "hello world");
    }
}
