//! Meta-test: verifies the scrubber recognizes every fixture in
//! `test_fixtures` as a redactable pattern.
//!
//! This is a regression guard against two failure modes:
//!
//! 1. **Fixture added but regex not updated to match.** If a future
//!    contributor adds a new `FAKE_*` constant to `src/test_fixtures.rs`
//!    but forgets to add a corresponding pattern to
//!    `PipelineConfig::with_safe_defaults`, this test catches it.
//!
//! 2. **Regex updated but fixture strings no longer match.** If the
//!    scrubber's regex patterns get tightened (e.g., length minimum
//!    increased), the fixture strings may no longer match, and this
//!    test catches it before the individual `redacts_*` unit tests
//!    silently start failing.
//!
//! See `src/test_fixtures.rs` module docs for the rationale on why
//! these strings are not real secrets.

use chrono::TimeZone;
use loust_llm_mempipe::config::PipelineConfig;
use loust_llm_mempipe::pipeline::scrubber::scrub;
use loust_llm_mempipe::pipeline::{NormalizedMessage, Role};
use loust_llm_mempipe::test_fixtures::{
    FAKE_ABSOLUTE_PATH, FAKE_ANTHROPIC_API_KEY, FAKE_AWS_ACCESS_KEY, FAKE_EMAIL, FAKE_GITHUB_TOKEN,
    FAKE_PRIVATE_IP,
};

fn msg_with(content: &str) -> NormalizedMessage {
    NormalizedMessage {
        id: "meta-1".into(),
        source: "meta-test".into(),
        role: Role::User,
        content: content.into(),
        original_length: 0,
        created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        thread_id: "t-meta".into(),
        thread_title: None,
        project_hint: None,
        content_hash: 0,
        hits: 1,
        signal_score: 0.0,
    }
}

#[test]
fn every_fixture_is_redacted_by_the_scrubber() {
    let cfg = PipelineConfig::with_safe_defaults();
    let fixtures = [
        ("FAKE_AWS_ACCESS_KEY", FAKE_AWS_ACCESS_KEY),
        ("FAKE_ANTHROPIC_API_KEY", FAKE_ANTHROPIC_API_KEY),
        ("FAKE_GITHUB_TOKEN", FAKE_GITHUB_TOKEN),
        ("FAKE_EMAIL", FAKE_EMAIL),
        ("FAKE_PRIVATE_IP", FAKE_PRIVATE_IP),
        ("FAKE_ABSOLUTE_PATH", FAKE_ABSOLUTE_PATH),
    ];

    for (name, fixture) in fixtures {
        let mut msg = msg_with(fixture);
        let report = scrub(&mut msg, &cfg);
        assert!(
            report.was_redacted(),
            "fixture {name} = {fixture:?} was not redacted by any pattern \
             — did you forget to add a matching SecretKind to with_safe_defaults()?",
        );
        // Sanity: the literal fixture string should be gone from the
        // post-scrub content (or at least not equal to the original).
        assert_ne!(
            msg.content, fixture,
            "fixture {name} = {fixture:?} survived redaction (literal still equals content)"
        );
    }
}

#[test]
fn fixture_strings_match_their_declared_regex_pattern() {
    // Independent check at the regex layer, complementing the end-to-end
    // scrubber test above. Catches a pattern that exists but doesn't match
    // the fixture (e.g., if someone tightens the regex without updating
    // the fixture string).
    use loust_llm_mempipe::config::SecretKind;
    let cfg = PipelineConfig::with_safe_defaults();

    let pairs = [
        (SecretKind::AwsAccessKey, FAKE_AWS_ACCESS_KEY),
        (SecretKind::AnthropicApiKey, FAKE_ANTHROPIC_API_KEY),
        (SecretKind::GitHubToken, FAKE_GITHUB_TOKEN),
        (SecretKind::EmailAddress, FAKE_EMAIL),
        (SecretKind::PrivateIpv4, FAKE_PRIVATE_IP),
        (SecretKind::AbsoluteUserPath, FAKE_ABSOLUTE_PATH),
    ];

    for (kind, fixture) in pairs {
        let pattern = cfg
            .secret_patterns
            .iter()
            .find(|(k, _)| k == &kind)
            .unwrap_or_else(|| panic!("no pattern registered for SecretKind::{kind:?}"));
        assert!(
            pattern.1.is_match(fixture),
            "regex for {kind:?} does not match its declared fixture {fixture:?} \
             — the fixture string drifted from what the pattern accepts",
        );
    }
}
