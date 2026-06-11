//! Pipeline orchestrator. The pipeline takes a stream of
//! `NormalizedMessage`s from any `Adapter` and produces token-efficient
//! output (JSONL or Markdown) ready for Claude Code / Projects.

use chrono::{DateTime, Utc};
use seahash::SeaHasher;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;

pub mod dedup;
pub mod normalizer;
pub mod parser;
pub mod scrubber;
pub mod signals;
pub mod writer;

use crate::config::PipelineConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
        }
    }
}

/// The canonical message shape that every Adapter must produce.
/// Downstream stages (scrubber, dedup, signals, writer) only know
/// this type — adapters are decoupled from each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    /// Stable identifier within the source (e.g. ChatGPT node UUID,
    /// Claude message UUID, JSONL line id).
    pub id: String,

    /// Source system identifier. Mirrors `AdapterKind::as_str()` in
    /// the serialized form for downstream grep-ability.
    pub source: String,

    /// Message role. Tool messages are kept for completeness but
    /// weighted low in `signals::signal_score`.
    pub role: Role,

    /// Scrubbed message body. Rule E applied upstream.
    pub content: String,

    /// Original (unscrubbed) length, kept for stats reporting.
    /// Set by the scrubber; if still 0 after normalization, defaults
    /// to `content.len()` (defensive against adapter bugs).
    pub original_length: u32,

    /// When the message was created in the source system.
    pub created_at: DateTime<Utc>,

    /// Source-specific thread identifier. Used to group messages
    /// in Markdown output.
    pub thread_id: String,

    /// Optional human-meaningful thread title (e.g. ChatGPT's
    /// `conversation.title`). Used for the Markdown file naming.
    pub thread_title: Option<String>,

    /// Heuristic project assignment (e.g. detected from first message
    /// or user-assigned tags). Used to group Markdown files.
    pub project_hint: Option<String>,

    /// FNV-1a 64-bit hash of the scrubbed content. Used as the
    /// first-pass dedup key before Jaccard similarity kicks in.
    pub content_hash: u64,

    /// Number of times this message appeared in the input (1 = unique,
    /// higher = duplicates collapsed into this survivor by `dedup`).
    /// Drives the `hits` term in `signal_score`.
    pub hits: u32,

    /// Composite `0.4·hits + 0.3·recency + 0.3·type_weight`. Populated
    /// by `signals::score`; downstream stages use it for ordering and
    /// the `signal_min` filter.
    pub signal_score: f32,
}

impl NormalizedMessage {
    /// Compute the FNV-1a-like content hash via `seahash`. Stable
    /// across runs and architectures (no random seed).
    pub fn compute_content_hash(content: &str) -> u64 {
        let mut hasher = SeaHasher::new();
        hasher.write(content.as_bytes());
        hasher.finish()
    }

    /// Slugify a string for use in a filename. Conservative:
    /// lowercase, alphanumeric + dash, max 64 chars.
    pub fn slugify(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut last_dash = false;
        for ch in input.chars() {
            let ch_lower = ch.to_ascii_lowercase();
            if ch_lower.is_ascii_alphanumeric() {
                out.push(ch_lower);
                last_dash = false;
            } else if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
            if out.len() >= 64 {
                break;
            }
        }
        let trimmed = out.trim_end_matches('-').to_string();
        if trimmed.is_empty() {
            "untitled".to_string()
        } else {
            trimmed
        }
    }
}

/// Top-level pipeline orchestrator. Wires together scrub → normalize →
/// dedup → signal-score → filter → (optional) write, in that order.
///
/// Construct via `Pipeline::new(cfg)` or `Pipeline::with_safe_defaults()`.
/// The `now` parameter on `run` makes the run deterministic for tests.
pub struct Pipeline {
    config: PipelineConfig,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    pub fn with_safe_defaults() -> Self {
        Self::new(PipelineConfig::with_safe_defaults())
    }

    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Run the full pipeline over `messages`. Returns kept messages
    /// (sorted by `signal_score` descending) plus stats.
    pub fn run(&self, messages: Vec<NormalizedMessage>, now: DateTime<Utc>) -> PipelineOutput {
        let input_count = messages.len();
        let mut msgs = messages;

        // 1) Scrub: Rule E secret/PII redaction. Sets original_length.
        let mut scrubbed_messages: usize = 0;
        let mut total_redactions: usize = 0;
        for msg in msgs.iter_mut() {
            let report = scrubber::scrub(msg, &self.config);
            if !report.redacted_kinds.is_empty() {
                scrubbed_messages += 1;
                total_redactions += report.redacted_kinds.len();
            }
        }

        // 2) Normalize: defensive backfill for adapter bugs.
        for msg in msgs.iter_mut() {
            normalizer::normalize(msg);
        }

        // 3) Dedup: two-pass (exact hash, then Jaccard token sim).
        let dedup_result = dedup::dedup(msgs, self.config.dedup_threshold);
        let mut msgs = dedup_result.survivors;
        let dedup_exact_dropped = dedup_result.exact_dropped;
        let dedup_fuzzy_dropped = dedup_result.fuzzy_dropped;

        // 4) Age filter: drop messages older than max_thread_age_days.
        let age_cutoff = now - chrono::Duration::days(self.config.max_thread_age_days as i64);
        let before_age = msgs.len();
        msgs.retain(|m| m.created_at >= age_cutoff);
        let filtered_by_age = before_age - msgs.len();

        // 5) Signal score: composite 0.4·hits + 0.3·recency + 0.3·type_weight.
        for msg in msgs.iter_mut() {
            signals::score(msg, now);
        }

        // 6) signal_min filter + sort by signal_score DESC.
        let before_signal = msgs.len();
        msgs.retain(|m| m.signal_score >= self.config.signal_min as f32);
        let filtered_by_signal = before_signal - msgs.len();
        msgs.sort_by(|a, b| {
            b.signal_score
                .partial_cmp(&a.signal_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let output_count = msgs.len();

        let stats = PipelineStats {
            input_count,
            scrubbed_messages,
            total_redactions,
            dedup_exact_dropped,
            dedup_fuzzy_dropped,
            filtered_by_age,
            filtered_by_signal,
            output_count,
        };

        PipelineOutput {
            messages: msgs,
            stats,
        }
    }
}

/// Pipeline run result. `messages` are sorted by `signal_score` DESC
/// and have `hits` + `signal_score` populated.
#[derive(Debug, Clone)]
pub struct PipelineOutput {
    pub messages: Vec<NormalizedMessage>,
    pub stats: PipelineStats,
}

/// Telemetry about a single pipeline run. All counts are non-decreasing
/// from the input side; `input_count` is the only "from" anchor.
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    pub input_count: usize,
    /// Number of messages where at least one redaction happened.
    pub scrubbed_messages: usize,
    /// Total redactions across all messages (one message can have many).
    pub total_redactions: usize,
    /// Duplicates dropped in the FNV-1a exact-hash pass.
    pub dedup_exact_dropped: usize,
    /// Near-duplicates dropped in the Jaccard pass.
    pub dedup_fuzzy_dropped: usize,
    /// Messages dropped because older than `max_thread_age_days`.
    pub filtered_by_age: usize,
    /// Messages dropped because `signal_score < signal_min`.
    pub filtered_by_signal: usize,
    /// Final kept count.
    pub output_count: usize,
}

impl PipelineStats {
    /// Render as a one-line stderr summary, e.g. for `--stats`.
    pub fn one_line(&self) -> String {
        format!(
            "in={} out={} scrubbed={} redactions={} dedup_exact={} dedup_fuzzy={} \
             age_drop={} signal_drop={}",
            self.input_count,
            self.output_count,
            self.scrubbed_messages,
            self.total_redactions,
            self.dedup_exact_dropped,
            self.dedup_fuzzy_dropped,
            self.filtered_by_age,
            self.filtered_by_signal,
        )
    }
}
