//! End-to-end integration test: real ChatGPT fixture → pipeline → writer.
//!
//! This is the smoke test for F3. If this passes, the full path from a
//! raw export to token-efficient output works. Each stage is also
//! unit-tested in its own module; this one validates the wiring.

use chrono::{TimeZone, Utc};
use loust_llm_mempipe::adapter::chatgpt::ChatGptAdapter;
use loust_llm_mempipe::pipeline::{parser, Pipeline, PipelineOutput};
use loust_llm_mempipe::PipelineConfig;

fn fixture() -> &'static str {
    include_str!("fixtures/chatgpt-tiny.json")
}

fn fixed_now() -> chrono::DateTime<Utc> {
    // 2023-11-14, well after all fixture messages (2023-11).
    Utc.timestamp_opt(1_700_000_000, 0).unwrap()
}

#[test]
fn full_pipeline_chatgpt_fixture_to_pipeline_output() {
    let adapter = ChatGptAdapter;
    let messages = parser::parse(&adapter, Box::new(fixture().as_bytes()), "chatgpt-tiny").unwrap();
    assert_eq!(messages.len(), 7, "fixture has 3 + 0 + 4 = 7 messages");

    let pipeline = Pipeline::new(PipelineConfig::with_safe_defaults());
    let output: PipelineOutput = pipeline.run(messages, fixed_now());

    // Stats sanity
    assert_eq!(output.stats.input_count, 7);
    // No secrets in the fixture, no redactions
    assert_eq!(output.stats.total_redactions, 0);
    // All messages are unique (no exact or fuzzy dupes in fixture)
    assert_eq!(output.stats.dedup_exact_dropped, 0);
    assert_eq!(output.stats.dedup_fuzzy_dropped, 0);
    // None are older than 3 years from fixed_now
    assert_eq!(output.stats.filtered_by_age, 0);
    // All should survive the signal_min filter (0.2)
    assert_eq!(output.stats.filtered_by_signal, 0);
    assert_eq!(output.stats.output_count, 7);

    // Every message has hits and signal_score populated
    for m in &output.messages {
        assert!(m.hits >= 1, "every survivor must have hits >= 1");
        assert!(
            m.signal_score > 0.0,
            "every survivor must have a positive score"
        );
    }

    // Output is sorted by signal_score DESC
    for window in output.messages.windows(2) {
        assert!(
            window[0].signal_score >= window[1].signal_score,
            "output must be sorted by signal_score DESC"
        );
    }
}

#[test]
fn pipeline_with_dupes_collapses_hits() {
    // Build an in-memory set with one exact duplicate.
    use loust_llm_mempipe::{NormalizedMessage, Role};
    let now = fixed_now();
    let mk = |id: &str, content: &str| NormalizedMessage {
        id: id.into(),
        source: "test".into(),
        role: Role::User,
        content: content.into(),
        original_length: 0,
        created_at: now,
        thread_id: "t1".into(),
        thread_title: Some("Test".into()),
        project_hint: Some("test".into()),
        content_hash: NormalizedMessage::compute_content_hash(content),
        hits: 1,
        signal_score: 0.0,
    };
    let messages = vec![
        mk("a", "How do I parse JSON in Rust?"),
        mk("b", "How do I parse JSON in Rust?"), // exact dupe
        mk("c", "What's the best Rust JSON library?"),
    ];

    let pipeline = Pipeline::new(PipelineConfig::with_safe_defaults());
    let output = pipeline.run(messages, now);

    assert_eq!(output.stats.input_count, 3);
    assert_eq!(output.stats.dedup_exact_dropped, 1);
    assert_eq!(output.stats.output_count, 2);
    let survivor = output.messages.iter().find(|m| m.id == "a").unwrap();
    assert_eq!(survivor.hits, 2, "exact dupe should bump hits to 2");
}

#[test]
fn pipeline_with_secrets_redacts_and_records_stats() {
    use loust_llm_mempipe::{NormalizedMessage, Role};
    let now = fixed_now();
    let mk = |id: &str, content: &str| NormalizedMessage {
        id: id.into(),
        source: "test".into(),
        role: Role::User,
        content: content.into(),
        original_length: 0,
        created_at: now,
        thread_id: "t1".into(),
        thread_title: Some("Test".into()),
        project_hint: Some("test".into()),
        content_hash: NormalizedMessage::compute_content_hash(content),
        hits: 1,
        signal_score: 0.0,
    };
    let messages = vec![
        mk("a", "my key AKIAIOSFODNN7EXAMPLE leaked"),
        mk("b", "email me at a@b.com"),
        mk("c", "no secrets here"),
    ];

    let pipeline = Pipeline::new(PipelineConfig::with_safe_defaults());
    let output = pipeline.run(messages, now);

    assert_eq!(output.stats.scrubbed_messages, 2);
    // 'a' has 1 redaction (aws), 'b' has 1 (email), 'c' has 0
    assert_eq!(output.stats.total_redactions, 2);

    // Verify redacted content made it through
    let a = output.messages.iter().find(|m| m.id == "a").unwrap();
    assert!(a.content.contains("[REDACTED:aws_key]"));
    assert!(!a.content.contains("AKIA"));

    let b = output.messages.iter().find(|m| m.id == "b").unwrap();
    assert!(b.content.contains("[REDACTED:email]"));

    let c = output.messages.iter().find(|m| m.id == "c").unwrap();
    assert_eq!(c.content, "no secrets here");
}

#[test]
fn pipeline_writes_jsonl_and_markdown() {
    use loust_llm_mempipe::pipeline::writer;
    use loust_llm_mempipe::OutputFormat;
    use tempfile::tempdir;

    let adapter = ChatGptAdapter;
    let messages = parser::parse(&adapter, Box::new(fixture().as_bytes()), "chatgpt-tiny").unwrap();
    let output = Pipeline::new(PipelineConfig::with_safe_defaults()).run(messages, fixed_now());

    let dir = tempdir().unwrap();
    let written = writer::write_all(dir.path(), &output, OutputFormat::Both).unwrap();
    assert!(written.iter().any(|p| p.ends_with("memory.jsonl")));
    assert!(written
        .iter()
        .any(|p| p.extension().and_then(|s| s.to_str()) == Some("md")));

    // JSONL should have one line per output message
    let jsonl_path = dir.path().join("memory.jsonl");
    let body = std::fs::read_to_string(&jsonl_path).unwrap();
    let line_count = body.lines().count();
    assert_eq!(line_count, output.stats.output_count);

    // Markdown should have at least 2 files (3 conversations, one empty
    // doesn't produce a file because it had 0 messages surviving the
    // pipeline; only non-empty threads write MD)
    let md_files: Vec<_> = written
        .iter()
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    assert!(
        md_files.len() >= 2,
        "expected at least 2 MD files, got {}",
        md_files.len()
    );
}
