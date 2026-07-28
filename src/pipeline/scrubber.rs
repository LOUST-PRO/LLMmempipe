//! Rule E secret / PII scrubber. Walks `PipelineConfig.secret_patterns`
//! in order and replaces each match with `[REDACTED:<kind>]`. Also
//! captures the pre-redaction content length into `original_length`
//! so the writer can report "X bytes of secrets redacted across N messages".
//!
//! Order matters when patterns can overlap (e.g. an email might also
//! contain a token-looking substring). We sort patterns so that the
//! most specific (longest literal) runs first. In `with_safe_defaults`
//! the patterns are tuned to be mostly disjoint, but this is defensive.

use crate::config::{PipelineConfig, SecretKind};
use crate::pipeline::NormalizedMessage;

/// Summary of what the scrubber did to a single message.
#[derive(Debug, Default, Clone)]
pub struct ScrubReport {
    /// Distinct kinds redacted in this message, in pattern order.
    /// Empty list = nothing was redacted.
    pub redacted_kinds: Vec<SecretKind>,
}

impl ScrubReport {
    pub fn was_redacted(&self) -> bool {
        !self.redacted_kinds.is_empty()
    }
}

/// Apply Rule E to a single message in place. Returns a report listing
/// which secret kinds were redacted. Always sets `original_length` to
/// the pre-scrub byte count (even if nothing matched), so downstream
/// stats can compute `original_length - content.len() == redacted bytes`.
pub fn scrub(msg: &mut NormalizedMessage, cfg: &PipelineConfig) -> ScrubReport {
    let pre_len = msg.content.len();
    let mut report = ScrubReport::default();

    for (kind, pattern) in &cfg.secret_patterns {
        // Skip if the pattern can't match (cheap pre-check).
        if !pattern.is_match(&msg.content) {
            continue;
        }
        // Replace all matches with a stable label. `replace_all` rewrites
        // every occurrence; for repeated leaks we get one redaction per
        // occurrence (collapsed into a single kind entry in the report).
        let replacement = format!("[REDACTED:{}]", kind.label());
        let new_content = pattern.replace_all(&msg.content, replacement.as_str());
        if new_content != msg.content {
            msg.content = new_content.into_owned();
            if !report.redacted_kinds.contains(kind) {
                report.redacted_kinds.push(*kind);
            }
        }
    }

    msg.original_length = pre_len as u32;
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretKind;
    use crate::pipeline::Role;
    use crate::test_fixtures::{
        FAKE_ABSOLUTE_PATH, FAKE_ANTHROPIC_API_KEY, FAKE_AWS_ACCESS_KEY, FAKE_EMAIL,
        FAKE_GITHUB_TOKEN, FAKE_PRIVATE_IP,
    };
    use chrono::TimeZone;

    fn make_msg(content: &str) -> NormalizedMessage {
        NormalizedMessage {
            id: "m1".into(),
            source: "chatgpt".into(),
            role: Role::User,
            content: content.into(),
            original_length: 0,
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            thread_id: "t1".into(),
            thread_title: None,
            project_hint: None,
            content_hash: 0,
            hits: 1,
            signal_score: 0.0,
        }
    }

    #[test]
    fn sets_original_length_to_pre_scrub_byte_count() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg("hello world");
        let _ = scrub(&mut msg, &cfg);
        assert_eq!(msg.original_length, "hello world".len() as u32);
    }

    #[test]
    fn redacts_aws_access_key() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("key={} leaked", FAKE_AWS_ACCESS_KEY));
        let report = scrub(&mut msg, &cfg);
        assert!(report.was_redacted());
        assert!(report.redacted_kinds.contains(&SecretKind::AwsAccessKey));
        assert!(msg.content.contains("[REDACTED:aws_key]"));
        assert!(!msg.content.contains("AKIA"));
    }

    #[test]
    fn redacts_anthropic_api_key() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("auth: {}", FAKE_ANTHROPIC_API_KEY));
        let report = scrub(&mut msg, &cfg);
        assert!(report.redacted_kinds.contains(&SecretKind::AnthropicApiKey));
        assert!(msg.content.contains("[REDACTED:anthropic_key]"));
    }

    #[test]
    fn redacts_github_token() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("token: {}", FAKE_GITHUB_TOKEN));
        let report = scrub(&mut msg, &cfg);
        assert!(report.redacted_kinds.contains(&SecretKind::GitHubToken));
    }

    #[test]
    fn redacts_email_address() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("ping {} later", FAKE_EMAIL));
        let report = scrub(&mut msg, &cfg);
        assert!(report.redacted_kinds.contains(&SecretKind::EmailAddress));
        assert!(!msg.content.contains("@b.com"));
    }

    #[test]
    fn redacts_private_ipv4() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("server at {} was down", FAKE_PRIVATE_IP));
        let report = scrub(&mut msg, &cfg);
        assert!(report.redacted_kinds.contains(&SecretKind::PrivateIpv4));
        assert!(msg.content.contains("[REDACTED:private_ip]"));
    }

    #[test]
    fn redacts_absolute_user_path() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("see {} for details", FAKE_ABSOLUTE_PATH));
        let report = scrub(&mut msg, &cfg);
        assert!(report
            .redacted_kinds
            .contains(&SecretKind::AbsoluteUserPath));
    }

    #[test]
    fn cleans_text_without_any_secrets() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg("no secrets here, just thoughts");
        let report = scrub(&mut msg, &cfg);
        assert!(!report.was_redacted());
        assert_eq!(msg.content, "no secrets here, just thoughts");
        assert_eq!(msg.original_length, msg.content.len() as u32);
    }

    #[test]
    fn redacts_multiple_kinds_in_one_message() {
        let cfg = PipelineConfig::with_safe_defaults();
        let mut msg = make_msg(&format!("key {} email {}", FAKE_AWS_ACCESS_KEY, FAKE_EMAIL));
        let report = scrub(&mut msg, &cfg);
        assert!(report.redacted_kinds.contains(&SecretKind::AwsAccessKey));
        assert!(report.redacted_kinds.contains(&SecretKind::EmailAddress));
    }
}
