use std::cell::RefMut;

use borsh::{BorshDeserialize, BorshSerialize};
use hypertree::{get_helper, DataIndex, RBNode};
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::{
    logs::{emit_stack, CancelOrderLog},
    program::{get_mut_dynamic_account, get_trader_index_with_hint},
    require,
    state::{MarketDataTreeNodeType, MarketRefMut, RestingOrder, MARKET_BLOCK_SIZE},
    validation::loaders::CancelOrderContext,
};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct CancelOrderParams {
    pub trader_index_hint: Option<DataIndex>,
    pub order_sequence_number: u64,
    pub order_index_hint: Option<DataIndex>,
    pub use_a_tree: bool,
}
pub fn process_cancel_order<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    data: &[u8],
) -> ProgramResult {
    let params: CancelOrderParams = CancelOrderParams::try_from_slice(data)?;
    process_cancel_order_core(program_id, accounts, params)
}

pub fn process_cancel_order_core<'a>(
    _program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    params: CancelOrderParams,
) -> ProgramResult {
    let CancelOrderParams {
        trader_index_hint,
        order_sequence_number,
        order_index_hint,
        use_a_tree,
    } = params;
    let cancel_order_context: CancelOrderContext = CancelOrderContext::load(accounts, use_a_tree)?;

    let CancelOrderContext {
        payer,
        market,
        market_loans,
        base_global,
        system_program,
        ..
    } = cancel_order_context;

    let market_data: &mut RefMut<&mut [u8]> = &mut market.try_borrow_mut_data()?;

    let mut dynamic_account: MarketRefMut = get_mut_dynamic_account(market_data);
    let trader_index: DataIndex =
        get_trader_index_with_hint(trader_index_hint, &dynamic_account, &payer)?;

    match order_index_hint {
        None => {
            dynamic_account.cancel_order(
                use_a_tree,
                trader_index,
                order_sequence_number,
                &base_global,
                payer.clone(),
                system_program,
                &market_loans,
            )?;
        }
        Some(hinted_cancel_index) => {
            // Simple sanity check on the hint given. Make sure that it
            // aligns with block boundaries. We do a check that it is an
            // order owned by the payer inside the handler.
            require!(
                hinted_cancel_index % (MARKET_BLOCK_SIZE as DataIndex) == 0,
                crate::program::NixError::WrongIndexHintParams,
                "Invalid cancel hint index {}",
                hinted_cancel_index,
            )?;

            require!(
                get_helper::<RBNode<RestingOrder>>(&dynamic_account.dynamic, hinted_cancel_index,)
                    .get_payload_type()
                    == MarketDataTreeNodeType::RestingOrder as u8,
                crate::program::NixError::WrongIndexHintParams,
                "Invalid cancel hint index {}",
                hinted_cancel_index,
            )?;

            let order: &RestingOrder = dynamic_account.get_order_by_index(hinted_cancel_index);
            require!(
                trader_index == order.get_trader_index(),
                crate::program::NixError::WrongIndexHintParams,
                "Invalid cancel hint index {}",
                hinted_cancel_index,
            )?;
            require!(
                order_sequence_number == order.get_sequence_number(),
                crate::program::NixError::WrongIndexHintParams,
                "Invalid cancel hint sequence number index {}",
                hinted_cancel_index,
            )?;
            dynamic_account.cancel_order_by_index(
                use_a_tree,
                hinted_cancel_index,
                &base_global,
                &Some(payer.clone()),
                &Some(system_program),
                &market_loans,
            )?;
        }
    };
    emit_stack(CancelOrderLog {
        market: *market.key,
        trader: *payer.key,
        order_sequence_number,
    })?;
    Ok(())
}
