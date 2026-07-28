//! Shared test fixtures for the scrubber test suite.
//!
//! These constants exist so the scrubber has realistic-looking inputs to
//! redact in tests, without copy-pasting the same placeholder strings
//! across `src/lib.rs`, `src/pipeline/scrubber.rs`, and the integration
//! tests under `tests/`.
//!
//! ## Why these strings are not real secrets
//!
//! - `FAKE_AWS_ACCESS_KEY` is `AKIAIOSFODNN7EXAMPLE` — Amazon publishes
//!   this literal in their AWS docs as the "example" key. Not real.
//! - `FAKE_ANTHROPIC_API_KEY` is `sk-ant-api03-` followed by contiguous
//!   alphabet+digits (`abcdefghijklmnopqrstuvwxyz0123456789ABCD`).
//!   Real Anthropic keys have ~93 random base62 chars after the prefix
//!   — never contiguous alphabet.
//! - `FAKE_GITHUB_TOKEN` is `ghp_` followed by contiguous alphabet+digits
//!   (`1234567890abcdefghijklmnopqrstuvwxyzAB`). Real PATs are random.
//! - `FAKE_EMAIL` / `FAKE_PRIVATE_IP` / `FAKE_ABSOLUTE_PATH` are
//!   obviously not real PII.
//!
//! ## Scanner caveats
//!
//! Upstream secret scanners (GitHub Secret Scanning, KeyScout) pattern
//! match on the prefix shape and may still flag these. If a scanner
//! opens an issue against this repo referencing these literals, the
//! response is: "constructed test fixture, see `src/test_fixtures.rs`".
//! See `CHANGELOG.md` v0.6.2 for the consolidation that reduced the
//! duplication that triggered the original KeyScout report.
//!
//! ## Visibility
//!
//! Marked `#[doc(hidden)]` in `lib.rs` so the module does not appear
//! in rustdoc, but kept `pub` so integration tests under `tests/`
//! can import the constants. This is the same pattern used by
//! `tokio`, `serde`, and other published crates with shared test
//! fixtures.

#![allow(dead_code)] // Different tests use different subsets

pub const FAKE_AWS_ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
pub const FAKE_ANTHROPIC_API_KEY: &str = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789ABCD";
pub const FAKE_GITHUB_TOKEN: &str = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
pub const FAKE_EMAIL: &str = "a@b.com";
pub const FAKE_PRIVATE_IP: &str = "192.168.1.42";
pub const FAKE_ABSOLUTE_PATH: &str = "/home/lou/secret.txt";
