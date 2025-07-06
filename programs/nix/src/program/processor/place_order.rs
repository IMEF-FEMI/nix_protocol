use std::cell::RefMut;

use borsh::{BorshDeserialize, BorshSerialize};
use hypertree::{DataIndex, PodBool};
use marginfi::state::price::{OraclePriceType, PriceBias};
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult,
     pubkey::Pubkey, sysvar::Sysvar,
};

use crate::{
    logs::{emit_stack, PlaceOrderLog}, marginfi_utils::get_oracle_price, program::{expand_market_if_needed, expand_market_loans}, state::{AddOrderToMarketArgs, MarketLoansFixed, MarketRefMut, OrderType}, utils::{get_now_slot, try_to_add_new_loans}, validation::loaders::PlaceOrderContext
};

use super::{get_mut_dynamic_account, get_trader_index_with_hint};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct PlaceOrderParams {
    pub trader_index_hint: Option<DataIndex>,
    pub num_base_atoms: u64,
    pub rate_bps: u16,
    pub reverse_spread_bps: u16,
    pub is_bid: bool,
    pub use_a_tree: bool,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
}

pub fn process_place_order<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    data: &[u8],
) -> ProgramResult {
    let params: PlaceOrderParams = PlaceOrderParams::try_from_slice(data)?;
    process_place_order_core(program_id, accounts, params)
}

pub fn process_place_order_core<'a>(
    _program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    params: PlaceOrderParams,
) -> ProgramResult {
    let place_order_context: PlaceOrderContext =
        PlaceOrderContext::load(accounts, params.use_a_tree)?;
    let current_slot: Option<u32> = Some(get_now_slot());

    // Process the order directly without wrapper function
    let market_data: &mut RefMut<&mut [u8]> =
        &mut place_order_context.market.try_borrow_mut_data()?;
    let mut dynamic_account: MarketRefMut = get_mut_dynamic_account(market_data);
    let trader_index: DataIndex = get_trader_index_with_hint(
        params.trader_index_hint,
        &dynamic_account,
        &place_order_context.payer,
    )?;

    let (base_oracle_price_usd, quote_oracle_price_usd) = {
        let base_marginfi_bank_fixed = place_order_context.marginfi_cpi_accounts_opts[0]
            .as_ref()
            .unwrap()
            .marginfi_bank
            .get_fixed()
            .unwrap();
        let clock = Clock::get()?;

        let base_oracle_price_usd = get_oracle_price(
            accounts,
            &base_marginfi_bank_fixed.config,
            &clock,
            Some(PriceBias::Low),
            OraclePriceType::TimeWeighted,
        )?;
        let quote_marginfi_bank_fixed = place_order_context.marginfi_cpi_accounts_opts[1]
            .as_ref()
            .unwrap()
            .marginfi_bank
            .get_fixed()
            .unwrap();
        let clock = Clock::get()?;

        let quote_oracle_price_usd = get_oracle_price(
            accounts,
            &quote_marginfi_bank_fixed.config,
            &clock,
            Some(PriceBias::Low),
            OraclePriceType::TimeWeighted,
        )?;

        (base_oracle_price_usd, quote_oracle_price_usd)
    };

    let args = AddOrderToMarketArgs {
        market: *place_order_context.market.key,
        market_signer: place_order_context.market_signer.clone(),
        market_signer_bump: place_order_context.market_signer.bump,
        trader_index,
        num_base_atoms: params.num_base_atoms,
        rate_bps: params.rate_bps,
        reverse_spread_bps: params.reverse_spread_bps,
        is_bid: params.is_bid,
        use_a_tree: params.use_a_tree,
        last_valid_slot: params.last_valid_slot,
        order_type: params.order_type,
        base_mint: place_order_context.base_mint.clone(),
        quote_mint: place_order_context.quote_mint.clone(),
        base_oracle_price_usd,
        quote_oracle_price_usd,
        global_trade_accounts_opts: place_order_context.global_trade_accounts_opts,
        marginfi_cpi_accounts_opts: place_order_context.marginfi_cpi_accounts_opts,
        current_slot,
    };

    let res = dynamic_account.place_order(args,accounts)?;
    emit_stack(PlaceOrderLog {
        market: *place_order_context.market.key,
        trader: *place_order_context.payer.key,
        base_atoms:res.base_atoms_traded,
        rate_bps:params.rate_bps,
        order_type:params.order_type,
        is_bid: PodBool::from(params.is_bid),
        _padding: [0; 6],
        order_sequence_number:res.order_sequence_number,
        order_index:res.order_index,
        last_valid_slot:params.last_valid_slot,
        _padding1: [0; 6],
    })?;

    expand_market_if_needed(&place_order_context.payer, &place_order_context.market)?;
    //expand markets loans
    let matched_loans = res.matched_loans;
    expand_market_loans::<MarketLoansFixed>(&place_order_context.payer, &place_order_context.market_loans, matched_loans.len() as u32,)?;
    // insert new loans
    try_to_add_new_loans(&place_order_context.market_loans, matched_loans)?;
    Ok(())
}
