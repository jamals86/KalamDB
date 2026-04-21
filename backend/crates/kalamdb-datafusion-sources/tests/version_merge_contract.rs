//! Version-merge contract: the shared substrate must expose the ordering rule
//! used by every MVCC-backed provider family — `(commit_seq, seq_id)` with
//! commit_seq as the primary key and seq_id as the tiebreaker.
//!
//! The real merge execution plan lands in US3. This contract test pins the
//! ordering rule so the implementation cannot silently drift.

/// Pure ordering used by the upcoming merge exec. Kept inline here until the
/// shared merge helper lands; at that point this test will switch to importing
/// the shared function directly.
fn prefers_version(
    a: (u64 /* commit_seq */, u64 /* seq_id */),
    b: (u64, u64),
) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
}

#[test]
fn commit_seq_wins_over_seq_id() {
    use std::cmp::Ordering::*;
    assert_eq!(prefers_version((10, 1), (9, 999)), Greater);
    assert_eq!(prefers_version((5, 100), (5, 101)), Less);
    assert_eq!(prefers_version((5, 100), (5, 100)), Equal);
}

#[test]
fn ordering_is_total_and_transitive() {
    let samples = [(1, 1), (1, 2), (2, 0), (2, 5), (3, 1)];
    for &a in &samples {
        for &b in &samples {
            for &c in &samples {
                if prefers_version(a, b).is_lt() && prefers_version(b, c).is_lt() {
                    assert!(prefers_version(a, c).is_lt(), "transitivity broke for {a:?} < {b:?} < {c:?}");
                }
            }
        }
    }
}
