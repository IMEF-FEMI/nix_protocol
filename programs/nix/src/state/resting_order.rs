use std::mem::size_of;

use std::cmp::Ordering;

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use fixed::types::I80F48;
use hypertree::{DataIndex, PodBool};
use marginfi::state::marginfi_group::Bank;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use shank::ShankType;
use solana_program::{entrypoint::ProgramResult, program_error::ProgramError};
use static_assertions::const_assert_eq;

use crate::{
    marginfi_utils::{
        convert_asset_shares_to_tokens, convert_tokens_to_asset_shares,
        convert_tokens_to_liability_shares, get_token_amount_to_repay_liability_shares,
    },
    quantities::WrappedI80F48,
};

use super::{constants::NO_EXPIRATION_LAST_VALID_SLOT, RESTING_ORDER_SIZE};

pub fn order_type_can_rest(order_type: OrderType) -> bool {
    order_type != OrderType::ImmediateOrCancel
}

pub fn order_type_can_take(order_type: OrderType) -> bool {
    order_type != OrderType::PostOnly && order_type != OrderType::Global
}
#[derive(
    Debug,
    BorshDeserialize,
    BorshSerialize,
    PartialEq,
    Clone,
    Copy,
    ShankType,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[repr(u8)]
pub enum OrderType {
    // Normal limit order.
    Limit = 0,

    // Does not rest. Take only.
    ImmediateOrCancel = 1,

    // Fails if would cross the orderbook.
    PostOnly = 2,

    // Global orders are post only but use funds from the global account.
    Global = 3,

    // Reverse orders behave like an AMM. When filled, they place an order on
    // the other side of the book with a small fee (spread).
    Reverse = 4,

    // P2P2Pool orders are like reverse orders but they are only placed when a p2p match is made.
    P2P2Pool = 5,
}
unsafe impl bytemuck::Zeroable for OrderType {}
unsafe impl bytemuck::Pod for OrderType {}
impl Default for OrderType {
    fn default() -> Self {
        OrderType::Limit
    }
}
#[repr(C)]
#[derive(Default, Debug, Copy, Clone, Zeroable, Pod, ShankType)]
pub struct RestingOrder {
    //collateral_shares represents:
    //collateral for bids
    //liquidity for asks
    collateral_shares: WrappedI80F48,
    liability_shares: WrappedI80F48,
    rate_bps: u16,
    padding: [u8; 6],
    sequence_number: u64,
    trader_index: DataIndex,
    last_valid_slot: u32,

    order_type: OrderType,
    is_bid: PodBool,
    is_a_tree: PodBool,
    padding1: [u8; 5],
    // // Spread for reverse orders. Defaults to zero.
    reverse_spread: u16,
    padding2: [u8; 30],
}

// bid(borrower)  asset_shares(collateral), liability_shares amount
// ask(lender) need: asset_shares,
const_assert_eq!(size_of::<RestingOrder>(), RESTING_ORDER_SIZE);
const_assert_eq!(size_of::<RestingOrder>() % 8, 0);

impl RestingOrder {
    pub fn new(
        rate_bps: u16,
        sequence_number: u64,
        collateral_shares: WrappedI80F48,
        liability_shares: WrappedI80F48,
        is_a_tree: bool,
        trader_index: DataIndex,
        last_valid_slot: u32,
        order_type: OrderType,
        is_bid: bool,
        reverse_spread: u16,
    ) -> Result<Self, ProgramError> {
        // Reverse orders cannot have expiration.
        assert!(
            !(order_type == OrderType::Reverse && last_valid_slot != NO_EXPIRATION_LAST_VALID_SLOT)
        );

        Ok(RestingOrder {
            rate_bps,
            collateral_shares,
            liability_shares,
            sequence_number,
            trader_index,
            last_valid_slot,
            is_bid: PodBool::from_bool(is_bid),
            is_a_tree: PodBool::from_bool(is_a_tree),
            order_type,
            reverse_spread,
            padding: Default::default(),
            padding1: Default::default(),
            padding2: Default::default(),
        })
    }

    pub fn get_collateral_shares(&self) -> WrappedI80F48 {
        self.collateral_shares
    }
    pub fn get_liability_shares(&self) -> WrappedI80F48 {
        self.liability_shares
    }
    pub fn is_expired(&self, current_slot: u32) -> bool {
        self.last_valid_slot != NO_EXPIRATION_LAST_VALID_SLOT && self.last_valid_slot < current_slot
    }

    pub fn get_is_bid(&self) -> bool {
        self.is_bid.0 == 1
    }
    pub fn get_rate_bps(&self) -> u16 {
        self.rate_bps
    }
    pub fn is_global(&self) -> bool {
        self.order_type == OrderType::Global
    }
    pub fn get_trader_index(&self) -> DataIndex {
        self.trader_index
    }
    pub fn get_sequence_number(&self) -> u64 {
        self.sequence_number
    }

    pub fn is_reverse(&self) -> bool {
        self.order_type == OrderType::Reverse
    }

    pub fn get_reverse_spread(self) -> u16 {
        self.reverse_spread
    }
    pub fn get_num_base_atoms(&self, base_bank: &Bank) -> Result<u64, ProgramError> {
        if self.get_is_bid() {
            //convert liability shares to asset tokens
            get_token_amount_to_repay_liability_shares(
                self.liability_shares.into(),
                base_bank,
            )
        } else {
            convert_asset_shares_to_tokens(self.collateral_shares.into(), base_bank)
        }
    }

    pub fn get_num_base_atoms_global(&self) -> WrappedI80F48 {
        self.collateral_shares
    }
    pub fn set_order_type(&mut self, order_type: OrderType) {
        self.order_type = order_type;
    }

    pub fn reduce_bid(
        &mut self,
        base_bank: &Bank,
        quote_bank: &Bank,
        quote_atoms_traded: u64,
        base_atoms_traded: u64,
    ) -> ProgramResult {
        if !self.get_is_bid() {
            return Err(ProgramError::InvalidArgument);
        }

        let collateral_shares_delta =
            convert_tokens_to_asset_shares(quote_atoms_traded, quote_bank)?;
        let liability_shares_delta =
            convert_tokens_to_liability_shares(base_atoms_traded, base_bank)?;

        //collateral amount
        self.collateral_shares =
            WrappedI80F48::from(I80F48::from(self.collateral_shares) - collateral_shares_delta);
        //liability amount reduced
        self.liability_shares =
            WrappedI80F48::from(I80F48::from(self.liability_shares) - liability_shares_delta);
        Ok(())
    }

    pub fn reduce_ask(&mut self, base_bank: &Bank, base_atoms_traded: u64) -> ProgramResult {
        if self.get_is_bid() {
            return Err(ProgramError::InvalidArgument);
        }

        let collateral_shares_delta = convert_tokens_to_asset_shares(base_atoms_traded, base_bank)?;

        self.collateral_shares =
            WrappedI80F48::from(I80F48::from(self.collateral_shares) - collateral_shares_delta);
        self.liability_shares = WrappedI80F48::from(I80F48::from(0));
        Ok(())
    }
}

impl Ord for RestingOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        // We only compare bids with bids or asks with asks. If you want to
        // check if orders match, directly access their prices.
        debug_assert!(self.get_is_bid() == other.get_is_bid());

        if self.get_is_bid() {
            (self.rate_bps).cmp(&other.rate_bps)
        } else {
            (other.rate_bps).cmp(&(self.rate_bps))
        }
    }
}

impl PartialOrd for RestingOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for RestingOrder {
    fn eq(&self, other: &Self) -> bool {
        if self.trader_index != other.trader_index || self.order_type != other.order_type {
            return false;
        }
        if self.order_type == OrderType::Reverse {
            self.rate_bps == other.rate_bps
                || self.rate_bps.wrapping_add(1) == other.rate_bps
                || self.rate_bps.wrapping_sub(1) == other.rate_bps
        } else {
            // Only used in equality check of lookups, so we can ignore size, seqnum, ...
            self.rate_bps == other.rate_bps
        }
    }
}

impl Eq for RestingOrder {}

impl std::fmt::Display for RestingOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "supplied shares: {} debt shares: {}@{}",
            I80F48::from(self.collateral_shares),
            I80F48::from(self.liability_shares),
            self.rate_bps
        )
    }
}
