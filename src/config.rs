//! Pipeline configuration. Defaults are tuned for a single-user operator
//! running this on a monthly ChatGPT export. CLI flags in F4 override these.

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Line-delimited JSON, one NormalizedMessage per line. Default.
    #[default]
    Jsonl,
    /// Hierarchical Markdown grouped by project_hint/thread_id.
    Markdown,
    /// Both files written under the output dir.
    Both,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Jaccard token similarity threshold (0.0-1.0) above which two
    /// messages are considered duplicates. 0.85 balances recall vs precision.
    pub dedup_threshold: f64,

    /// Minimum `signal_score` to keep a message. 0.2 prunes the long tail
    /// of low-signal noise without dropping actionable context.
    pub signal_min: f64,

    /// Drop messages older than this many days. 1095 = 3 years.
    pub max_thread_age_days: u64,

    /// Secret / PII patterns for the Rule E scrubber. Defaults ship in
    /// `PipelineConfig::with_safe_defaults()`; users can extend.
    pub secret_patterns: Vec<(SecretKind, Regex)>,

    /// Output format selection.
    pub output_format: OutputFormat,

    /// If true, the pipeline emits stats to stderr but writes no output.
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    AwsAccessKey,
    GitHubToken,
    OpenAiApiKey,
    AnthropicApiKey,
    PrivateIpv4,
    AbsoluteUserPath,
    EmailAddress,
}

impl SecretKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AwsAccessKey => "aws_key",
            Self::GitHubToken => "github_token",
            Self::OpenAiApiKey => "openai_key",
            Self::AnthropicApiKey => "anthropic_key",
            Self::PrivateIpv4 => "private_ip",
            Self::AbsoluteUserPath => "user_path",
            Self::EmailAddress => "email",
        }
    }
}

impl PipelineConfig {
    /// Safe defaults for operator use. Audit these before publishing
    /// to make sure no pattern is missing for your environment.
    pub fn with_safe_defaults() -> Self {
        let secret_patterns = vec![
            (
                SecretKind::AwsAccessKey,
                Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            ),
            (
                SecretKind::GitHubToken,
                Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36,}").unwrap(),
            ),
            (
                SecretKind::OpenAiApiKey,
                Regex::new(r"sk-[A-Za-z0-9]{48}").unwrap(),
            ),
            (
                SecretKind::AnthropicApiKey,
                Regex::new(r"sk-ant-[A-Za-z0-9\-_]{40,}").unwrap(),
            ),
            (
                SecretKind::PrivateIpv4,
                // RFC 1918: 10/8, 172.16/12, 192.168/16. Each pattern is
                // written with the exact number of trailing octets so
                // a 4-octet IPv4 is matched end-to-end.
                Regex::new(
                    r"(?:10\.\d{1,3}\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})",
                )
                .unwrap(),
            ),
            (
                SecretKind::AbsoluteUserPath,
                Regex::new(r"/(?:Users|home)/[^/\s]+").unwrap(),
            ),
            (
                SecretKind::EmailAddress,
                Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
            ),
        ];

        Self {
            dedup_threshold: 0.85,
            signal_min: 0.2,
            max_thread_age_days: 1095,
            secret_patterns,
            output_format: OutputFormat::Jsonl,
            dry_run: false,
        }
    }
}
