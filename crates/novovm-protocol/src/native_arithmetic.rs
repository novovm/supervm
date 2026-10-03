//! Versioned native arithmetic shared by Host execution and future proof guests.
//! No storage, clock, environment, JSON, signatures or authority decisions.
//! V1 preserves existing saturating arithmetic, including extreme-value behavior.
//! These functions alone do NOT validate a transaction or authenticate state.

pub fn credit_balance_v1(balance: u128, amount: u128) -> u128 {
    balance.saturating_add(amount)
}

/// None means insufficient balance. The caller retains its error/context policy.
pub fn debit_balance_v1(balance: u128, amount: u128) -> Option<u128> {
    balance.checked_sub(amount)
}

pub fn amm_output_for_exact_input_v1(
    reserve_in: u128,
    reserve_out: u128,
    amount_in: u128,
    fee_ppm: u32,
) -> Option<u128> {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
        return None;
    }
    let fee_den = 1_000_000u128;
    let amount_in_after_fee =
        amount_in.saturating_mul(fee_den.saturating_sub(u128::from(fee_ppm))) / fee_den;
    if amount_in_after_fee == 0 {
        return None;
    }
    let numerator = amount_in_after_fee.saturating_mul(reserve_out);
    let denominator = reserve_in.saturating_add(amount_in_after_fee);
    if denominator == 0 {
        return None;
    }
    Some(numerator / denominator)
}

/// Apply an already selected/validated swap to the pool reserves (x, y).
/// Not a quote, balance check, slippage check, or permission to execute a swap.
pub fn amm_reserves_after_swap_v1(
    reserve_x: u128,
    reserve_y: u128,
    amount_in: u128,
    amount_out: u128,
    reversed: bool,
) -> (u128, u128) {
    if reversed {
        (
            reserve_x.saturating_sub(amount_out),
            reserve_y.saturating_add(amount_in),
        )
    } else {
        (
            reserve_x.saturating_add(amount_in),
            reserve_y.saturating_sub(amount_out),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Frozen pre-extraction implementation, test-only: never a production route.
    fn legacy_quote(ri: u128, ro: u128, amount: u128, fee: u32) -> Option<u128> {
        if ri == 0 || ro == 0 || amount == 0 {
            return None;
        }
        let net = amount.saturating_mul(1_000_000u128.saturating_sub(u128::from(fee))) / 1_000_000;
        if net == 0 {
            return None;
        }
        let numerator = net.saturating_mul(ro);
        let denominator = ri.saturating_add(net);
        if denominator == 0 {
            return None;
        }
        Some(numerator / denominator)
    }

    const VALUES: &[u128] = &[
        0,
        1,
        2,
        99,
        100,
        999_999,
        1_000_000,
        u64::MAX as u128,
        u128::MAX / 1_000_000,
        u128::MAX / 2,
        u128::MAX - 1,
        u128::MAX,
    ];

    #[test]
    fn native_arithmetic_quote_matches_legacy_boundary_matrix() {
        for &ri in VALUES {
            for &ro in VALUES {
                for &amount in VALUES {
                    for fee in [0, 1, 3_000, 999_999, 1_000_000, 1_000_001, u32::MAX] {
                        assert_eq!(
                            amm_output_for_exact_input_v1(ri, ro, amount, fee),
                            legacy_quote(ri, ro, amount, fee)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn native_arithmetic_known_quotes_and_zero_are_preserved() {
        assert_eq!(
            amm_output_for_exact_input_v1(1_000_000, 1_000_000, 100, 3_000),
            Some(98)
        );
        assert_eq!(
            amm_output_for_exact_input_v1(1_000, 1_000, 100, 0),
            Some(90)
        );
        // Some(0) is not None: the host has distinct rejection messages.
        assert_eq!(amm_output_for_exact_input_v1(u128::MAX, 1, 1, 0), Some(0));
        assert_eq!(amm_output_for_exact_input_v1(1, 1, 1, 3_000), None);
        assert_eq!(
            amm_output_for_exact_input_v1(1, 1, u128::MAX, 1_000_001),
            None
        );
    }

    #[test]
    fn native_arithmetic_balances_match_legacy() {
        for &balance in VALUES {
            for &amount in VALUES {
                assert_eq!(
                    credit_balance_v1(balance, amount),
                    balance.saturating_add(amount)
                );
                let before = if balance < amount {
                    None
                } else {
                    Some(balance.saturating_sub(amount))
                };
                assert_eq!(debit_balance_v1(balance, amount), before);
            }
        }
    }

    #[test]
    fn native_arithmetic_pool_updates_match_both_legacy_directions() {
        for &x in VALUES {
            for &y in VALUES {
                for &input in VALUES {
                    for &output in VALUES {
                        for reversed in [false, true] {
                            let (mut old_x, mut old_y) = (x, y);
                            if reversed {
                                old_y = old_y.saturating_add(input);
                                old_x = old_x.saturating_sub(output);
                            } else {
                                old_x = old_x.saturating_add(input);
                                old_y = old_y.saturating_sub(output);
                            }
                            assert_eq!(
                                amm_reserves_after_swap_v1(x, y, input, output, reversed),
                                (old_x, old_y)
                            );
                        }
                    }
                }
            }
        }
    }
}
