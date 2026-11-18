//! Constant product (x * y = k) math used by the pool.
//!
//! Everything is done in u128 so `reserve * amount` can never overflow for
//! u64 inputs. Rounding always favours the pool: LPs pay rounded up when they
//! deposit, get rounded down when they withdraw, and swappers get the output
//! rounded down.

use crate::error::AmmError;

pub const BPS_DENOMINATOR: u128 = 10_000;

/// (x, y) a depositor has to put in to mint `lp_amount` new LP tokens.
pub fn deposit_amounts(
    reserve_x: u64,
    reserve_y: u64,
    lp_supply: u64,
    lp_amount: u64,
) -> Result<(u64, u64), AmmError> {
    if lp_supply == 0 {
        return Err(AmmError::NoLiquidityInPool);
    }
    let x = mul_div_ceil(reserve_x, lp_amount, lp_supply)?;
    let y = mul_div_ceil(reserve_y, lp_amount, lp_supply)?;
    Ok((x, y))
}

/// (x, y) returned for burning `lp_amount` LP tokens.
pub fn withdraw_amounts(
    reserve_x: u64,
    reserve_y: u64,
    lp_supply: u64,
    lp_amount: u64,
) -> Result<(u64, u64), AmmError> {
    if lp_supply == 0 {
        return Err(AmmError::NoLiquidityInPool);
    }
    if lp_amount > lp_supply {
        return Err(AmmError::InsufficientBalance);
    }
    let x = mul_div_floor(reserve_x, lp_amount, lp_supply)?;
    let y = mul_div_floor(reserve_y, lp_amount, lp_supply)?;
    Ok((x, y))
}

/// Output amount for swapping `amount_in` into the pool.
///
/// The LP fee is taken off the input before it hits the curve, but the whole
/// `amount_in` still ends up in the vault, so k grows a little on every trade.
///
///   in_after_fee = amount_in * (10_000 - fee_bps) / 10_000
///   amount_out   = reserve_out * in_after_fee / (reserve_in + in_after_fee)
pub fn swap_amount_out(
    reserve_in: u64,
    reserve_out: u64,
    amount_in: u64,
    fee_bps: u16,
) -> Result<u64, AmmError> {
    if reserve_in == 0 || reserve_out == 0 {
        return Err(AmmError::NoLiquidityInPool);
    }
    let fee_bps = fee_bps as u128;
    if fee_bps >= BPS_DENOMINATOR {
        return Err(AmmError::InvalidFee);
    }

    let in_after_fee = (amount_in as u128)
        .checked_mul(BPS_DENOMINATOR - fee_bps)
        .ok_or(AmmError::Overflow)?
        / BPS_DENOMINATOR;

    let numerator = (reserve_out as u128)
        .checked_mul(in_after_fee)
        .ok_or(AmmError::Overflow)?;
    let denominator = (reserve_in as u128)
        .checked_add(in_after_fee)
        .ok_or(AmmError::Overflow)?;

    u64::try_from(numerator / denominator).map_err(|_| AmmError::Overflow)
}

/// Basis point cut of `amount`, rounded down.
pub fn bps_of(amount: u64, bps: u16) -> Result<u64, AmmError> {
    let cut = (amount as u128)
        .checked_mul(bps as u128)
        .ok_or(AmmError::Overflow)?
        / BPS_DENOMINATOR;
    u64::try_from(cut).map_err(|_| AmmError::Overflow)
}

fn mul_div_floor(a: u64, b: u64, c: u64) -> Result<u64, AmmError> {
    let res = (a as u128)
        .checked_mul(b as u128)
        .ok_or(AmmError::Overflow)?
        .checked_div(c as u128)
        .ok_or(AmmError::Overflow)?;
    u64::try_from(res).map_err(|_| AmmError::Overflow)
}

fn mul_div_ceil(a: u64, b: u64, c: u64) -> Result<u64, AmmError> {
    let c = c as u128;
    if c == 0 {
        return Err(AmmError::Overflow);
    }
    let prod = (a as u128)
        .checked_mul(b as u128)
        .ok_or(AmmError::Overflow)?;
    let res = prod.div_ceil(c);
    u64::try_from(res).map_err(|_| AmmError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_is_proportional_and_rounds_up() {
        // pool 1000 x / 2000 y / 1000 lp, mint 10% more lp
        assert_eq!(
            deposit_amounts(1_000, 2_000, 1_000, 100).unwrap(),
            (100, 200)
        );
        // 1 lp out of 3 supply on 10 x -> 3.33 rounds up to 4
        assert_eq!(deposit_amounts(10, 10, 3, 1).unwrap(), (4, 4));
    }

    #[test]
    fn withdraw_is_proportional_and_rounds_down() {
        assert_eq!(
            withdraw_amounts(1_000, 2_000, 1_000, 250).unwrap(),
            (250, 500)
        );
        assert_eq!(withdraw_amounts(10, 10, 3, 1).unwrap(), (3, 3));
        assert_eq!(withdraw_amounts(10, 10, 3, 3).unwrap(), (10, 10));
    }

    #[test]
    fn withdraw_more_than_supply_fails() {
        assert!(matches!(
            withdraw_amounts(10, 10, 3, 4),
            Err(AmmError::InsufficientBalance)
        ));
    }

    #[test]
    fn empty_pool_errors() {
        assert!(matches!(
            deposit_amounts(0, 0, 0, 1),
            Err(AmmError::NoLiquidityInPool)
        ));
        assert!(matches!(
            swap_amount_out(0, 100, 1, 30),
            Err(AmmError::NoLiquidityInPool)
        ));
    }

    #[test]
    fn swap_without_fee_keeps_k() {
        // 100 * 100 = 10_000, put in 100 -> new x = 200, new y = 50
        assert_eq!(swap_amount_out(100, 100, 100, 0).unwrap(), 50);
    }

    #[test]
    fn swap_with_fee_grows_k() {
        let (x, y) = (1_000_000u64, 1_000_000u64);
        let amount_in = 10_000;
        let out = swap_amount_out(x, y, amount_in, 30).unwrap();
        // 10_000 * 0.997 = 9_970 effective -> 1e6 * 9970 / 1_009_970 = 9871.58
        assert_eq!(out, 9_871);
        let k_before = x as u128 * y as u128;
        let k_after = (x + amount_in) as u128 * (y - out) as u128;
        assert!(k_after > k_before);
    }

    #[test]
    fn fee_reduces_output() {
        let no_fee = swap_amount_out(1_000_000, 1_000_000, 10_000, 0).unwrap();
        let with_fee = swap_amount_out(1_000_000, 1_000_000, 10_000, 30).unwrap();
        assert!(with_fee < no_fee);
    }

    #[test]
    fn swap_rejects_full_fee() {
        assert!(matches!(
            swap_amount_out(10, 10, 1, 10_000),
            Err(AmmError::InvalidFee)
        ));
    }

    #[test]
    fn swap_never_drains_reserve() {
        let out = swap_amount_out(1, 1_000_000, u64::MAX, 0).unwrap();
        assert!(out < 1_000_000);
    }

    #[test]
    fn bps_cut() {
        assert_eq!(bps_of(1_000_000, 5).unwrap(), 500);
        assert_eq!(bps_of(199, 50).unwrap(), 0);
    }
}
