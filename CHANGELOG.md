# Changelog

All notable changes to loust-llm-mempipe are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] — 2026-06-11

### Added
- F2: ChatGPT export adapter (production implementation, replaces the F1 stub).
  - Streaming JSON deserializer via `serde_json::from_reader`
  - Thread reconstruction: walks `current_node` → parent chain → root → forward
  - Role mapping: `user`/`assistant`/`system`/`tool`; unknown roles dropped
  - Skips empty / whitespace-only content
  - Skips structured `parts` (image_url, code, tool calls) — text only
  - Detection: sniffs `"mapping"` or `"conversations"` in the file header
- F2: Synthetic test fixture `tests/fixtures/chatgpt-tiny.json` (3 conversations,
  covers linear thread, empty conversation, system message).
- F2: 6 unit tests for the ChatGPT adapter: detect positive/negative, thread
  reconstruction, empty conversation handling, system role preservation, unknown
  role drop, empty content drop.
- F2: ChatGPT adapter exposed via `loust_llm_mempipe::adapter::chatgpt::ChatGptAdapter`
  (currently private; will be public once F4 lands the CLI).

### Notes
- Library API: `NormalizedMessage`, `Role`, `PipelineConfig`, `Adapter`,
  `AdapterKind` are stable since 0.1.0. No breaking changes in 0.2.0.
- CLI: still a placeholder (F4). The library is usable from Rust code today.

## [0.1.0] — 2026-06-10

Initial skeleton release. CLI is a placeholder (`--version` and `--info`
work; the full surface lands in F4). Library types are stable and ready
for downstream consumption.

[Unreleased]: https://github.com/LOUST-PRO/loust-llm-mempipe/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/LOUST-PRO/loust-llm-mempipe/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/LOUST-PRO/loust-llm-mempipe/releases/tag/v0.1.0
