use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRange {
    pub start: u64,
    pub end_inclusive: u64,
}

impl NovoRudpRange {
    pub fn new(start: u64, end_inclusive: u64) -> Self {
        Self {
            start,
            end_inclusive,
        }
    }

    pub fn count(self) -> u64 {
        if self.end_inclusive < self.start {
            0
        } else {
            self.end_inclusive
                .saturating_sub(self.start)
                .saturating_add(1)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpWindowConfig {
    pub window_size: u64,
    pub packet_copies: u64,
    pub tail_packet_copies: u64,
    pub batch_size: u64,
    pub batch_pause_ms: u64,
    pub ack_wait_ms: u64,
    pub max_window_retries: u64,
    pub no_progress_backoff: bool,
}

impl Default for NovoRudpWindowConfig {
    fn default() -> Self {
        Self {
            window_size: 64,
            packet_copies: 2,
            tail_packet_copies: 3,
            batch_size: 16,
            batch_pause_ms: 10,
            ack_wait_ms: 1000,
            max_window_retries: 8,
            no_progress_backoff: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRepairWindow {
    pub window_id: u64,
    pub range: NovoRudpRange,
    pub missing_ranges: Vec<NovoRudpRange>,
    pub missing_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRepairPlan {
    pub window: NovoRudpRepairWindow,
    pub packet_copies: u64,
    pub batch_size: u64,
    pub batch_pause_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpAckProgress {
    Complete,
    Progress,
    NoProgress,
}

pub fn normalize_missing_ranges(ranges: &[NovoRudpRange], expected: u64) -> Vec<NovoRudpRange> {
    if expected == 0 {
        return Vec::new();
    }
    let max = expected.saturating_sub(1);
    let mut clipped = ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.min(max);
            let end = range.end_inclusive.min(max);
            (end >= start).then_some(NovoRudpRange::new(start, end))
        })
        .collect::<Vec<_>>();
    clipped.sort_by_key(|range| range.start);

    let mut merged = Vec::<NovoRudpRange>::new();
    for range in clipped {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end_inclusive.saturating_add(1) {
                last.end_inclusive = last.end_inclusive.max(range.end_inclusive);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

pub fn missing_count(ranges: &[NovoRudpRange]) -> u64 {
    ranges
        .iter()
        .fold(0u64, |total, range| total.saturating_add(range.count()))
}

pub fn select_first_missing_window(
    ranges: &[NovoRudpRange],
    expected: u64,
    config: &NovoRudpWindowConfig,
) -> Option<NovoRudpRepairWindow> {
    let window_size = config.window_size.max(1);
    let normalized = normalize_missing_ranges(ranges, expected);
    let first = normalized.first()?;
    let window_start = first.start;
    let window_end = window_start
        .saturating_add(window_size.saturating_sub(1))
        .min(expected.saturating_sub(1));
    let mut window_ranges = Vec::<NovoRudpRange>::new();
    for range in normalized {
        let start = range.start.max(window_start);
        let end = range.end_inclusive.min(window_end);
        if end >= start {
            window_ranges.push(NovoRudpRange::new(start, end));
        }
    }
    let missing_count = missing_count(window_ranges.as_slice());
    (missing_count > 0).then_some(NovoRudpRepairWindow {
        window_id: window_start / window_size,
        range: NovoRudpRange::new(window_start, window_end),
        missing_ranges: window_ranges,
        missing_count,
    })
}

pub fn build_repair_plan(
    ranges: &[NovoRudpRange],
    expected: u64,
    config: &NovoRudpWindowConfig,
) -> Option<NovoRudpRepairPlan> {
    let window = select_first_missing_window(ranges, expected, config)?;
    let tail_starts_at = expected.saturating_sub(config.window_size.max(1));
    let packet_copies = if window.range.start >= tail_starts_at {
        config.tail_packet_copies.max(config.packet_copies).max(1)
    } else {
        config.packet_copies.max(1)
    };
    Some(NovoRudpRepairPlan {
        window,
        packet_copies,
        batch_size: config.batch_size.max(1),
        batch_pause_ms: config.batch_pause_ms,
    })
}

pub fn classify_ack_progress(previous_missing: u64, current_missing: u64) -> NovoRudpAckProgress {
    if current_missing == 0 {
        NovoRudpAckProgress::Complete
    } else if current_missing < previous_missing {
        NovoRudpAckProgress::Progress
    } else {
        NovoRudpAckProgress::NoProgress
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_repair_plan, classify_ack_progress, normalize_missing_ranges,
        select_first_missing_window, NovoRudpAckProgress, NovoRudpRange, NovoRudpWindowConfig,
    };

    #[test]
    fn normalizes_and_merges_ranges() {
        let ranges = vec![
            NovoRudpRange::new(10, 12),
            NovoRudpRange::new(13, 20),
            NovoRudpRange::new(90, 120),
        ];
        let normalized = normalize_missing_ranges(ranges.as_slice(), 100);
        assert_eq!(
            normalized,
            vec![NovoRudpRange::new(10, 20), NovoRudpRange::new(90, 99)]
        );
    }

    #[test]
    fn selects_first_missing_window_only() {
        let config = NovoRudpWindowConfig {
            window_size: 64,
            ..NovoRudpWindowConfig::default()
        };
        let ranges = vec![NovoRudpRange::new(14112, 14399)];
        let window =
            select_first_missing_window(ranges.as_slice(), 14400, &config).expect("window");
        assert_eq!(window.window_id, 220);
        assert_eq!(window.range, NovoRudpRange::new(14112, 14175));
        assert_eq!(window.missing_count, 64);
    }

    #[test]
    fn tail_window_uses_tail_packet_copies() {
        let config = NovoRudpWindowConfig {
            window_size: 64,
            packet_copies: 2,
            tail_packet_copies: 6,
            ..NovoRudpWindowConfig::default()
        };
        let plan =
            build_repair_plan(&[NovoRudpRange::new(14360, 14399)], 14400, &config).expect("plan");
        assert_eq!(plan.packet_copies, 6);
    }

    #[test]
    fn ack_progress_classification_is_strict() {
        assert_eq!(classify_ack_progress(100, 0), NovoRudpAckProgress::Complete);
        assert_eq!(
            classify_ack_progress(100, 80),
            NovoRudpAckProgress::Progress
        );
        assert_eq!(
            classify_ack_progress(100, 100),
            NovoRudpAckProgress::NoProgress
        );
    }
}
