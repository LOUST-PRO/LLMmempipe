# Changelog

All notable changes to loust-llm-mempipe are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.6.1] — 2026-07-15

### Changed
- README: removed defensive "this is not a memory harness" scope callout.
  The tool is described positively by what it does (post-export compiler)
  instead of what it isn't.
- README: replaced internal phase labels (F1–F5) with version numbers
  in the Project status table.
- CHANGELOG: stripped F-code prefixes (F2/F3/F4/F5/F6) from historical
  v0.1.0–v0.6.0 entries; these were development labels and have no
  value for downstream readers.

### Notes
- Docs-only patch. No source-code or contract-type changes.
- Republishes to crates.io to surface the cleaned README on the
  crate page.

## [0.6.0] — 2026-07-15

### Changed
- **License: Apache-2.0 only** (was `MIT OR Apache-2.0` dual). Added a full
  Apache-2.0 LICENSE file at the repo root.
- **Repository URL unified**: all README links, CI badge, and Cargo.toml
  `repository` field now point to `https://github.com/LOUST-PRO/LLMmempipe`
  (was `https://github.com/LOUST-PRO/loust-llm-mempipe`, which 404'd).
- Cargo.toml keyword `memory` replaced with `memory-export` to better
  describe the export-compilation use case in crates.io search results.

### Notes
- No source-code semantic changes — this is a metadata/docs/license batch.
- v0.6.0 republishes to crates.io replacing the v0.0.2 placeholder.

## [0.5.0] — 2026-06-11

### Added
- GitHub Actions CI workflow at `.github/workflows/ci.yml`. Runs on
  every push to `main` and on every pull request. Matrix with
  `stable` + `beta` toolchains. Steps: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`,
  `cargo build --release`. Uses `Swatinem/rust-cache@v2` to cache the
  cargo registry and build target. Minimal permissions (`contents:
  read` only). No secrets, no write tokens.
- README CI badge in the header — links to the workflow run history.

### Notes
- CI is the last public-MVP phase. Subsequent work would be:
  - Adding release automation (build on tag, attach binaries to
    GitHub release)
  - Adding a `cargo audit` step for known-vulnerable deps
  - Adding code coverage (cargo-tarpaulin → codecov or similar)
  - Adding docs.rs config so the library API is browsable online
- Validation: `cargo fmt --check` clean, `cargo clippy --all-targets
  -- -D warnings` clean, `cargo test` 61/61 pass, `cargo build
  --release` 12s. Workflow YAML validated for parse-correctness;
  first end-to-end run will fire on the v0.5.0 push.

## [0.4.0] — 2026-06-11

### Added
- clap CLI surface in `src/main.rs` (was a stub in 0.1.0-0.3.0).
  Flags:
  - `-i, --input <PATH>` (required) — raw export file
  - `-o, --output <DIR>` (required) — output dir, created if missing
  - `-f, --format <jsonl|markdown|both>` (default `jsonl`)
  - `--adapter <chatgpt|claude_web|gemini|claude_code>` (default: auto-detect)
  - `--dedup-threshold <FLOAT>` (default 0.85)
  - `--signal-min <FLOAT>` (default 0.2)
  - `--max-age-days <INT>` (default 1095)
  - `--stats` — print one-line stats to stderr
  - `--dry-run` — compute but don't write
  - `--info` — print build metadata and exit
  - `--version`, `--help` — clap defaults
- `loust_llm_mempipe::adapter::registry()` — ordered list of all
  known adapters for auto-detection.
- `loust_llm_mempipe::adapter::pick_adapter(kind, header)` —
  returns the adapter for an explicit kind, or the first adapter whose
  `detect()` matches the header. Used by the CLI to auto-select.
- `OutputFormat::from_cli(s)` and `AdapterKind::from_cli(s)` —
  kebab-case parsers used by clap's `value_parser`.
- stderr progress lines on every run: `detected adapter: <kind>`,
  `parsed N messages`, `stats: <one-line>`, `wrote: <path>`, `done: N
  files written`. Mirrors what humans want when running interactively.
- Exit code 0 on success, non-zero on any error (clap parse,
  missing input file, unsupported adapter). Surfaces errors via
  `anyhow` with file-path context.
- Integration test `tests/cli_e2e.rs` — 9 tests that shell out to
  the built binary with the real ChatGPT fixture. Covers `--version`,
  `--help`, `--info`, full run with `--format both --stats`, dry-run,
  explicit `--adapter chatgpt`, rejection of unknown format / adapter,
  graceful failure on missing input.

### Notes
- Library API: `Adapter` trait, `AdapterKind` enum, and `registry()` /
  `pick_adapter()` are stable since 0.4.0. `OutputFormat::from_cli`
  and `AdapterKind::from_cli` are new.
- The CLI is now feature-complete for the public MVP. Subsequent
  work (CI hardening, public release) is not a blocker for downstream
  consumption.
- Validation: `cargo fmt --check` clean, `cargo clippy --all-targets
  -- -D warnings` clean, `cargo test` 61/61 pass (44 lib + 4 main clap +
  9 cli_e2e + 4 e2e), `cargo build --release` ~12s, smoke test
  against the real fixture produces `memory.jsonl` + 2 Markdown files.

## [0.3.0] — 2026-06-11

### Added
- Pipeline core — `Pipeline` orchestrator wires together scrub →
  normalize → dedup → age filter → signal score → sort. Public types
  `Pipeline`, `PipelineOutput`, `PipelineStats` (re-exported from
  `loust_llm_mempipe`).
- Rule E secret scrubber (`pipeline::scrubber::scrub`). Applies all
  patterns from `PipelineConfig.secret_patterns` in order, replaces each
  match with `[REDACTED:<kind>]`, and captures `original_length` (the
  pre-scrub byte count) for stats. 9 unit tests cover AWS / GitHub /
  Anthropic / OpenAI / email / private IP / user path redaction, plus
  clean-text passthrough and multi-kind messages.
- Two-pass dedup (`pipeline::dedup::dedup`). Pass 1 is exact by
  FNV-1a `content_hash`; Pass 2 is Jaccard token similarity with the
  threshold from `PipelineConfig.dedup_threshold` (0.85 default).
  Duplicates fold into the survivor via the `hits` counter. 5 unit
  tests cover exact, fuzzy, threshold, and disjoint-set edge cases.
- Composite signal scoring (`pipeline::signals::score`).
  `0.4 · hits_norm + 0.3 · recency + 0.3 · type_weight` where
  - `hits_norm` saturates at 10
  - `recency` is `exp(-age_days / 365.0)`, clamped to [0, 1]
  - `type_weight` is `assistant=1.0`, `user=0.8`, `tool=0.5`, `system=0.3`
  6 unit tests cover each term and the combined formula.
- Output writers (`pipeline::writer`). JSONL writes
  `dir/memory.jsonl` (one `NormalizedMessage` per line, for
  `claude-code --context`). Markdown writes a `dir/<project>/<thread>.md`
  hierarchy with metadata frontmatter and `## role` sections per
  message (for Claude Projects). `write_all` dispatches on
  `OutputFormat::Jsonl | Markdown | Both`. 4 unit tests cover JSONL
  line shape, MD grouping, `_untitled` fallback, and the combined path.
- Defensive normalizer (`pipeline::normalizer::normalize`) — fills
  in `original_length` if an adapter forgot, and trims trailing
  whitespace introduced by redactions. 3 unit tests.
- Adapter → Vec bridge (`pipeline::parser::parse`). Consumes the
  `Box<dyn Iterator>` from any `Adapter` into a `Vec<NormalizedMessage>`
  with `anyhow::Error` context on adapter failure. 1 unit test against
  the real ChatGPT fixture.
- `NormalizedMessage` extended with `hits: u32` (set by adapters
  to 1, incremented by dedup) and `signal_score: f32` (computed by
  `signals::score`). Adapters without these defaults to 0/0 until the
  orchestrator fills them.
- `PipelineStats::one_line()` — one-line stderr summary for
  `--stats` / CI consumption.
- End-to-end integration test `tests/pipeline_e2e.rs` — ChatGPT
  fixture → parser → pipeline → writer. Verifies stats correctness,
  signal ordering, redaction content, dedup hits bump, JSONL+MD file
  layout. 4 tests.
- Fix `private_ip` regex — the previous pattern required 3 trailing
  octets after `192.168`, but RFC 1918 IPs only have 2. New pattern
  is `(?:10\.\d.\d.\d | 192\.168\.\d.\d | 172.(16-31).\d.\d)`.

### Notes
- Library API additions: `Pipeline`, `PipelineOutput`, `PipelineStats`,
  `pipeline::scrubber::scrub`, `pipeline::dedup::dedup`,
  `pipeline::signals::score`, `pipeline::writer::{write_jsonl,
  write_markdown, write_all}`, `pipeline::parser::parse`. No breaking
  removals. Adapters compiled against 0.2.0 will need to add `hits: 1,
  signal_score: 0.0` to their `NormalizedMessage` initializers (the
  v0.2.0 ChatGPT adapter already shipped that update).
- Validation: `cargo fmt --check` clean, `cargo clippy --all-targets
  -- -D warnings` clean, `cargo test` 48/48 pass (9 lib + 6 chatgpt +
  9 scrubber + 5 dedup + 6 signals + 4 writer + 3 normalizer + 1 parser
  + 4 e2e + 1 main binary + 0 doc), `cargo build --release` 21s.

## [0.2.0] — 2026-06-11

### Added
- ChatGPT export adapter (production implementation, replaces the
  v0.1.0 stub).
  - Streaming JSON deserializer via `serde_json::from_reader`
  - Thread reconstruction: walks `current_node` → parent chain → root → forward
  - Role mapping: `user`/`assistant`/`system`/`tool`; unknown roles dropped
  - Skips empty / whitespace-only content
  - Skips structured `parts` (image_url, code, tool calls) — text only
  - Detection: sniffs `"mapping"` or `"conversations"` in the file header
- Synthetic test fixture `tests/fixtures/chatgpt-tiny.json` (3 conversations,
  covers linear thread, empty conversation, system message).
- 6 unit tests for the ChatGPT adapter: detect positive/negative, thread
  reconstruction, empty conversation handling, system role preservation, unknown
  role drop, empty content drop.
- ChatGPT adapter exposed via `loust_llm_mempipe::adapter::chatgpt::ChatGptAdapter`
  (currently private; will be public once the CLI ships).

### Notes
- Library API: `NormalizedMessage`, `Role`, `PipelineConfig`, `Adapter`,
  `AdapterKind` are stable since 0.1.0. No breaking changes in 0.2.0.
- CLI: still a placeholder. The library is usable from Rust code today.

## [0.1.0] — 2026-06-10

Initial skeleton release. CLI is a placeholder (`--version` and `--info`
work; the full surface ships in v0.4.0). Library types are stable and ready
for downstream consumption.

[Unreleased]: https://github.com/LOUST-PRO/loust-llm-mempipe/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/LOUST-PRO/loust-llm-mempipe/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/LOUST-PRO/loust-llm-mempipe/releases/tag/v0.1.0
