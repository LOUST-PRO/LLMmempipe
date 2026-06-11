//! loust-llm-mempipe — public library surface.
//!
//! Re-exports the contract types that downstream tooling (MCP servers,
//! Claude Code plugins, RAG indexers) can consume without depending on
//! the binary CLI shape.

pub mod adapter;
pub mod config;
pub mod pipeline;

pub use adapter::{Adapter, AdapterKind};
pub use config::{OutputFormat, PipelineConfig, SecretKind};
pub use pipeline::{NormalizedMessage, Pipeline, PipelineOutput, PipelineStats, Role};

#[cfg(test)]
mod tests {
    use super::*;

    // --- NormalizedMessage helpers ---

    #[test]
    fn compute_content_hash_is_deterministic() {
        let a = NormalizedMessage::compute_content_hash("hello world");
        let b = NormalizedMessage::compute_content_hash("hello world");
        assert_eq!(a, b, "same input must produce same hash");
    }

    #[test]
    fn compute_content_hash_differs_for_different_input() {
        let a = NormalizedMessage::compute_content_hash("hello world");
        let b = NormalizedMessage::compute_content_hash("hello, world");
        assert_ne!(a, b);
    }

    #[test]
    fn slugify_lowercases_and_replaces_non_alnum() {
        assert_eq!(
            NormalizedMessage::slugify("Project Alpha!"),
            "project-alpha"
        );
        assert_eq!(
            NormalizedMessage::slugify("  multi   space  "),
            "multi-space"
        );
        assert_eq!(NormalizedMessage::slugify("___"), "untitled");
        assert_eq!(NormalizedMessage::slugify("CamelCase42"), "camelcase42");
    }

    #[test]
    fn slugify_caps_length_at_64() {
        let long = "a".repeat(200);
        let slug = NormalizedMessage::slugify(&long);
        assert!(slug.len() <= 64, "slug should be capped at 64 chars");
    }

    // --- Role ---

    #[test]
    fn role_as_str_matches_serde_lowercase() {
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn role_serializes_lowercase() {
        let r = serde_json::to_string(&Role::Assistant).unwrap();
        assert_eq!(r, "\"assistant\"");
    }

    // --- PipelineConfig defaults ---

    #[test]
    fn defaults_have_safe_thresholds() {
        let cfg = PipelineConfig::with_safe_defaults();
        assert!((cfg.dedup_threshold - 0.85).abs() < f64::EPSILON);
        assert!((cfg.signal_min - 0.2).abs() < f64::EPSILON);
        assert_eq!(cfg.max_thread_age_days, 1095);
        assert_eq!(cfg.output_format, OutputFormat::Jsonl);
        assert!(!cfg.dry_run);
    }

    #[test]
    fn defaults_include_core_secret_patterns() {
        let cfg = PipelineConfig::with_safe_defaults();
        let labels: Vec<&str> = cfg.secret_patterns.iter().map(|(k, _)| k.label()).collect();
        assert!(labels.contains(&"aws_key"));
        assert!(labels.contains(&"github_token"));
        assert!(labels.contains(&"anthropic_key"));
        assert!(labels.contains(&"openai_key"));
        assert!(labels.contains(&"private_ip"));
        assert!(labels.contains(&"email"));
    }

    #[test]
    fn scrubber_patterns_match_realistic_secrets() {
        let cfg = PipelineConfig::with_safe_defaults();

        let sample = "key=AKIAIOSFODNN7EXAMPLE leaked";
        let aws_pat = cfg
            .secret_patterns
            .iter()
            .find(|(k, _)| k == &SecretKind::AwsAccessKey)
            .unwrap();
        assert!(aws_pat.1.is_match(sample));

        let sample = "Authorization: sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
        let ant_pat = cfg
            .secret_patterns
            .iter()
            .find(|(k, _)| k == &SecretKind::AnthropicApiKey)
            .unwrap();
        assert!(ant_pat.1.is_match(sample));

        let sample = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
        let gh_pat = cfg
            .secret_patterns
            .iter()
            .find(|(k, _)| k == &SecretKind::GitHubToken)
            .unwrap();
        assert!(gh_pat.1.is_match(sample));

        let sample = "reach me at davidmirelesll@outlook.com";
        let email_pat = cfg
            .secret_patterns
            .iter()
            .find(|(k, _)| k == &SecretKind::EmailAddress)
            .unwrap();
        assert!(email_pat.1.is_match(sample));
    }
}
