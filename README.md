# 🚀 loust-llm-mempipe

[![CI](https://github.com/LOUST-PRO/LLMmempipe/actions/workflows/ci.yml/badge.svg)](https://github.com/LOUST-PRO/LLMmempipe/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/loust-llm-mempipe.svg)](https://crates.io/crates/loust-llm-mempipe)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> Compile noisy LLM exports (ChatGPT, Claude, Gemini) into token-efficient JSONL + Markdown for Claude Code, Projects, and agent runtimes.

**Scope.** This is a **post-export compiler** for LLM data takeouts (ChatGPT / Claude / Gemini / Claude Code JSONL). It does **not** store, retrieve, or manage a memory system — it normalizes and reformats noisy exports into clean records that downstream tooling (Claude Code, MCP servers, RAG indexers) consume. If you are looking for a memory harness or agent context store, this is not it.

**Status: MVP complete (F1–F5 shipped).** The CLI surface is stable; the contract types are public; CI is green.

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

## Project status

| Phase | Scope | Status |
|---|---|---|
| F0.1 | Pre-publish audit (gh search) | ✅ done |
| F0.2 | Org hardening (2FA + member privileges) | ✅ done |
| F1 | Skeleton + Cargo.toml + contracts | ✅ done ([v0.1.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.1.0)) |
| F2 | ChatGPT adapter MVP | ✅ done ([v0.2.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.2.0)) |
| F3 | Pipeline core (scrubber + dedup + signals + writer) | ✅ done ([v0.3.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.3.0)) |
| F4 | CLI ergonomics | ✅ done ([v0.4.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.4.0)) |
| F5 | Validation (CI + smoke E2E) | ✅ done ([v0.5.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.5.0)) |
| F6 | Apache-2.0 only + repo URL unification + scope clarification | ✅ done ([v0.6.0](https://github.com/LOUST-PRO/LLMmempipe/releases/tag/v0.6.0)) |
| F7 | Public release announcement | ⏸️ (user opted out for now) |

## Build (current skeleton)

```bash
make all        # fmt-check + clippy + test + build
make release    # release binary
make info       # print build metadata
```

## License

Apache-2.0 — see [LICENSE](LICENSE).
