# 🚀 loust-llm-mempipe

[![CI](https://github.com/LOUST-PRO/LLMmempipe/actions/workflows/ci.yml/badge.svg)](https://github.com/LOUST-PRO/LLMmempipe/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/loust-llm-mempipe.svg)](https://crates.io/crates/loust-llm-mempipe)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> Compile noisy LLM exports (ChatGPT, Claude, Gemini) into token-efficient JSONL + Markdown for Claude Code, Projects, and agent runtimes.

**Status: MVP shipped through v0.5.0; v0.6.0 is a metadata/license batch.** The CLI surface is stable; the contract types are public; CI is green.

## The problem

You export 50 MB of `conversations.json` from ChatGPT. Now you have a JSON file full of OpenAI message IDs, internal mapping UUIDs, and duplicate threads. Pasting it into Claude Projects or `claude-code --context` burns tokens and dilutes attention. The same applies to Claude Web takeouts, Gemini takeouts, and your local Claude Code JSONL sessions.

## What this does

A single-binary Rust CLI that:

- 🔍 Strips UI noise, system prompts, and broken/empty messages
- 🛡️ Redacts secrets and PII (Rule E gate) before anything touches disk
- 🧬 Deduplicates via FNV-1a hashing + Jaccard token similarity (threshold 0.85)
- 📊 Scores each memory with `0.4·hits + 0.3·recency + 0.3·type_weight`
- 📦 Outputs `.jsonl` (Claude Code ready) and/or hierarchical `.md` (Claude Projects ready)

## Install

```bash
cargo install loust-llm-mempipe
# or download a binary from Releases
```

## Usage

```bash
# 1. Export from ChatGPT: Settings → Data Controls → Export Data
#    Unzip and find conversations.json

# 2. Pipe it through mempipe:
loust-llm-mempipe \
  --input conversations.json \
  --output ./claude-memory/ \
  --format jsonl \
  --stats

# 3. Point Claude Code at it:
claude-code --context ./claude-memory/memory.jsonl
```

## Library API

The crate exposes a public library surface alongside the binary CLI, so downstream tooling (MCP servers, Claude Code plugins, RAG indexers) can consume the pipeline without depending on the CLI shape.

```rust
use loust_llm_mempipe::{Pipeline, PipelineConfig, AdapterKind, OutputFormat};

let config = PipelineConfig {
    input_path: "conversations.json".into(),
    output_dir: "./out".into(),
    format: OutputFormat::Jsonl,
    adapter: AdapterKind::ChatGpt,
    ..Default::default()
};
let pipeline = Pipeline::new(config);
let output = pipeline.run()?;
println!("wrote {} messages", output.stats.kept);
```

Re-exports from [`src/lib.rs`](src/lib.rs): `Adapter`, `AdapterKind`, `OutputFormat`, `PipelineConfig`, `SecretKind`, `NormalizedMessage`, `Pipeline`, `PipelineOutput`, `PipelineStats`, `Role`.

## Project status

| Phase | Scope | Status |
|---|---|---|
| Pre-release | Pre-publish audit (gh search) | ✅ done |
| Pre-release | Org hardening (2FA + member privileges) | ✅ done |
| v0.1.0 | Skeleton + Cargo.toml + contracts | ✅ done ([v0.1.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.1.0)) |
| v0.2.0 | ChatGPT adapter MVP | ✅ done ([v0.2.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.2.0)) |
| v0.3.0 | Pipeline core (scrubber + dedup + signals + writer) | ✅ done ([v0.3.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.3.0)) |
| v0.4.0 | CLI ergonomics | ✅ done ([v0.4.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.4.0)) |
| v0.5.0 | Validation (CI + smoke E2E) | ✅ done ([v0.5.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.5.0)) |
| v0.6.0 | Apache-2.0 only + repo URL unification | ✅ done ([v0.6.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.6.0)) |
| v0.7.0 | Public release announcement | ⏸️ deferred |

## Build (current skeleton)

```bash
make all        # fmt-check + clippy + test + build
make release    # release binary
make info       # print build metadata
```

## Contributing

Issues and PRs are tracked on [GitHub](https://github.com/LOUST-PRO/LLMmempipe/issues). For substantive changes, open an issue first to align scope before sending code — the contract types are the public surface and changing them affects downstream tooling. Bug reports are most useful with a small reproducer (input fixture + observed vs expected output). The maintainer reviews every PR but response windows may vary.

## Acknowledgments

Thanks to the maintainers of the upstream LLM export formats that this tool ingests (ChatGPT, Claude, Gemini, Claude Code JSONL), and to early reviewers who helped shape the contract types during the initial development cycle.

## License

Apache-2.0 — see [LICENSE](LICENSE).
