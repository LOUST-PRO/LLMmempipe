//! Two-pass dedup. Order matters.
//!
//! **Pass 1 (exact)**: group by `content_hash` (FNV-1a of the scrubbed
//! content, set by the adapter). First occurrence survives; later
//! identical ones are dropped. The survivor's `hits` field is
//! incremented by the number of dropped dupes.
//!
//! **Pass 2 (fuzzy / Jaccard)**: for each remaining survivor in input
//! order, compute Jaccard similarity with all earlier survivors. If
//! similarity > `threshold`, drop the current and bump the earlier's
//! `hits`.
//!
//! Jaccard is computed on a token set built from
//! `lowercase(content) split on non-alphanumeric`. Cheap, deterministic,
//! and good enough for collapsing near-duplicates like "How do I parse
//! JSON in Rust?" vs "how do i parse json in rust?".

use crate::pipeline::NormalizedMessage;
use std::collections::HashSet;

/// Result of running the dedup passes.
#[derive(Debug, Default, Clone)]
pub struct DedupResult {
    pub survivors: Vec<NormalizedMessage>,
    pub exact_dropped: usize,
    pub fuzzy_dropped: usize,
}

pub fn dedup(messages: Vec<NormalizedMessage>, threshold: f64) -> DedupResult {
    let mut result = DedupResult::default();
    let _ = threshold; // used in pass 2 below
    let _ = result; // mutated below

    // --- Pass 1: exact by FNV-1a content_hash ---
    // First-seen wins; later dupes get folded into the survivor.
    let mut survivors_pass1: Vec<NormalizedMessage> = Vec::with_capacity(messages.len());
    let mut by_hash: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for mut msg in messages {
        if let Some(&idx) = by_hash.get(&msg.content_hash) {
            survivors_pass1[idx].hits += 1;
            result.exact_dropped += 1;
            // Mark the dropped message as a "drop" by leaving it out of survivors.
            let _ = msg; // drop
        } else {
            let idx = survivors_pass1.len();
            by_hash.insert(msg.content_hash, idx);
            // Ensure hits starts at 1 (unique). The adapter might have set
            // it to 0, in which case `+= 1` from the duplicate path would
            // be wrong; reset to 1 here.
            msg.hits = 1;
            survivors_pass1.push(msg);
        }
    }

    // --- Pass 2: Jaccard on token set ---
    // Compare each survivor to all earlier survivors; if Jaccard > threshold,
    // drop the later one and bump the earlier's hits.
    //
    // We collect hits increments into a separate Vec instead of mutating
    // `survivors_pass1` directly: the outer loop also needs to iterate
    // `survivors_pass1` to know which indices are still kept, so a
    // simultaneous mutable + immutable borrow would conflict.
    let token_cache: Vec<HashSet<String>> = survivors_pass1
        .iter()
        .map(|m| tokenize(&m.content))
        .collect();
    let n = survivors_pass1.len();
    let mut kept_indices: Vec<usize> = Vec::with_capacity(n);
    let mut extra_hits: Vec<(usize, u32)> = Vec::new();
    for i in 0..n {
        let candidate_tokens = &token_cache[i];
        let mut merged_into: Option<usize> = None;
        for &earlier in &kept_indices {
            let earlier_tokens = &token_cache[earlier];
            let sim = jaccard(candidate_tokens, earlier_tokens);
            if sim > threshold {
                merged_into = Some(earlier);
                break;
            }
        }
        if let Some(earlier) = merged_into {
            extra_hits.push((earlier, 1));
            result.fuzzy_dropped += 1;
        } else {
            kept_indices.push(i);
        }
    }

    // Apply the deferred hits increments.
    for (idx, delta) in extra_hits {
        survivors_pass1[idx].hits += delta;
    }

    result.survivors = kept_indices
        .into_iter()
        .map(|i| survivors_pass1[i].clone())
        .collect();
    // Note: clone preserves the input order, which is what the caller
    // expects (dedup is a fold, not a shuffle). Downstream `Pipeline::run`
    // re-sorts by `signal_score` DESC anyway.

    result
}

/// Tokenize for Jaccard: lowercase, split on non-alphanumeric, drop
/// empty tokens. Punctuation and casing are normalized away.
fn tokenize(content: &str) -> HashSet<String> {
    content
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0; // both empty = identical (edge case)
    }
    let intersection = a.intersection(b).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Role;
    use chrono::TimeZone;

    fn make_msg(id: &str, content: &str) -> NormalizedMessage {
        NormalizedMessage {
            id: id.into(),
            source: "test".into(),
            role: Role::User,
            content: content.into(),
            original_length: 0,
            created_at: chrono::Utc.timestamp_opt(1, 0).unwrap(),
            thread_id: "t".into(),
            thread_title: None,
            project_hint: None,
            content_hash: NormalizedMessage::compute_content_hash(content),
            hits: 1,
            signal_score: 0.0,
        }
    }

    #[test]
    fn exact_dedup_drops_identical_hashes_and_credits_hits() {
        let msgs = vec![
            make_msg("a", "hello world"),
            make_msg("b", "hello world"),
            make_msg("c", "hello world"),
        ];
        let r = dedup(msgs, 0.85);
        assert_eq!(r.survivors.len(), 1);
        assert_eq!(r.exact_dropped, 2);
        assert_eq!(r.survivors[0].hits, 3);
    }

    #[test]
    fn different_content_survives_exact_pass() {
        let msgs = vec![make_msg("a", "first thing"), make_msg("b", "second thing")];
        let r = dedup(msgs, 0.85);
        assert_eq!(r.survivors.len(), 2);
        assert_eq!(r.exact_dropped, 0);
    }

    #[test]
    fn fuzzy_dedup_collapses_near_duplicates() {
        let msgs = vec![
            make_msg("a", "How do I parse JSON in Rust?"),
            make_msg("b", "how do i parse json in rust"),
            make_msg("c", "What's the best Rust JSON library?"),
        ];
        let r = dedup(msgs, 0.85);
        // 'a' survives. 'b' is near-identical to 'a' (case + punctuation
        // normalize away) so jaccard ≈ 1.0, dropped. 'c' shares only
        // some tokens, survives.
        assert_eq!(r.survivors.len(), 2);
        assert_eq!(r.fuzzy_dropped, 1);
        // 'a' should have hits=2 now.
        let a_survived = r.survivors.iter().find(|m| m.id == "a").unwrap();
        assert_eq!(a_survived.hits, 2);
    }

    #[test]
    fn fuzzy_dedup_respects_threshold() {
        // Two messages with overlap ~0.5 should both survive at threshold 0.85
        // but collapse at threshold 0.3.
        let msgs = vec![
            make_msg("a", "the quick brown fox"),
            make_msg("b", "the quick brown dog"),
        ];
        let r_high = dedup(msgs.clone(), 0.85);
        assert_eq!(r_high.survivors.len(), 2);
        let r_low = dedup(msgs, 0.3);
        assert_eq!(r_low.survivors.len(), 1);
        assert_eq!(r_low.fuzzy_dropped, 1);
    }

    #[test]
    fn empty_intersection_handled_gracefully() {
        // Two completely disjoint token sets.
        let msgs = vec![
            make_msg("a", "alpha beta gamma"),
            make_msg("b", "delta epsilon zeta"),
        ];
        let r = dedup(msgs, 0.85);
        assert_eq!(r.survivors.len(), 2);
    }
}
