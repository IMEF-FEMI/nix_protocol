use crate::{
    logs::{emit_stack, FillLog},
    marginfi_utils::{
        convert_tokens_to_asset_shares, convert_tokens_to_liability_shares, cpi_marginfi_borrow,
        cpi_marginfi_deposit_place_order, cpi_marginfi_repay, cpi_marginfi_withdraw,
        get_required_quote_collateral_to_back_loan,
    },
    market_signer_seeds_with_bump,
    program::{expand_market_loans, NixError},
    quantities::WrappedI80F48,
    require,
    state::{market_loan::ActiveLoan, order_type_can_rest, GlobalFixed, MarketLoansFixed},
    utils::{
        assert_already_has_seat, assert_can_take, assert_not_already_expired,
        assert_valid_order_type, get_discriminant, get_now_slot, get_now_unix_timestamp,
        remove_from_global, remove_from_global_core, try_to_add_new_loans, try_to_add_to_global,
        try_to_move_global_tokens,
    },
    validation::{
        get_market_fee_receiver_address, get_nix_marginfi_account_address, get_vault_address,
        loaders::{CreateMarketContext, GlobalTradeAccounts, MarginfiCpiAccounts},
        MarketSigner, MintAccountInfo, NixAccount, NixAccountInfo, Program, Signer,
    },
};
use bytemuck::{Pod, Zeroable};

use fixed::types::I80F48;
use hypertree::{
    get_helper, get_mut_helper, is_not_nil, trace, DataIndex, FreeList, FreeListNode, Get,
    HyperTreeReadOperations, HyperTreeValueIteratorTrait, HyperTreeWriteOperations, PodBool,
    RBNode, NIL,
};

use shank::ShankType;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};
use static_assertions::const_assert_eq;
use std::mem::size_of;

use super::{
    ClaimedSeat, DerefOrBorrow, DerefOrBorrowMut, DynamicAccount, OrderType, RestingOrder,
    MARKET_BLOCK_SIZE, MARKET_FIXED_SIZE, MARKET_FREE_LIST_BLOCK_SIZE,
};

#[path = "market_helpers.rs"]
pub mod market_helpers;
pub use market_helpers::*;

mod helpers {
    use hypertree::{get_mut_helper, RBNode};

    use crate::state::RestingOrder;

    use super::*;

    /// Read a `RBNode<ClaimedSeat>` in an array of data at a given index.
    pub fn get_helper_seat(data: &[u8], index: DataIndex) -> &RBNode<ClaimedSeat> {
        get_helper::<RBNode<ClaimedSeat>>(data, index)
    }
    /// Read a `RBNode<ClaimedSeat>` in an array of data at a given index.
    pub fn get_mut_helper_seat(data: &mut [u8], index: DataIndex) -> &mut RBNode<ClaimedSeat> {
        get_mut_helper::<RBNode<ClaimedSeat>>(data, index)
    }
    pub fn get_helper_order(data: &[u8], index: DataIndex) -> &RBNode<RestingOrder> {
        get_helper::<RBNode<RestingOrder>>(data, index)
    }
    pub fn get_mut_helper_order(data: &mut [u8], index: DataIndex) -> &mut RBNode<RestingOrder> {
        get_mut_helper::<RBNode<RestingOrder>>(data, index)
    }

    pub fn get_helper_bid_order(data: &[u8], index: DataIndex) -> &RBNode<RestingOrder> {
        get_helper::<RBNode<RestingOrder>>(data, index)
    }
    pub fn get_mut_helper_bid_order(
        data: &mut [u8],
        index: DataIndex,
    ) -> &mut RBNode<RestingOrder> {
        get_mut_helper::<RBNode<RestingOrder>>(data, index)
    }

    pub fn get_helper_ask_order(data: &[u8], index: DataIndex) -> &RBNode<RestingOrder> {
        get_helper::<RBNode<RestingOrder>>(data, index)
    }
    pub fn get_mut_helper_ask_order(
        data: &mut [u8],
        index: DataIndex,
    ) -> &mut RBNode<RestingOrder> {
        get_mut_helper::<RBNode<RestingOrder>>(data, index)
    }
}

pub use helpers::*;

pub struct RestRemainingOrderToMarketArgs<'a, 'info> {
    pub trader_index: DataIndex,
    pub rate_bps: u16,
    pub is_bid: bool,
    pub current_slot: Option<u32>,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
    pub use_a_tree: bool,
    pub global_trade_accounts_opts: [Option<GlobalTradeAccounts<'a, 'info>>; 2],
}
pub struct AddOrderToMarketArgs<'a, 'info> {
    pub market: Pubkey,
    pub market_signer: MarketSigner<'a, 'info>,
    pub market_signer_bump: u8,
    pub trader_index: DataIndex,
    pub num_base_atoms: u64,
    pub rate_bps: u16,
    pub reverse_spread_bps: u16,
    pub is_bid: bool,
    pub use_a_tree: bool,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
    pub base_mint: MintAccountInfo<'a, 'info>,
    pub quote_mint: MintAccountInfo<'a, 'info>,
    pub base_oracle_price_usd: I80F48,
    pub quote_oracle_price_usd: I80F48,
    pub global_trade_accounts_opts: [Option<GlobalTradeAccounts<'a, 'info>>; 2],
    pub marginfi_cpi_accounts_opts: [Option<MarginfiCpiAccounts<'a, 'info>>; 2],
    pub current_slot: Option<u32>,
}

#[derive(Default)]
pub struct AddOrderToMarketResult {
    pub order_sequence_number: u64,
    pub order_index: DataIndex,
    pub base_atoms_traded: u64,
    pub quote_atoms_traded: u64,
    pub matched_loans: Vec<ActiveLoan>,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum MarketDataTreeNodeType {
    // 0 is reserved because zeroed byte arrays should be empty.
    Empty = 0,
    #[default]
    ClaimedSeat = 1,
    RestingOrder = 2,
}
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Pod, Zeroable)]
pub struct MarketUnusedFreeListPadding {
    _padding: [u64; 12],
    _padding2: [u8; 12],
}
// 4 bytes are for the free list, rest is payload.
const_assert_eq!(
    size_of::<MarketUnusedFreeListPadding>(),
    MARKET_FREE_LIST_BLOCK_SIZE
);
// Does not need to align to word boundaries because does not deserialize.
#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod, ShankType)]
pub struct MarketFixed {
    /// Discriminant for identifying this type of account.
    pub discriminant: u64,

    /// Version
    version: u8,
    base_a_mint_decimals: u8,
    base_b_mint_decimals: u8,
    // base_a_vault_bump: u8,
    // base_b_vault_bump: u8,

    // base_a_fee_receiver_bump: u8,
    // base_b_fee_receiver_bump: u8,
    market_state: u8,

    // base_a_marginfi_account_bump: u8,
    // base_b_marginfi_account_bump: u8,
    _padding1: [u8; 4],

    /// Base A mint
    base_a_mint: Pubkey,
    /// Base B mint
    base_b_mint: Pubkey,

    /// Base vault
    base_a_vault: Pubkey,
    /// Base B vault
    base_b_vault: Pubkey,

    /// The sequence number of the next order.
    base_a_order_sequence_number: u64,
    base_b_order_sequence_number: u64,

    /// Num bytes allocated as RestingOrder or ClaimedSeat or FreeList. Does not
    /// include the fixed bytes.
    num_bytes_allocated: u32,

    /// Red-black tree root representing the bids in the order book.
    base_a_bids_root_index: DataIndex,
    base_a_bids_best_index: DataIndex,

    base_b_bids_root_index: DataIndex,
    base_b_bids_best_index: DataIndex,

    /// Red-black tree root representing the asks in the order book.
    base_a_asks_root_index: DataIndex,
    base_a_asks_best_index: DataIndex,

    base_b_asks_root_index: DataIndex,
    base_b_asks_best_index: DataIndex,

    /// Red-black tree root representing the seats
    claimed_seats_root_index: DataIndex,

    /// LinkedList representing all free blocks that could be used for ClaimedSeats or RestingOrders
    free_list_head_index: DataIndex,

    _padding2: [u32; 1],

    /// base a MarginFi group account
    base_a_marginfi_group: Pubkey,
    /// base a MarginFi bank account
    base_a_marginfi_bank: Pubkey,
    base_a_marginfi_account: Pubkey,
    /// base b MarginFi group account
    base_b_marginfi_group: Pubkey,
    /// base b MarginFi bank account
    base_b_marginfi_bank: Pubkey,
    base_b_marginfi_account: Pubkey,

    fee_state: FeeState,

    /// volumes traded over lifetime, can overflow. This is for
    /// informational and monitoring purposes only. This is not guaranteed to
    /// be maintained. It does not secure any value in Nix.
    /// Use at your own risk.
    base_a_match_volume: WrappedI80F48,
    base_b_match_volume: WrappedI80F48,

    base_a_marginfi_account_asset_shares: WrappedI80F48,
    base_a_marginfi_account_liability_shares: WrappedI80F48,

    base_b_marginfi_account_shares: WrappedI80F48,
    base_b_marginfi_account_liability_shares: WrappedI80F48,

    // // Unused padding. Saved in case a later version wants to be backwards
    // // compatible. Also, it is nice to have the fixed size be a round number,
    // // 256 bytes.
    _padding3: [u64; 16],
}

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod, ShankType)]
pub struct FeeState {
    protocol_fee_rate_bps: u64,
    ltv_buffer_bps: u64,
    base_a_fee_receiver: Pubkey,
    base_b_fee_receiver: Pubkey,
    admin: Pubkey,
}

const_assert_eq!(
    size_of::<MarketFixed>(),
    8 +   // discriminant
    1 +   // version
    1 +   // base_a_mint_decimals
    1 +   // base_b_mint_decimals
    // 1 +   // base_a_vault_bump
    // 1 +   // base_b_vault_bump
    // 1 +   // base_a_fee_receiver_bump
    // 1 +   // base_b_fee_receiver_bump
    1 +   // market_state
    // 1 +   // base_a_marginfi_account_bump
    // 1 +   // base_b_marginfi_account_bump
    4 +   // _padding1
    32 +  // base_a_mint
    32 +  // base_b_mint
    32 +  // base_a_vault
    32 +  // base_b_vault
    8 +   // base_a_order_sequence_number
    8 +   // base_b_order_sequence_number
    4 +   // num_bytes_allocated
    4 +   // base_a_bids_root_index
    4 +   // base_a_bids_best_index
    4 +   // base_b_bids_root_index
    4 +   // base_b_bids_best_index
    4 +   // base_a_asks_root_index
    4 +   // base_a_asks_best_index
    4 +   // base_b_asks_root_index
    4 +   // base_b_asks_best_index
    4 +   // claimed_seats_root_index
    4 +   // free_list_head_index
    4 +   // _padding2
    32 +  // base_a_marginfi_group
    32 +  // base_a_marginfi_bank
    32 +  // base_a_marginfi_account
    32 +  // base_b_marginfi_group
    32 +  // base_b_marginfi_bank
    32 +  // base_b_marginfi_account
    size_of::<FeeState>() + // fee_state
    16 + // base_a_match_volume
    16 + // base_b_match_volume
    16 + // base_a_marginfi_account_asset_shares
    16 + // base_a_marginfi_account_liability_shares
    16 + // base_b_marginfi_account_shares
    16 + // base_b_marginfi_account_liability_shares
    (16 * 8) // _padding3: [u64; 23]
);

const_assert_eq!(size_of::<MarketFixed>(), MARKET_FIXED_SIZE);
const_assert_eq!(size_of::<MarketFixed>() % 8, 0);
impl Get for MarketFixed {}

impl MarketFixed {
    pub(crate) fn new_empty(
        ctx: &CreateMarketContext,
        protocol_fee_rate_bps: u64,
        ltv_buffer_bps: u64,
    ) -> Self {
        let CreateMarketContext {
            base_a_mint,
            base_b_mint,
            market,
            base_a_marginfi_group,
            base_b_marginfi_group,
            base_a_marginfi_bank,
            base_b_marginfi_bank,
            admin,
            ..
        } = ctx;
        let (base_a_vault, _) = get_vault_address(market.key, base_a_mint.as_ref().key);
        let (base_b_vault, _) = get_vault_address(market.key, base_b_mint.as_ref().key);
        let (base_a_fee_receiver, _) =
            get_market_fee_receiver_address(market.key, base_a_mint.as_ref().key);
        let (base_b_fee_receiver, _) =
            get_market_fee_receiver_address(market.key, base_b_mint.as_ref().key);
        let (base_a_marginfi_account, _) =
            get_nix_marginfi_account_address(market.key, base_a_mint.as_ref().key);
        let (base_b_marginfi_account, _) =
            get_nix_marginfi_account_address(market.key, base_b_mint.as_ref().key);

        MarketFixed {
            discriminant: get_discriminant::<MarketFixed>().unwrap(),
            version: 1,
            base_a_mint_decimals: ctx.base_a_mint.mint.decimals,
            base_b_mint_decimals: ctx.base_b_mint.mint.decimals,
            market_state: 0,
            _padding1: Default::default(),
            base_a_mint: *base_a_mint.as_ref().key,
            base_b_mint: *base_b_mint.as_ref().key,
            base_a_vault,
            base_b_vault,
            base_a_order_sequence_number: 0,
            base_b_order_sequence_number: 0,
            num_bytes_allocated: 0,
            base_a_bids_root_index: NIL,
            base_a_bids_best_index: NIL,
            base_b_bids_root_index: NIL,
            base_b_bids_best_index: NIL,
            base_a_asks_root_index: NIL,
            base_a_asks_best_index: NIL,
            base_b_asks_root_index: NIL,
            base_b_asks_best_index: NIL,
            claimed_seats_root_index: NIL,
            free_list_head_index: NIL,
            _padding2: Default::default(),
            base_a_marginfi_group: *base_a_marginfi_group.as_ref().key,
            base_a_marginfi_bank: *base_a_marginfi_bank.as_ref().key,
            base_a_marginfi_account,
            base_b_marginfi_group: *base_b_marginfi_group.as_ref().key,
            base_b_marginfi_bank: *base_b_marginfi_bank.as_ref().key,
            base_b_marginfi_account,
            fee_state: FeeState {
                protocol_fee_rate_bps,
                ltv_buffer_bps,
                base_a_fee_receiver,
                base_b_fee_receiver,
                admin: *admin.as_ref().key,
            },
            base_a_match_volume: Default::default(),
            base_b_match_volume: Default::default(),
            base_a_marginfi_account_asset_shares: Default::default(),
            base_a_marginfi_account_liability_shares: Default::default(),
            base_b_marginfi_account_shares: Default::default(),
            base_b_marginfi_account_liability_shares: Default::default(),
            _padding3: Default::default(),
        }
    }

    pub fn get_base_a_mint(&self) -> &Pubkey {
        &self.base_a_mint
    }
    pub fn get_base_b_mint(&self) -> &Pubkey {
        &self.base_b_mint
    }

    pub fn get_base_a_decimals(&self) -> u8 {
        self.base_a_mint_decimals
    }
    pub fn get_base_b_decimals(&self) -> u8 {
        self.base_b_mint_decimals
    }
    pub fn get_base_a_vault(&self) -> &Pubkey {
        &self.base_a_vault
    }
    pub fn get_base_b_vault(&self) -> &Pubkey {
        &self.base_b_vault
    }
    pub fn get_base_a_fee_receiver(&self) -> &Pubkey {
        &self.fee_state.base_a_fee_receiver
    }
    pub fn get_base_b_fee_receiver(&self) -> &Pubkey {
        &self.fee_state.base_b_fee_receiver
    }
    pub fn get_base_a_marginfi_account(&self) -> &Pubkey {
        &self.base_a_marginfi_account
    }
    pub fn get_base_b_marginfi_account(&self) -> &Pubkey {
        &self.base_b_marginfi_account
    }
    pub fn get_base_a_marginfi_group(&self) -> &Pubkey {
        &self.base_a_marginfi_group
    }
    pub fn get_base_b_marginfi_group(&self) -> &Pubkey {
        &self.base_b_marginfi_group
    }
    pub fn get_base_a_marginfi_bank(&self) -> &Pubkey {
        &self.base_a_marginfi_bank
    }
    pub fn get_base_b_marginfi_bank(&self) -> &Pubkey {
        &self.base_b_marginfi_bank
    }

    pub fn get_base_a_order_sequence_number(&self) -> u64 {
        self.base_a_order_sequence_number
    }
    pub fn get_base_b_order_sequence_number(&self) -> u64 {
        self.base_b_order_sequence_number
    }
    pub fn has_free_block(&self) -> bool {
        self.free_list_head_index != NIL
    }
    pub fn get_admin(&self) -> &Pubkey {
        &self.fee_state.admin
    }
}

impl NixAccount for MarketFixed {
    fn verify_discriminant(&self) -> ProgramResult {
        let expected_discriminant: u64 = crate::utils::get_discriminant::<MarketFixed>().unwrap();

        require!(
            self.discriminant == expected_discriminant,
            ProgramError::InvalidAccountData,
            "Invalid market discriminant actual: {} expected: {}",
            self.discriminant,
            expected_discriminant
        )?;
        Ok(())
    }
}

/// Fully owned Market, used in clients that can copy.
pub type MarketValue = DynamicAccount<MarketFixed, Vec<u8>>;
/// Full market reference type.
pub type MarketRef<'a> = DynamicAccount<&'a MarketFixed, &'a [u8]>;
/// Full market reference type.
pub type MarketRefMut<'a> = DynamicAccount<&'a mut MarketFixed, &'a mut [u8]>;

mod types {
    use hypertree::{RedBlackTree, RedBlackTreeReadOnly};

    use crate::state::{ClaimedSeat, RestingOrder};

    pub type ClaimedSeatTree<'a> = RedBlackTree<'a, ClaimedSeat>;
    pub type ClaimedSeatTreeReadOnly<'a> = RedBlackTreeReadOnly<'a, ClaimedSeat>;
    pub type Bookside<'a> = RedBlackTree<'a, RestingOrder>;
    pub type BooksideReadOnly<'a> = RedBlackTreeReadOnly<'a, RestingOrder>;
}
pub use types::*;

// This generic impl covers MarketRef, MarketRefMut and other
// DynamicAccount variants that allow read access.
impl<Fixed: DerefOrBorrow<MarketFixed>, Dynamic: DerefOrBorrow<[u8]>>
    DynamicAccount<Fixed, Dynamic>
{
    fn borrow_market(&self) -> MarketRef {
        MarketRef {
            fixed: self.fixed.deref_or_borrow(),
            dynamic: self.dynamic.deref_or_borrow(),
        }
    }

    pub fn get_base_a_mint(&self) -> &Pubkey {
        let DynamicAccount { fixed, .. } = self.borrow_market();
        fixed.get_base_a_mint()
    }

    pub fn get_base_b_mint(&self) -> &Pubkey {
        let DynamicAccount { fixed, .. } = self.borrow_market();
        fixed.get_base_b_mint()
    }
    pub fn has_free_block(&self) -> bool {
        let DynamicAccount { fixed, .. } = self.borrow_market();
        let free_list_head_index: DataIndex = fixed.free_list_head_index;
        return free_list_head_index != NIL;
    }

    pub fn has_two_free_blocks(&self) -> bool {
        let DynamicAccount { fixed, dynamic } = self.borrow_market();
        let free_list_head_index: DataIndex = fixed.free_list_head_index;
        if free_list_head_index == NIL {
            return false;
        }
        let free_list_head: &FreeListNode<MarketUnusedFreeListPadding> =
            get_helper::<FreeListNode<MarketUnusedFreeListPadding>>(dynamic, free_list_head_index);
        free_list_head.has_next()
    }
    pub fn get_trader_index(&self, trader: &Pubkey) -> DataIndex {
        let DynamicAccount { fixed, dynamic } = self.borrow_market();

        let claimed_seats_tree: ClaimedSeatTreeReadOnly =
            ClaimedSeatTreeReadOnly::new(dynamic, fixed.claimed_seats_root_index, NIL);
        let trader_index: DataIndex =
            claimed_seats_tree.lookup_index(&ClaimedSeat::new_empty(*trader));
        trader_index
    }
    pub fn get_trader_key_by_index(&self, index: DataIndex) -> &Pubkey {
        let DynamicAccount { dynamic, .. } = self.borrow_market();

        &get_helper_seat(dynamic, index).get_value().trader
    }

    pub fn get_order_by_index(&self, index: DataIndex) -> &RestingOrder {
        let DynamicAccount { dynamic, .. } = self.borrow_market();
        &get_helper::<RBNode<RestingOrder>>(dynamic, index).get_value()
    }

}

// This generic impl covers MarketRef, MarketRefMut and other
// DynamicAccount variants that allow write access.
impl<
        Fixed: DerefOrBorrowMut<MarketFixed> + DerefOrBorrow<MarketFixed>,
        Dynamic: DerefOrBorrowMut<[u8]> + DerefOrBorrow<[u8]>,
    > DynamicAccount<Fixed, Dynamic>
{
    fn borrow_mut(&mut self) -> MarketRefMut {
        MarketRefMut {
            fixed: self.fixed.deref_or_borrow_mut(),
            dynamic: self.dynamic.deref_or_borrow_mut(),
        }
    }

    pub fn market_expand(&mut self) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();
        let mut free_list: FreeList<MarketUnusedFreeListPadding> =
            FreeList::new(dynamic, fixed.free_list_head_index);

        free_list.add(fixed.num_bytes_allocated);
        fixed.num_bytes_allocated += MARKET_BLOCK_SIZE as u32;
        fixed.free_list_head_index = free_list.get_head();
        Ok(())
    }

    pub fn claim_seat(&mut self, trader: &Pubkey) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();
        let free_address: DataIndex = get_free_address_on_market_fixed_for_seat(fixed, dynamic);

        let mut claimed_seats_tree: ClaimedSeatTree =
            ClaimedSeatTree::new(dynamic, fixed.claimed_seats_root_index, NIL);

        let claimed_seat: ClaimedSeat = ClaimedSeat::new_empty(*trader);

        require!(
            claimed_seats_tree.lookup_index(&claimed_seat) == NIL,
            NixError::AlreadyClaimedSeat,
            "Already claimed seat",
        )?;
        claimed_seats_tree.insert(free_address, claimed_seat);
        fixed.claimed_seats_root_index = claimed_seats_tree.get_root_index();
        get_mut_helper::<RBNode<ClaimedSeat>>(dynamic, free_address)
            .set_payload_type(MarketDataTreeNodeType::ClaimedSeat as u8);
        Ok(())
    }

    pub fn deposit(
        &mut self,
        trader_index: DataIndex,
        asset_shares: WrappedI80F48,
        update_base_a: bool,
    ) -> ProgramResult {
        require!(
            is_not_nil!(trader_index),
            NixError::InvalidDepositAccounts,
            "No seat initialized",
        )?;
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();
        update_balance(
            fixed,
            dynamic,
            trader_index,
            update_base_a,
            true,
            asset_shares,
        )?;
        Ok(())
    }

    /// Place an order and update the market
    ///
    /// 1. Check the order against the opposite bookside
    /// 2. Rest any amount of the order leftover on the book
    pub fn place_order<'a, 'info>(
        &mut self,
        args: AddOrderToMarketArgs<'a, 'info>,
        remaining_accounts: &'a [AccountInfo<'a>],
    ) -> Result<AddOrderToMarketResult, ProgramError>
    where
        'a: 'info,
    {
        let AddOrderToMarketArgs {
            market,
            market_signer,
            market_signer_bump,
            trader_index,
            num_base_atoms,
            rate_bps,
            reverse_spread_bps,
            is_bid,
            use_a_tree,
            last_valid_slot,
            order_type,
            base_mint,
            quote_mint,
            base_oracle_price_usd,
            quote_oracle_price_usd,
            global_trade_accounts_opts,
            marginfi_cpi_accounts_opts,
            current_slot,
        } = args;

        assert_already_has_seat(trader_index)?;
        let now_slot: u32 = current_slot.unwrap_or_else(|| get_now_slot());
        let now_unix_timestamp = get_now_unix_timestamp();

        assert_not_already_expired(last_valid_slot, now_slot)?;
        assert_valid_order_type(order_type, is_bid)?;

        let DynamicAccount { fixed, dynamic } = self.borrow_mut();

        let (bids_best_index, asks_best_index, bids_root_index, asks_root_index) =
            get_tree_indexes(fixed, use_a_tree);

        let base_marginfi_bank = marginfi_cpi_accounts_opts[0]
            .as_ref()
            .unwrap()
            .marginfi_bank
            .get_fixed()
            .unwrap();
        let quote_marginfi_bank = marginfi_cpi_accounts_opts[1]
            .as_ref()
            .unwrap()
            .marginfi_bank
            .get_fixed()
            .unwrap();

        let mut current_maker_order_index: DataIndex = if is_bid {
            asks_best_index
        } else {
            bids_best_index
        };

        let buffer_f = I80F48::from_num(10000i64 - fixed.fee_state.ltv_buffer_bps as i64)
            .checked_div(I80F48::from_num(10000))
            .ok_or(NixError::NumericalOverflow)?;
        let mut total_base_atoms_traded: u64 = 0;
        let mut total_quote_atoms_traded: u64 = 0;

        let mut global_base_atoms_traded: u64 = 0;
        let mut global_quote_atoms_traded: u64 = 0;

        let mut remaining_base_atoms: u64 = num_base_atoms;

        let taker: Pubkey = get_helper_seat(dynamic, trader_index).get_value().trader;
        let mut new_loans = Vec::new();

        while remaining_base_atoms > 0 && is_not_nil!(current_maker_order_index) {
            let maker_order: &RestingOrder =
                get_helper::<RBNode<RestingOrder>>(dynamic.as_ref(), current_maker_order_index)
                    .get_value();

            if maker_order.is_expired(now_slot)
                || I80F48::from(maker_order.get_collateral_shares()) == 0
            {
                if maker_order.get_is_bid() {
                    // convert expired order to a loan on underlying protocol
                    let active_loan = ActiveLoan::new_empty(
                        use_a_tree,
                        0, //direct underlying protocol
                        current_maker_order_index,
                        maker_order.is_global(),
                        maker_order.get_collateral_shares(),
                        maker_order.get_liability_shares(),
                        0, //underlying protocol rate
                        now_unix_timestamp,
                        now_slot.into(),
                    );
                    new_loans.push(active_loan);
                }
                let next_maker_order_index: DataIndex = get_next_candidate_match_index(
                    dynamic,
                    current_maker_order_index,
                    asks_root_index,
                    asks_best_index,
                    bids_root_index,
                    bids_best_index,
                    is_bid,
                );

                remove_and_update_balances(
                    fixed,
                    dynamic,
                    use_a_tree,
                    current_maker_order_index,
                    &global_trade_accounts_opts,
                )?;
                current_maker_order_index = next_maker_order_index;
                continue;
            }

            // Stop trying to match if rate no longer satisfies limit.
            if (is_bid && maker_order.get_rate_bps() > rate_bps)
                || (!is_bid && maker_order.get_rate_bps() < rate_bps)
            {
                break;
            }

            // Got a match. First make sure we are allowed to match. We check
            // inside the matching rather than skipping the matching altogether
            // because post only orders should fail, not produce a crossed book.
            assert_can_take(order_type)?;

            let maker_sequence_number = maker_order.get_sequence_number();
            let maker_trader_index: DataIndex = maker_order.get_trader_index();

            let maker_base_atoms: u64 = maker_order.get_num_base_atoms(&base_marginfi_bank)?;
            let did_fully_match_resting_order: bool = remaining_base_atoms >= maker_base_atoms;
            let base_atoms_traded: u64 = if did_fully_match_resting_order {
                maker_base_atoms
            } else {
                remaining_base_atoms
            };

            let matched_rate = maker_order.get_rate_bps();

            let quote_atoms_traded: u64 = get_required_quote_collateral_to_back_loan(
                &base_marginfi_bank,
                &quote_marginfi_bank,
                base_oracle_price_usd,
                quote_oracle_price_usd,
                buffer_f,
                base_atoms_traded,
            )?;

            // If it is a global order, just in time bring the funds over, or
            // remove from the tree and continue on to the next order.
            let maker: Pubkey = get_helper_seat(dynamic, maker_trader_index)
                .get_value()
                .trader;
            let is_maker_global: bool = maker_order.is_global();

            if is_maker_global {
                let has_enough_tokens: bool = try_to_move_global_tokens(
                    &global_trade_accounts_opts[0].clone(),
                    &base_mint,
                    &maker,
                    //global orders are expected to only be asks
                    //meaning they supply base atoms only
                    base_atoms_traded,
                )?;

                if !has_enough_tokens {
                    let next_maker_order_index: DataIndex = get_next_candidate_match_index(
                        dynamic,
                        current_maker_order_index,
                        asks_root_index,
                        asks_best_index,
                        bids_root_index,
                        bids_best_index,
                        is_bid,
                    );

                    remove_and_update_balances(
                        fixed,
                        dynamic,
                        use_a_tree,
                        current_maker_order_index,
                        &global_trade_accounts_opts,
                    )?;
                    current_maker_order_index = next_maker_order_index;
                    continue;
                } else {
                    // base_atoms_traded is currently in token form
                    // we will make cpi calls to deposit back on marginfi (at eof)
                    if is_bid {
                        global_base_atoms_traded = global_base_atoms_traded
                            .checked_add(base_atoms_traded)
                            .ok_or(NixError::NumericalOverflow)?;
                    } else {
                        global_quote_atoms_traded = global_quote_atoms_traded
                            .checked_add(quote_atoms_traded)
                            .ok_or(NixError::NumericalOverflow)?;
                    }
                }
            }

            total_base_atoms_traded = total_base_atoms_traded
                .checked_add(base_atoms_traded)
                .ok_or(NixError::NumericalOverflow)?;
            total_quote_atoms_traded = total_quote_atoms_traded
                .checked_add(quote_atoms_traded)
                .ok_or(NixError::NumericalOverflow)?;

            let base_atom_asset_shares_traded =
                convert_tokens_to_asset_shares(base_atoms_traded, &base_marginfi_bank)?;
            let quote_atom_asset_shares_traded =
                convert_tokens_to_asset_shares(quote_atoms_traded, &quote_marginfi_bank)?;
            // Decrease taker
            update_balance(
                fixed,
                dynamic,
                trader_index,
                should_update_base_a(use_a_tree, !is_bid),
                false,
                if is_bid {
                    quote_atom_asset_shares_traded.into()
                } else {
                    base_atom_asset_shares_traded.into()
                },
            )?;

            // Increase taker
            //only a borrower (that isn't reversing) would receive their borrowed tokens
            if is_bid && order_type != OrderType::Reverse {
                assert_not_already_expired(last_valid_slot, now_slot)?;
                update_balance(
                    fixed,
                    dynamic,
                    trader_index,
                    should_update_base_a(use_a_tree, is_bid),
                    true,
                    if is_bid {
                        base_atom_asset_shares_traded.into()
                    } else {
                        quote_atom_asset_shares_traded.into()
                    },
                )?;
            }

            // record maker & taker volume
            record_volume_by_trader_index(
                dynamic,
                maker_trader_index,
                base_atom_asset_shares_traded,
                use_a_tree,
            );
            record_volume_by_trader_index(
                dynamic,
                trader_index,
                base_atom_asset_shares_traded,
                use_a_tree,
            );
            emit_stack(FillLog {
                market,
                maker,
                taker,
                base_mint: *base_mint.as_ref().key,
                quote_mint: *quote_mint.as_ref().key,
                base_atoms: base_atoms_traded,
                quote_atoms: quote_atoms_traded,
                rate_bps: matched_rate,
                maker_sequence_number,
                taker_sequence_number: if use_a_tree {
                    fixed.base_a_order_sequence_number
                } else {
                    fixed.base_b_order_sequence_number
                },
                taker_is_buy: PodBool::from(is_bid),
                is_maker_global: PodBool::from(is_maker_global),
                _padding: [0; 6],
                _padding1: [0; 14],
            })?;

            if did_fully_match_resting_order {
                // Get paid for removing a global order.
                if get_helper::<RBNode<RestingOrder>>(dynamic, current_maker_order_index)
                    .get_value()
                    .is_global()
                {
                    if is_bid {
                        remove_from_global(&global_trade_accounts_opts[0])?;
                    } else {
                        remove_from_global(&global_trade_accounts_opts[1])?;
                    }
                }
                let next_maker_order_index: DataIndex = get_next_candidate_match_index(
                    dynamic,
                    current_maker_order_index,
                    asks_root_index,
                    asks_best_index,
                    bids_root_index,
                    bids_best_index,
                    is_bid,
                );

                remove_order_from_tree_and_free(
                    fixed,
                    dynamic,
                    use_a_tree,
                    current_maker_order_index,
                    !is_bid,
                )?;

                remaining_base_atoms = remaining_base_atoms
                    .checked_sub(base_atoms_traded)
                    .ok_or(NixError::NumericalOverflow)?;

                let active_loan = ActiveLoan::new_empty(
                    use_a_tree,
                    if is_bid {
                        current_maker_order_index
                    } else {
                        trader_index
                    },
                    if is_bid {
                        trader_index
                    } else {
                        current_maker_order_index
                    },
                    if is_bid {
                        is_maker_global
                    } else {
                        order_type == OrderType::Global
                    },
                    quote_atom_asset_shares_traded.into(),
                    base_atom_asset_shares_traded.into(),
                    matched_rate,
                    now_unix_timestamp,
                    now_slot.into(),
                );

                new_loans.push(active_loan);
                current_maker_order_index = next_maker_order_index;
            } else {
                let maker_order: &mut RestingOrder =
                    get_mut_helper::<RBNode<RestingOrder>>(dynamic, current_maker_order_index)
                        .get_mut_value();
                if maker_order.get_is_bid() {
                    // If the maker order is a bid, we need to update the asset shares
                    // to reflect the amount of asset shares that were traded.

                    maker_order.reduce_bid(
                        &base_marginfi_bank,
                        &quote_marginfi_bank,
                        quote_atoms_traded,
                        base_atoms_traded,
                    )?;
                } else {
                    maker_order.reduce_ask(&base_marginfi_bank, base_atoms_traded)?;
                }
                remaining_base_atoms = 0;
            }

            // Stop if the last resting order did not fully match since that
            // means the taker was exhausted.
            if !did_fully_match_resting_order {
                break;
            }
        }
        // Record volume on market
        if use_a_tree {
            fixed.base_a_match_volume = WrappedI80F48::from(
                I80F48::from(fixed.base_a_match_volume)
                    .wrapping_add(I80F48::from_num(total_base_atoms_traded)),
            );
        } else {
            fixed.base_b_match_volume = WrappedI80F48::from(
                I80F48::from(fixed.base_b_match_volume)
                    .wrapping_add(I80F48::from_num(total_base_atoms_traded)),
            );
        }

        // Bump the order sequence number even for orders which do not end up
        // resting.
        let order_sequence_number: u64 = if use_a_tree {
            fixed.base_a_order_sequence_number = fixed.base_a_order_sequence_number.wrapping_add(1);
            fixed.base_a_order_sequence_number
        } else {
            fixed.base_b_order_sequence_number = fixed.base_b_order_sequence_number.wrapping_add(1);
            fixed.base_b_order_sequence_number
        };

        // If there is nothing left to rest, then return before resting.
        if !order_type_can_rest(order_type) || remaining_base_atoms == 0 || rate_bps == 0 {
            return Ok(AddOrderToMarketResult {
                order_sequence_number,
                order_index: NIL,
                base_atoms_traded: total_base_atoms_traded,
                quote_atoms_traded: total_quote_atoms_traded,
                matched_loans: new_loans,
            });
        }

        if is_bid {
            cpi_marginfi_borrow(
                &marginfi_cpi_accounts_opts,
                &global_trade_accounts_opts,
                remaining_base_atoms,
                if base_mint.as_ref().owner == &spl_token_2022::ID {
                    Some(&base_mint)
                } else {
                    None
                },
                market_signer.clone(),
                market_signer_seeds_with_bump!(market, market_signer_bump),
                remaining_accounts,
            )?;
            //deposit the borrowed base atoms into the marginfi base account
            cpi_marginfi_deposit_place_order(
                marginfi_cpi_accounts_opts[0].as_ref().unwrap(),
                market_signer.clone(),
                global_trade_accounts_opts[0]
                    .as_ref()
                    .unwrap()
                    .market_vault_opt
                    .as_ref()
                    .unwrap(),
                global_trade_accounts_opts[0]
                    .as_ref()
                    .unwrap()
                    .token_program_opt
                    .as_ref()
                    .unwrap(),
                if base_mint.as_ref().owner == &spl_token_2022::ID {
                    Some(&base_mint)
                } else {
                    None
                },
                market_signer_seeds_with_bump!(market, market_signer_bump),
            )?;
        } else {
            //withdraw total_base_atoms_traded from marginfi base account
            cpi_marginfi_withdraw(
                &marginfi_cpi_accounts_opts,
                &global_trade_accounts_opts,
                total_base_atoms_traded,
                if base_mint.as_ref().owner == &spl_token_2022::ID {
                    Some(&base_mint)
                } else {
                    None
                },
                market_signer.clone(),
                market_signer_seeds_with_bump!(market, market_signer_bump),
                remaining_accounts,
            )?;
            // repay into marginfi quote account
            cpi_marginfi_repay(
                marginfi_cpi_accounts_opts[1].as_ref().unwrap(),
                market_signer.clone(),
                global_trade_accounts_opts[0]
                    .as_ref()
                    .unwrap()
                    .market_vault_opt
                    .as_ref()
                    .unwrap(),
                global_trade_accounts_opts[0]
                    .as_ref()
                    .unwrap()
                    .token_program_opt
                    .as_ref()
                    .unwrap(),
                if base_mint.as_ref().owner == &spl_token_2022::ID {
                    Some(&base_mint)
                } else {
                    None
                },
                market_signer_seeds_with_bump!(market, market_signer_bump),
            )?;
        }

        //use total received base_atoms to create reverse order
        if is_bid && order_type == OrderType::Reverse {
            // New Ask @R --> Bid @R * (1 - spread)
            let reverse_rate = rate_bps
                .checked_mul(10_000u16 - reverse_spread_bps)
                .and_then(|v| v.checked_div(10_000u16))
                .ok_or(NixError::NumericalOverflow)?;

            let reverse_base_atoms = total_base_atoms_traded
                .checked_add(global_base_atoms_traded + remaining_base_atoms)
                .ok_or(NixError::NumericalOverflow)?;

            let total_reverse_base_shares =
                convert_tokens_to_asset_shares(reverse_base_atoms, &base_marginfi_bank)?;

            if total_reverse_base_shares > 0 {
                // place reverse order on the alternative book side
                let reverse_order_sequence_number: u64 = if use_a_tree {
                    fixed.base_b_order_sequence_number =
                        fixed.base_b_order_sequence_number.wrapping_add(1);
                    fixed.base_b_order_sequence_number
                } else {
                    fixed.base_a_order_sequence_number =
                        fixed.base_a_order_sequence_number.wrapping_add(1);
                    fixed.base_a_order_sequence_number
                };

                let free_address: DataIndex =
                    get_free_address_on_market_fixed_for_ask_order(fixed, dynamic);

                let new_reverse_resting_order: RestingOrder = RestingOrder::new(
                    reverse_rate,
                    reverse_order_sequence_number,
                    total_reverse_base_shares.into(),
                    WrappedI80F48::from(I80F48::from(0)), // liability shares are 0 for asks
                    !use_a_tree,
                    trader_index,
                    last_valid_slot,
                    OrderType::Limit,
                    !is_bid,
                    0,
                )?;

                insert_order_into_tree(
                    use_a_tree,
                    is_bid,
                    fixed,
                    dynamic,
                    free_address,
                    &new_reverse_resting_order,
                );
                set_payload_order(dynamic, free_address);

                return Ok(AddOrderToMarketResult {
                    order_sequence_number,
                    order_index: free_address,
                    base_atoms_traded: total_base_atoms_traded,
                    quote_atoms_traded: total_quote_atoms_traded,
                    matched_loans: new_loans,
                });
            }
        }

        let remaining_quote_atoms = if is_bid {
            get_required_quote_collateral_to_back_loan(
                &base_marginfi_bank,
                &quote_marginfi_bank,
                base_oracle_price_usd,
                quote_oracle_price_usd,
                buffer_f,
                remaining_base_atoms,
            )
        } else {
            Ok(0u64)
        }?;

        let (remaining_collateral_shares, remaining_liability_shares) = if is_bid {
            (
                convert_tokens_to_asset_shares(remaining_quote_atoms, &quote_marginfi_bank)?,
                convert_tokens_to_liability_shares(remaining_base_atoms, &base_marginfi_bank)?,
            )
        } else {
            // ask
            if order_type == OrderType::Global {
                //global order collateral shares are stored in token form
                (I80F48::from(remaining_base_atoms), I80F48::from(0))
            } else {
                (
                    convert_tokens_to_asset_shares(remaining_base_atoms, &base_marginfi_bank)?,
                    I80F48::from(0),
                )
            }
        };

        let rest_args = RestRemainingOrderToMarketArgs {
            trader_index,
            rate_bps,
            is_bid,
            use_a_tree,
            order_type,
            global_trade_accounts_opts,
            current_slot,
            last_valid_slot,
        };

        self.rest_remaining(
            &rest_args,
            remaining_collateral_shares,
            remaining_liability_shares,
            order_sequence_number,
            total_base_atoms_traded,
            total_quote_atoms_traded,
            new_loans,
        )
    }

    fn rest_remaining<'a, 'info>(
        &mut self,
        args: &RestRemainingOrderToMarketArgs<'a, 'info>,
        remaining_collateral_shares: I80F48,
        remaining_liability_shares: I80F48,
        order_sequence_number: u64,
        total_base_atoms_traded: u64,
        total_quote_atoms_traded: u64,
        loans: Vec<ActiveLoan>,
    ) -> Result<AddOrderToMarketResult, ProgramError>
    where
        'a: 'info,
    {
        let RestRemainingOrderToMarketArgs {
            trader_index,
            rate_bps,
            is_bid,
            last_valid_slot,
            order_type,
            use_a_tree,

            global_trade_accounts_opts,
            ..
        } = args;
        assert_valid_order_type(*order_type, *is_bid)?;
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();

        // Put the remaining in an order on the other bookside.
        let free_address: DataIndex = if *is_bid {
            get_free_address_on_market_fixed_for_bid_order(fixed, dynamic)
        } else {
            get_free_address_on_market_fixed_for_ask_order(fixed, dynamic)
        };

        let resting_order: RestingOrder = RestingOrder::new(
            *rate_bps,
            order_sequence_number,
            remaining_collateral_shares.into(),
            remaining_liability_shares.into(),
            *use_a_tree,
            *trader_index,
            *last_valid_slot,
            *order_type,
            *is_bid,
            0,
        )?;

        if resting_order.is_global() {
            if *is_bid {
                //global is ask only
                return Err(NixError::InvalidGlobalBidOrder.into());
            } else {
                let global_trade_account_opt = &global_trade_accounts_opts[0];
                require!(
                    global_trade_account_opt.is_some(),
                    NixError::MissingGlobal,
                    "Missing global accounts when adding a global",
                )?;
                try_to_add_to_global(&global_trade_account_opt.as_ref().unwrap(), &resting_order)?;
            }
        } else {
            update_balance(
                fixed,
                dynamic,
                *trader_index,
                should_update_base_a(*use_a_tree, !*is_bid),
                false,
                remaining_collateral_shares.into(),
            )?;
        }
        insert_order_into_tree(
            *use_a_tree,
            *is_bid,
            fixed,
            dynamic,
            free_address,
            &resting_order,
        );

        set_payload_order(dynamic, free_address);

        Ok(AddOrderToMarketResult {
            order_sequence_number,
            order_index: free_address,
            base_atoms_traded: total_base_atoms_traded,
            quote_atoms_traded: total_quote_atoms_traded,
            matched_loans: loans,
        })
    }

    // Does a linear scan over the orderbook to find the index to cancel.
    pub fn cancel_order<'a, 'info>(
        &mut self,
        use_a_tree: bool,
        trader_index: DataIndex,
        order_sequence_number: u64,
        base_global: &NixAccountInfo<'a, 'info, GlobalFixed>,
        payer: Signer<'a, 'info>,
        system_program: Program<'a, 'info>,
        market_loans: &NixAccountInfo<'a, 'info, MarketLoansFixed>,
    ) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();

        let (bids_best_index, asks_best_index, bids_root_index, asks_root_index) =
            get_tree_indexes(fixed, use_a_tree);

        let mut index_to_remove: DataIndex = NIL;

        // One iteration to find the index to cancel in the ask side.
        let tree: BooksideReadOnly =
            BooksideReadOnly::new(dynamic, asks_root_index, asks_best_index);

        for (index, resting_order) in tree.iter::<RestingOrder>() {
            if resting_order.get_sequence_number() == order_sequence_number {
                require!(
                    resting_order.get_trader_index() == trader_index,
                    NixError::InvalidCancel,
                    "Cannot cancel for another trader",
                )?;
                require!(
                    index_to_remove == NIL,
                    NixError::InvalidCancel,
                    "Book is broken, matched multiple orders",
                )?;
                index_to_remove = index;
            }
        }

        // Second iteration to find the index to cancel in the bid side.
        let tree: BooksideReadOnly =
            BooksideReadOnly::new(dynamic, bids_root_index, bids_best_index);
        for (index, resting_order) in tree.iter::<RestingOrder>() {
            if resting_order.get_sequence_number() == order_sequence_number {
                require!(
                    resting_order.get_trader_index() == trader_index,
                    NixError::InvalidCancel,
                    "Cannot cancel for another trader",
                )?;
                require!(
                    index_to_remove == NIL,
                    NixError::InvalidCancel,
                    "Book is broken, matched multiple orders",
                )?;
                index_to_remove = index;
            }
        }

        if is_not_nil!(index_to_remove) {
            // Cancel order by index will update balances.
            self.cancel_order_by_index(
                use_a_tree,
                index_to_remove,
                base_global,
                &Some(payer),
                &Some(system_program),
                market_loans,
            )?;
            return Ok(());
        }

        // Do not fail silently.
        Err(NixError::InvalidCancel.into())
    }
    pub fn cancel_order_by_index<'a, 'info>(
        &mut self,
        use_a_tree: bool,
        order_index: DataIndex,
        base_global: &NixAccountInfo<'a, 'info, GlobalFixed>,
        payer: &Option<Signer<'a, 'info>>,
        system_program:  &Option<Program<'a, 'info>>,
        market_loans: &NixAccountInfo<'a, 'info, MarketLoansFixed>,
    ) -> ProgramResult {
        let DynamicAccount { fixed, dynamic } = self.borrow_mut();
        let resting_order: &RestingOrder = get_helper_order(dynamic, order_index).get_value();
        let is_bid: bool = resting_order.get_is_bid();

        // Update the accounting for the order that was just canceled.
        if resting_order.is_global() {
            if is_bid {
                return Err(NixError::InvalidGlobalBidOrder.into());
            } else {
                remove_from_global_core(base_global, payer, system_program)?;
            }
        } else {
            if is_bid {
                let new_active_loan = ActiveLoan::new_empty(
                    use_a_tree,
                    0, //direct underlying protocol
                    resting_order.get_trader_index(),
                    false,
                    resting_order.get_collateral_shares(),
                    resting_order.get_liability_shares(),
                    0, //underlying protocol rate
                    get_now_unix_timestamp(),
                    get_now_slot().into(),
                );
                expand_market_loans::<MarketLoansFixed>(
                    payer.clone().unwrap().as_ref(),
                    market_loans,
                    1,
                )?;

                try_to_add_new_loans(market_loans, [new_active_loan].into())?;
            } else {
                update_balance(
                    fixed,
                    dynamic,
                    resting_order.get_trader_index(),
                    should_update_base_a(use_a_tree, false),
                    true,
                    resting_order.get_collateral_shares(),
                )?;
            }
        }
        remove_order_from_tree_and_free(fixed, dynamic, use_a_tree, order_index, is_bid)?;

        Ok(())
    }
}

fn set_payload_order(dynamic: &mut [u8], free_address: DataIndex) {
    get_mut_helper_order(dynamic, free_address)
        .set_payload_type(MarketDataTreeNodeType::RestingOrder as u8);
}
fn remove_order_from_tree(
    fixed: &mut MarketFixed,
    dynamic: &mut [u8],
    use_a_tree: bool,
    order_index: DataIndex,
    is_bid: bool,
) -> ProgramResult {
    if use_a_tree {
        let mut tree: Bookside = if is_bid {
            Bookside::new(
                dynamic,
                fixed.base_a_bids_root_index,
                fixed.base_a_bids_best_index,
            )
        } else {
            Bookside::new(
                dynamic,
                fixed.base_a_asks_root_index,
                fixed.base_a_asks_best_index,
            )
        };
        tree.remove_by_index(order_index);

        // Possibly changes the root and/or best.
        if is_bid {
            trace!(
                "remove order bid root:{}->{} max:{}->{}",
                fixed.base_a_bids_root_index,
                tree.get_root_index(),
                fixed.base_a_bids_best_index,
                tree.get_max_index()
            );
            fixed.base_a_bids_root_index = tree.get_root_index();
            fixed.base_a_bids_best_index = tree.get_max_index();
        } else {
            trace!(
                "remove order ask root:{}->{} max:{}->{}",
                fixed.base_a_asks_root_index,
                tree.get_root_index(),
                fixed.base_a_asks_best_index,
                tree.get_max_index()
            );
            fixed.base_a_asks_root_index = tree.get_root_index();
            fixed.base_a_asks_best_index = tree.get_max_index();
        }
    } else {
        let mut tree: Bookside = if is_bid {
            Bookside::new(
                dynamic,
                fixed.base_b_bids_root_index,
                fixed.base_b_bids_best_index,
            )
        } else {
            Bookside::new(
                dynamic,
                fixed.base_b_asks_root_index,
                fixed.base_b_asks_best_index,
            )
        };
        tree.remove_by_index(order_index);

        // Possibly changes the root and/or best.
        if is_bid {
            trace!(
                "remove order bid root:{}->{} max:{}->{}",
                fixed.base_b_bids_root_index,
                tree.get_root_index(),
                fixed.base_b_bids_best_index,
                tree.get_max_index()
            );
            fixed.base_b_bids_root_index = tree.get_root_index();
            fixed.base_b_bids_best_index = tree.get_max_index();
        } else {
            trace!(
                "remove order ask root:{}->{} max:{}->{}",
                fixed.base_b_asks_root_index,
                tree.get_root_index(),
                fixed.base_b_asks_best_index,
                tree.get_max_index()
            );
            fixed.base_b_asks_root_index = tree.get_root_index();
            fixed.base_b_asks_best_index = tree.get_max_index();
        }
    }

    Ok(())
}

fn remove_order_from_tree_and_free(
    fixed: &mut MarketFixed,
    dynamic: &mut [u8],
    use_a_tree: bool,
    order_index: DataIndex,
    is_bid: bool,
) -> ProgramResult {
    remove_order_from_tree(fixed, dynamic, use_a_tree, order_index, is_bid)?;
    if is_bid {
        release_address_on_market_fixed_for_bid_order(fixed, dynamic, order_index);
    } else {
        release_address_on_market_fixed_for_ask_order(fixed, dynamic, order_index);
    }
    Ok(())
}
#[allow(unused_variables)]
pub fn update_balance(
    fixed: &mut MarketFixed,
    dynamic: &mut [u8],
    trader_index: DataIndex,
    update_base_a: bool,
    is_increase: bool,
    asset_shares: WrappedI80F48,
) -> ProgramResult {
    let claimed_seat: &mut ClaimedSeat = get_mut_helper_seat(dynamic, trader_index).get_mut_value();

    trace!("update_balance_by_trader_index idx:{trader_index} base:{is_base} inc:{is_increase} amount:{asset_shares}");

    let asset_shares: I80F48 = asset_shares.into();

    if update_base_a {
        if is_increase {
            claimed_seat.base_a_withdrawable_asset_share = WrappedI80F48::from(
                I80F48::from(claimed_seat.base_a_withdrawable_asset_share) + asset_shares,
            );
        } else {
            require!(
                I80F48::from(claimed_seat.base_a_withdrawable_asset_share) >= asset_shares,
                ProgramError::InsufficientFunds,
                "Not enough base a withdrawable asset shares. Has {}, needs {}",
                I80F48::from(claimed_seat.base_a_withdrawable_asset_share),
                asset_shares
            )?;
            claimed_seat.base_a_withdrawable_asset_share = WrappedI80F48::from(
                I80F48::from(claimed_seat.base_a_withdrawable_asset_share) - asset_shares,
            );
        }
    } else if is_increase {
        claimed_seat.base_b_withdrawable_asset_share = WrappedI80F48::from(
            I80F48::from(claimed_seat.base_b_withdrawable_asset_share) + asset_shares,
        );
    } else {
        require!(
            I80F48::from(claimed_seat.base_b_withdrawable_asset_share) >= asset_shares,
            ProgramError::InsufficientFunds,
            "Not enough base b withdrawable asset shares. Has {}, needs {}",
            I80F48::from(claimed_seat.base_b_withdrawable_asset_share),
            asset_shares
        )?;
        claimed_seat.base_b_withdrawable_asset_share = WrappedI80F48::from(
            I80F48::from(claimed_seat.base_b_withdrawable_asset_share) - asset_shares,
        );
    }
    Ok(())
}

fn record_volume_by_trader_index(
    dynamic: &mut [u8],
    trader_index: DataIndex,
    amount_atoms: I80F48,
    use_a_tree: bool,
) {
    let claimed_seat: &mut ClaimedSeat = get_mut_helper_seat(dynamic, trader_index).get_mut_value();
    if use_a_tree {
        claimed_seat.base_a_volume = I80F48::from(claimed_seat.base_a_volume)
            .wrapping_add(amount_atoms)
            .into();
    } else {
        claimed_seat.base_b_volume = I80F48::from(claimed_seat.base_b_volume)
            .wrapping_add(amount_atoms)
            .into();
    }
}
#[inline(always)]
fn insert_order_into_tree(
    use_a_tree: bool,
    is_bid: bool,
    fixed: &mut MarketFixed,
    dynamic: &mut [u8],
    free_address: DataIndex,
    resting_order: &RestingOrder,
) {
    if use_a_tree {
        let mut tree: Bookside = if is_bid {
            Bookside::new(
                dynamic,
                fixed.base_a_bids_root_index,
                fixed.base_a_bids_best_index,
            )
        } else {
            Bookside::new(
                dynamic,
                fixed.base_a_asks_root_index,
                fixed.base_a_asks_best_index,
            )
        };
        tree.insert(free_address, *resting_order);

        if is_bid {
            trace!(
                "insert order bid {resting_order:?} root:{}->{} max:{}->{}->{}",
                fixed.bids_root_index,
                tree.get_root_index(),
                fixed.bids_best_index,
                tree.get_max_index(),
                tree.get_next_lower_index::<RestingOrder>(tree.get_max_index()),
            );
            fixed.base_a_bids_root_index = tree.get_root_index();
            fixed.base_a_bids_best_index = tree.get_max_index();
        } else {
            trace!(
                "insert order ask {resting_order:?} root:{}->{} max:{}->{}->{}",
                fixed.asks_root_index,
                tree.get_root_index(),
                fixed.asks_best_index,
                tree.get_max_index(),
                tree.get_next_lower_index::<RestingOrder>(tree.get_max_index()),
            );
            fixed.base_a_asks_root_index = tree.get_root_index();
            fixed.base_a_asks_best_index = tree.get_max_index();
        }
    } else {
        let mut tree: Bookside = if is_bid {
            Bookside::new(
                dynamic,
                fixed.base_b_bids_root_index,
                fixed.base_b_bids_best_index,
            )
        } else {
            Bookside::new(
                dynamic,
                fixed.base_b_asks_root_index,
                fixed.base_b_asks_best_index,
            )
        };
        tree.insert(free_address, *resting_order);

        if is_bid {
            trace!(
                "insert order bid {resting_order:?} root:{}->{} max:{}->{}->{}",
                fixed.bids_root_index,
                tree.get_root_index(),
                fixed.bids_best_index,
                tree.get_max_index(),
                tree.get_next_lower_index::<RestingOrder>(tree.get_max_index()),
            );
            fixed.base_b_bids_root_index = tree.get_root_index();
            fixed.base_b_bids_best_index = tree.get_max_index();
        } else {
            trace!(
                "insert order ask {resting_order:?} root:{}->{} max:{}->{}->{}",
                fixed.asks_root_index,
                tree.get_root_index(),
                fixed.asks_best_index,
                tree.get_max_index(),
                tree.get_next_lower_index::<RestingOrder>(tree.get_max_index()),
            );
            fixed.base_b_asks_root_index = tree.get_root_index();
            fixed.base_b_asks_best_index = tree.get_max_index();
        }
    }
}
fn get_next_candidate_match_index(
    dynamic: &[u8],
    current_maker_order_index: DataIndex,
    asks_root_index: DataIndex,
    asks_best_index: DataIndex,
    bids_root_index: DataIndex,
    bids_best_index: DataIndex,
    is_bid: bool,
) -> DataIndex {
    if is_bid {
        let tree: BooksideReadOnly =
            BooksideReadOnly::new(dynamic, asks_root_index, asks_best_index);
        let next_order_index: DataIndex =
            tree.get_next_lower_index::<RestingOrder>(current_maker_order_index);
        next_order_index
    } else {
        let tree: BooksideReadOnly =
            BooksideReadOnly::new(dynamic, bids_root_index, bids_best_index);
        let next_order_index: DataIndex =
            tree.get_next_lower_index::<RestingOrder>(current_maker_order_index);
        next_order_index
    }
}

fn remove_and_update_balances(
    fixed: &mut MarketFixed,
    dynamic: &mut [u8],
    use_a_tree: bool,
    order_to_remove_index: DataIndex,
    global_trade_accounts_opts: &[Option<GlobalTradeAccounts>; 2],
) -> ProgramResult {
    let resting_order_to_remove: &RestingOrder =
        get_helper_order(dynamic, order_to_remove_index).get_value();
    let order_to_remove_is_bid: bool = resting_order_to_remove.get_is_bid();

    // Global order balances are accounted for on the global accounts, not on the market.
    if resting_order_to_remove.is_global() {
        if order_to_remove_is_bid {
            return Err(NixError::InvalidGlobalBidOrder.into());
        } else {
            remove_from_global(&global_trade_accounts_opts[0])?;
        }
    } else {
        //return asset_shares only if resting_order is ask
        //if resting_order bid, create a new active loan (as asset_shares is already used as collateral to back up debt)

        // maker = 0
        if !order_to_remove_is_bid {
            let asset_shares_to_return = resting_order_to_remove.get_collateral_shares();
            update_balance(
                fixed,
                dynamic,
                resting_order_to_remove.get_trader_index(),
                should_update_base_a(use_a_tree, order_to_remove_is_bid),
                true,
                asset_shares_to_return,
            )?;
        };
    }
    remove_order_from_tree_and_free(
        fixed,
        dynamic,
        use_a_tree,
        order_to_remove_index,
        order_to_remove_is_bid,
    )?;
    Ok(())
}

fn get_tree_indexes(
    fixed: &mut MarketFixed,
    use_a_tree: bool,
) -> (DataIndex, DataIndex, DataIndex, DataIndex) {
    let (bids_best_index, asks_best_index, bids_root_index, asks_root_index) = if use_a_tree {
        (
            fixed.base_a_bids_best_index,
            fixed.base_a_asks_best_index,
            fixed.base_a_bids_root_index,
            fixed.base_a_asks_root_index,
        )
    } else {
        (
            fixed.base_b_bids_best_index,
            fixed.base_b_asks_best_index,
            fixed.base_b_bids_root_index,
            fixed.base_b_asks_root_index,
        )
    };
    (
        bids_best_index,
        asks_best_index,
        bids_root_index,
        asks_root_index,
    )
}

fn should_update_base_a(use_a_tree: bool, is_bid: bool) -> bool {
    // Determine which base asset to use based on tree type and order type
    // In A tree: bids use base B (quote), asks use base A (base)
    // In B tree: bids use base A (quote), asks use base B (base)
    let use_base_a = match (use_a_tree, is_bid) {
        (true, true) => false,   // A tree bid -> use base B
        (true, false) => true,   // A tree ask -> use base A
        (false, true) => true,   // B tree bid -> use base A
        (false, false) => false, // B tree ask -> use base B
    };
    use_base_a
}
