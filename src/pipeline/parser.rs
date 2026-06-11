//! Streaming JSON / adapter → `Vec<NormalizedMessage>` wrapper.
//!
//! Adapters return `Box<dyn Iterator<Item = NormalizedMessage>>` to
//! preserve zero-copy streaming, but the pipeline stages downstream
//! (dedup, signal sort) need a `Vec` to do whole-batch work. This
//! module is the bridge: it consumes the adapter iterator into a Vec
//! and surfaces adapter errors as `anyhow::Error`.
//!
//! For F3 we materialize the full set in memory. The F2 ChatGPT adapter
//! already does this internally (full export loaded into `Vec<ChatGptConversation>`),
//! so this is consistent. True streaming through dedup (e.g. a
//! HyperLogLog sketch or block-level pass) is a future optimization.

use crate::adapter::Adapter;
use crate::pipeline::NormalizedMessage;
use anyhow::{Context, Result};
use std::io::Read;

/// Drive an adapter to completion and collect all messages. Surfaces
/// adapter I/O and parsing errors with the file path as context.
pub fn parse<A: Adapter + ?Sized>(
    adapter: &A,
    reader: Box<dyn Read>,
    source_label: &str,
) -> Result<Vec<NormalizedMessage>> {
    let iter = adapter
        .stream_messages(reader)
        .with_context(|| format!("adapter {} failed to stream", source_label))?;
    Ok(iter.collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::chatgpt::ChatGptAdapter;

    #[test]
    fn parse_chatgpt_fixture_into_vec() {
        let fixture = include_str!("../../tests/fixtures/chatgpt-tiny.json");
        let adapter = ChatGptAdapter;
        let msgs = parse(&adapter, Box::new(fixture.as_bytes()), "chatgpt-tiny").unwrap();
        // 3 convs: 3 + 0 + 4 = 7 messages
        assert_eq!(msgs.len(), 7);
    }
}
