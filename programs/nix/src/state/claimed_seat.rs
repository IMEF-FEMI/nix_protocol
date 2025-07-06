use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

use shank::ShankType;
use solana_program::pubkey::Pubkey;
use static_assertions::const_assert_eq;
use std::cmp::Ordering;
use crate::quantities::WrappedI80F48;

// use crate::quantities::WrappedI80F48;
use super::constants::CLAIMED_SEAT_SIZE;

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, Zeroable, Pod, ShankType)]
pub struct ClaimedSeat {
    pub trader: Pubkey,
    // asset_share are withdrawable on the underlying protocol (marginfi). They do not include funds in
    // open orders. When moving funds over to open orders, use the worst case
    // rounding.
    pub base_a_withdrawable_asset_share: WrappedI80F48,
    pub base_b_withdrawable_asset_share: WrappedI80F48,
    /// volumes traded over lifetime, can overflow. Double counts self
    /// trades. This is for informational and monitoring purposes only. This is
    /// not guaranteed to be maintained. It does not secure any value in
    /// nix. Use at your own risk.
    pub base_a_volume: WrappedI80F48,
    pub base_b_volume: WrappedI80F48,
}
// 32 + // trader
//  8 + // base_asset_share
//  8 + // quote_asset_share
//  8 + // quote_volume
//  8   // padding
// = 64
const_assert_eq!(size_of::<ClaimedSeat>(), CLAIMED_SEAT_SIZE);
const_assert_eq!(size_of::<ClaimedSeat>() % 8, 0);

impl ClaimedSeat {
    pub fn new_empty(trader: Pubkey) -> Self {
        ClaimedSeat {
            trader,
            ..Default::default()
        }
    }
}

impl Ord for ClaimedSeat {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.trader).cmp(&(other.trader))
    }
}

impl PartialOrd for ClaimedSeat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ClaimedSeat {
    fn eq(&self, other: &Self) -> bool {
        (self.trader) == (other.trader)
    }
}

impl Eq for ClaimedSeat {}

impl std::fmt::Display for ClaimedSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.trader)
    }
}

#[test]
fn test_display() {
    let claimed_seat: ClaimedSeat = ClaimedSeat::new_empty(Pubkey::default());
    assert_eq!(claimed_seat.trader, Pubkey::default());
}
