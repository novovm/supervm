pub(super) const MAX_FUTURE_BLOCK_TIME_MS: u64 = 30_000;

pub(super) fn timestamp_allowed(timestamp_ms: u64, wall_ms: u64) -> bool {
    timestamp_ms.saturating_sub(wall_ms) <= MAX_FUTURE_BLOCK_TIME_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_timestamp_boundary_and_clock_rollback() {
        let wall_ms = 1_700_000_000_000;
        assert!(timestamp_allowed(wall_ms, wall_ms));
        assert!(timestamp_allowed(
            wall_ms + MAX_FUTURE_BLOCK_TIME_MS,
            wall_ms
        ));
        assert!(!timestamp_allowed(
            wall_ms + MAX_FUTURE_BLOCK_TIME_MS + 1,
            wall_ms
        ));
        assert!(!timestamp_allowed(
            wall_ms,
            wall_ms - MAX_FUTURE_BLOCK_TIME_MS - 1
        ));
        assert!(timestamp_allowed(
            wall_ms,
            wall_ms - MAX_FUTURE_BLOCK_TIME_MS
        ));
    }

    #[test]
    fn historical_blocks_and_extreme_clock_values() {
        assert!(timestamp_allowed(1, 1_700_000_000_000));
        assert!(timestamp_allowed(0, u64::MAX));
        assert!(!timestamp_allowed(u64::MAX, 0));
        assert!(timestamp_allowed(u64::MAX, u64::MAX));
        assert!(timestamp_allowed(
            u64::MAX,
            u64::MAX - MAX_FUTURE_BLOCK_TIME_MS
        ));
    }
}
