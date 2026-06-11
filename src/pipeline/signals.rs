//! Composite signal scoring. Default formula:
//!
//! ```text
//! signal_score = 0.4 * hits_norm + 0.3 * recency + 0.3 * type_weight
//! ```
//!
//! - **`hits_norm`**: `min(hits, 10) / 10.0` — saturates at 10 occurrences.
//!   Reflects "this memory was useful enough to repeat".
//! - **`recency`**: `exp(-age_days / 365.0)` — exponential decay with a
//!   1-year half-life-ish. A message from today ≈ 1.0; 1 year old ≈ 0.37;
//!   3 years old ≈ 0.05. Older than 3 years is filtered by the age gate
//!   before this runs anyway.
//! - **`type_weight`**: role-based heuristic. Assistant answers carry the
//!   most semantic value; user questions are the seed; system prompts are
//!   boilerplate; tool messages are usually noise.

use crate::pipeline::{NormalizedMessage, Role};
use chrono::{DateTime, Utc};

const HITS_SATURATION: u32 = 10;
const RECENCY_HALF_LIFE_DAYS: f64 = 365.0;
const WEIGHT_HITS: f32 = 0.4;
const WEIGHT_RECENCY: f32 = 0.3;
const WEIGHT_TYPE: f32 = 0.3;

fn type_weight(role: Role) -> f32 {
    match role {
        Role::Assistant => 1.0,
        Role::User => 0.8,
        Role::Tool => 0.5,
        Role::System => 0.3,
    }
}

fn recency(created_at: DateTime<Utc>, now: DateTime<Utc>) -> f32 {
    let age_days = (now - created_at).num_days().max(0) as f64;
    let decay = -age_days / RECENCY_HALF_LIFE_DAYS;
    decay.exp().clamp(0.0, 1.0) as f32
}

fn hits_normalized(hits: u32) -> f32 {
    let h = hits.min(HITS_SATURATION) as f32;
    h / HITS_SATURATION as f32
}

/// Compute the composite `signal_score` and write it back to the message.
/// The `now` parameter is explicit (not `Utc::now()`) so tests are
/// deterministic.
pub fn score(msg: &mut NormalizedMessage, now: DateTime<Utc>) {
    let h = hits_normalized(msg.hits);
    let r = recency(msg.created_at, now);
    let t = type_weight(msg.role);
    msg.signal_score = WEIGHT_HITS * h + WEIGHT_RECENCY * r + WEIGHT_TYPE * t;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Role;
    use chrono::{Duration, TimeZone};

    fn make_msg(role: Role, hits: u32, days_ago: i64) -> NormalizedMessage {
        let now = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        NormalizedMessage {
            id: "m".into(),
            source: "test".into(),
            role,
            content: "x".into(),
            original_length: 0,
            created_at: now - Duration::days(days_ago),
            thread_id: "t".into(),
            thread_title: None,
            project_hint: None,
            content_hash: 0,
            hits,
            signal_score: 0.0,
        }
    }

    fn now() -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn type_weight_orders_assistant_above_user_above_tool_above_system() {
        let a = type_weight(Role::Assistant);
        let u = type_weight(Role::User);
        let tl = type_weight(Role::Tool);
        let s = type_weight(Role::System);
        assert!(a > u, "assistant > user");
        assert!(u > tl, "user > tool");
        assert!(tl > s, "tool > system");
    }

    #[test]
    fn recency_decays_with_age() {
        let n = now();
        assert!((recency(n, n) - 1.0).abs() < 1e-6);
        // ~1 year old → ~1/e
        let year_old = n - Duration::days(365);
        let r = recency(year_old, n);
        assert!((r - 1.0 / std::f32::consts::E).abs() < 0.01);
        // Older = smaller
        let old = n - Duration::days(730);
        assert!(recency(old, n) < r);
    }

    #[test]
    fn recency_clamps_to_zero_for_future_dates() {
        // Negative ages (clock skew) clamp to 0.0
        let now = now();
        let future = now + Duration::days(30);
        // age_days = max(0, -30) = 0 → recency = exp(0) = 1.0
        assert!((recency(future, now) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hits_normalized_saturates_at_10() {
        assert!((hits_normalized(0) - 0.0).abs() < 1e-6);
        assert!((hits_normalized(1) - 0.1).abs() < 1e-6);
        assert!((hits_normalized(5) - 0.5).abs() < 1e-6);
        assert!((hits_normalized(10) - 1.0).abs() < 1e-6);
        assert!((hits_normalized(100) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_combines_three_terms() {
        let now = now();
        let mut msg = make_msg(Role::Assistant, 5, 0);
        score(&mut msg, now);
        // 0.4*0.5 + 0.3*1.0 + 0.3*1.0 = 0.2 + 0.3 + 0.3 = 0.8
        assert!((msg.signal_score - 0.8).abs() < 0.01);
    }

    #[test]
    fn score_low_signal_for_old_system_message() {
        let now = now();
        let mut msg = make_msg(Role::System, 1, 1000);
        score(&mut msg, now);
        // hits=0.1, recency=exp(-1000/365)≈0.064, type=0.3
        // 0.4*0.1 + 0.3*0.064 + 0.3*0.3 = 0.04 + 0.019 + 0.09 ≈ 0.149
        assert!(
            msg.signal_score < 0.2,
            "old system messages should be low signal"
        );
    }
}
