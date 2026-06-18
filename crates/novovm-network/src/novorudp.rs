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
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Debug, Clone)]
    struct HarnessAck {
        epoch: u64,
        receiver_done: bool,
        missing_count: u64,
        current_window: Option<NovoRudpRange>,
        current_window_missing_ranges: Vec<NovoRudpRange>,
    }

    #[derive(Debug)]
    struct HarnessReceiver {
        expected: u64,
        window_size: u64,
        epoch: u64,
        received: BTreeSet<u64>,
        payload_available: BTreeSet<u64>,
        active_pending: BTreeSet<u64>,
        receipt: BTreeSet<u64>,
        admitted: BTreeSet<u64>,
    }

    impl HarnessReceiver {
        fn new(expected: u64, received_until_exclusive: u64, window_size: u64) -> Self {
            let mut received = BTreeSet::new();
            let mut receipt = BTreeSet::new();
            for sequence in 0..received_until_exclusive.min(expected) {
                received.insert(sequence);
                receipt.insert(sequence);
            }
            Self {
                expected,
                window_size,
                epoch: 0,
                received,
                payload_available: BTreeSet::new(),
                active_pending: BTreeSet::new(),
                receipt,
                admitted: BTreeSet::new(),
            }
        }

        fn missing_ranges(&self) -> Vec<NovoRudpRange> {
            let mut ranges = Vec::<NovoRudpRange>::new();
            let mut start = None::<u64>;
            let mut previous = None::<u64>;
            for sequence in 0..self.expected {
                if self.receipt.contains(&sequence) {
                    continue;
                }
                match (start, previous) {
                    (Some(_), Some(prev)) if sequence == prev.saturating_add(1) => {
                        previous = Some(sequence);
                    }
                    (Some(left), Some(prev)) => {
                        ranges.push(NovoRudpRange::new(left, prev));
                        start = Some(sequence);
                        previous = Some(sequence);
                    }
                    _ => {
                        start = Some(sequence);
                        previous = Some(sequence);
                    }
                }
            }
            if let (Some(left), Some(right)) = (start, previous) {
                ranges.push(NovoRudpRange::new(left, right));
            }
            ranges
        }

        fn ack(&mut self) -> HarnessAck {
            self.epoch = self.epoch.saturating_add(1);
            let ranges = self.missing_ranges();
            let missing_count = super::missing_count(ranges.as_slice());
            let window = select_first_missing_window(
                ranges.as_slice(),
                self.expected,
                &NovoRudpWindowConfig {
                    window_size: self.window_size,
                    ..NovoRudpWindowConfig::default()
                },
            );
            HarnessAck {
                epoch: self.epoch,
                receiver_done: missing_count == 0,
                missing_count,
                current_window: window.as_ref().map(|window| window.range),
                current_window_missing_ranges: window
                    .map(|window| window.missing_ranges)
                    .unwrap_or_default(),
            }
        }

        fn receive_repair(&mut self, sequence: u64) {
            if sequence >= self.expected || self.receipt.contains(&sequence) {
                return;
            }
            self.received.insert(sequence);
            self.payload_available.insert(sequence);
            self.active_pending.insert(sequence);
        }

        fn admit_active_pending(&mut self, limit: usize) {
            let candidates = self
                .active_pending
                .iter()
                .copied()
                .take(limit)
                .collect::<Vec<_>>();
            for sequence in candidates {
                self.active_pending.remove(&sequence);
                self.admitted.insert(sequence);
                self.receipt.insert(sequence);
            }
        }

        fn cleanup_active_pending(&mut self) {
            self.active_pending.clear();
        }

        fn requeue_payload_without_receipt(&mut self) {
            for sequence in self.payload_available.iter().copied().collect::<Vec<_>>() {
                if !self.receipt.contains(&sequence) {
                    self.active_pending.insert(sequence);
                }
            }
        }
    }

    #[derive(Debug)]
    struct HarnessSender {
        latest_epoch: u64,
        current_window: Option<NovoRudpRange>,
        stale_ack_rejected: u64,
        sent_sequences: Vec<u64>,
    }

    impl HarnessSender {
        fn new() -> Self {
            Self {
                latest_epoch: 0,
                current_window: None,
                stale_ack_rejected: 0,
                sent_sequences: Vec::new(),
            }
        }

        fn repair_from_ack(&mut self, ack: &HarnessAck) -> Vec<u64> {
            if ack.epoch <= self.latest_epoch {
                self.stale_ack_rejected = self.stale_ack_rejected.saturating_add(1);
                return Vec::new();
            }
            self.latest_epoch = ack.epoch;
            if ack.receiver_done || ack.missing_count == 0 {
                self.current_window = None;
                return Vec::new();
            }
            self.current_window = ack.current_window;
            let mut repair = Vec::<u64>::new();
            for range in &ack.current_window_missing_ranges {
                for sequence in range.start..=range.end_inclusive {
                    repair.push(sequence);
                }
            }
            self.sent_sequences.extend(repair.iter().copied());
            repair
        }
    }

    #[derive(Debug)]
    struct LossyHarnessChannel {
        drops_remaining: BTreeMap<u64, u64>,
    }

    impl LossyHarnessChannel {
        fn with_tail_loss(start: u64, end_inclusive: u64, drops_per_sequence: u64) -> Self {
            let mut drops_remaining = BTreeMap::new();
            for sequence in start..=end_inclusive {
                drops_remaining.insert(sequence, drops_per_sequence);
            }
            Self { drops_remaining }
        }

        fn deliver(&mut self, sequences: &[u64], receiver: &mut HarnessReceiver) {
            for sequence in sequences {
                let remaining = self.drops_remaining.entry(*sequence).or_insert(0);
                if *remaining > 0 {
                    *remaining = remaining.saturating_sub(1);
                    continue;
                }
                receiver.receive_repair(*sequence);
            }
        }
    }

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

    #[test]
    fn novorudp_window_state_machine_converges_with_tail_loss() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = HarnessSender::new();
        let mut channel = LossyHarnessChannel::with_tail_loss(14_152, 14_399, 2);

        for _ in 0..128 {
            let ack = receiver.ack();
            if ack.receiver_done {
                break;
            }
            let repair = sender.repair_from_ack(&ack);
            channel.deliver(repair.as_slice(), &mut receiver);
            receiver.admit_active_pending(16);
        }
        while !receiver.active_pending.is_empty() {
            receiver.admit_active_pending(16);
        }

        let final_ack = receiver.ack();
        assert!(final_ack.receiver_done);
        assert_eq!(final_ack.missing_count, 0);
        assert_eq!(receiver.receipt.len() as u64, 14_400);
    }

    #[test]
    fn novorudp_window_state_machine_rejects_stale_ack_progress() {
        let mut sender = HarnessSender::new();
        let fresh_ack = HarnessAck {
            epoch: 10,
            receiver_done: false,
            missing_count: 64,
            current_window: Some(NovoRudpRange::new(14_152, 14_215)),
            current_window_missing_ranges: vec![NovoRudpRange::new(14_152, 14_215)],
        };
        let stale_ack = HarnessAck {
            epoch: 9,
            receiver_done: false,
            missing_count: 10_648,
            current_window: Some(NovoRudpRange::new(3_752, 3_815)),
            current_window_missing_ranges: vec![NovoRudpRange::new(3_752, 3_815)],
        };

        assert_eq!(sender.repair_from_ack(&fresh_ack).len(), 64);
        assert!(sender.repair_from_ack(&stale_ack).is_empty());
        assert_eq!(sender.stale_ack_rejected, 1);
        assert_eq!(
            sender.current_window,
            Some(NovoRudpRange::new(14_152, 14_215))
        );
    }

    #[test]
    fn novorudp_window_state_machine_does_not_advance_until_window_bitmap_zero() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = HarnessSender::new();
        let ack = receiver.ack();
        let first_repair = sender.repair_from_ack(&ack);
        for sequence in first_repair.iter().copied().take(32) {
            receiver.receive_repair(sequence);
        }
        receiver.admit_active_pending(64);

        let second_ack = receiver.ack();
        assert_eq!(
            second_ack.current_window,
            Some(NovoRudpRange::new(14_184, 14_247)),
            "receiver owns the next first-missing window after only partial delivery"
        );
        assert!(second_ack.missing_count > 0);
        assert!(!second_ack.receiver_done);
    }

    #[test]
    fn novorudp_window_state_machine_recovers_payload_after_cleanup() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        receiver.receive_repair(14_152);
        receiver.cleanup_active_pending();
        assert!(receiver.active_pending.is_empty());
        assert!(receiver.payload_available.contains(&14_152));
        assert!(!receiver.receipt.contains(&14_152));

        receiver.requeue_payload_without_receipt();
        assert!(receiver.active_pending.contains(&14_152));
        receiver.admit_active_pending(1);
        assert!(receiver.receipt.contains(&14_152));
    }

    #[test]
    fn novorudp_window_state_machine_completes_14152_14399_tail_gap() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = HarnessSender::new();
        let mut channel = LossyHarnessChannel::with_tail_loss(14_152, 14_399, 1);

        for _ in 0..96 {
            let ack = receiver.ack();
            if ack.receiver_done {
                break;
            }
            let repair = sender.repair_from_ack(&ack);
            assert!(
                repair.iter().all(|seq| ack
                    .current_window_missing_ranges
                    .iter()
                    .any(|range| *seq >= range.start && *seq <= range.end_inclusive)),
                "sender must only repair receiver-owned current window"
            );
            channel.deliver(repair.as_slice(), &mut receiver);
            receiver.admit_active_pending(8);
            receiver.requeue_payload_without_receipt();
        }
        while !receiver.active_pending.is_empty() {
            receiver.admit_active_pending(8);
        }

        let final_ack = receiver.ack();
        assert_eq!(final_ack.missing_count, 0);
        assert!(final_ack.receiver_done);
        assert_eq!(receiver.receipt.len(), 14_400);
    }
}
