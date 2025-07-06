use bytemuck::Pod;
use hypertree::{get_helper, get_mut_helper, DataIndex, Get, RBNode};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    program_error::ProgramError, sysvar::Sysvar,
};
use std::{
    cell::{Ref, RefMut},
    mem::size_of,
};

use crate::{
    require,
    state::{
        market_loan::MarketLoansFixed, ClaimedSeat, DynamicAccount, GlobalFixed, MarketDataTreeNodeType, MarketFixed, MarketRefMut, GLOBAL_BLOCK_SIZE, MARKET_BLOCK_SIZE, MARKET_LOAN_BLOCK_SIZE
    },
    validation::{NixAccount, NixAccountInfo, Signer},
};
pub(crate) fn expand_market_loans_if_needed<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    market_loans_account_info: &'a AccountInfo<'info>,
    n: u32,
) -> ProgramResult {
    let need_expand: bool = {
        let market_loans_data: Ref<&mut [u8]> = market_loans_account_info.try_borrow_data()?;
        let fixed: &MarketLoansFixed = get_helper::<MarketLoansFixed>(&market_loans_data, 0_u32);
        !fixed.has_free_block()
    };

    if !need_expand {
        return Ok(());
    }
    expand_market_loans::<MarketLoansFixed>(payer, market_loans_account_info, n)
}


pub(crate) fn expand_market_loans<'a, 'info, T: NixAccount + Pod + Clone>(
    payer:  &'a AccountInfo<'info>,
    nix_account:  &'a AccountInfo<'info>,
    n: u32,
) -> ProgramResult {
    expand_dynamic(payer, nix_account, n as usize * MARKET_LOAN_BLOCK_SIZE)?;
    expand_market_loans_fixed(nix_account, n)?;
    Ok(())
}

pub(crate) fn expand_market_if_needed<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    market_account_info: &'a AccountInfo<'info>,
) -> ProgramResult {
    let need_expand: bool = {
        let market_data: Ref<&mut [u8]> = market_account_info.try_borrow_data()?;
        let fixed: &MarketFixed = get_helper::<MarketFixed>(&market_data, 0_u32);
        !fixed.has_free_block()
    };

    if !need_expand {
        return Ok(());
    }
    expand_market::<MarketFixed>(payer, market_account_info)
}
pub(crate) fn expand_market<'a, 'info, T: Clone>(
    payer: &'a AccountInfo<'info>,
    nix_account: &'a AccountInfo<'info>,
) -> ProgramResult {
    expand_dynamic(payer, nix_account, MARKET_BLOCK_SIZE)?;
    expand_market_fixed(nix_account)?;
    Ok(())
}

fn expand_dynamic<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    expandable_account: &'a AccountInfo<'info>,
    block_size: usize,
) -> ProgramResult {
    // Account types were already validated, so do not need to reverify that the
    // accounts are in order: payer, expandable_account, ...

    let new_size: usize = expandable_account.data_len() + block_size;

    let rent: solana_program::rent::Rent = solana_program::rent::Rent::get()?;
    let new_minimum_balance: u64 = rent.minimum_balance(new_size);
    let old_minimum_balance: u64 = rent.minimum_balance(expandable_account.data_len());
    let lamports_diff: u64 = new_minimum_balance.saturating_sub(old_minimum_balance);

    invoke(
        &solana_program::system_instruction::transfer(
            payer.key,
            expandable_account.key,
            lamports_diff,
        ),
        &[payer.clone(), expandable_account.clone()],
    )?;

    #[cfg(feature = "fuzz")]
    {
        solana_program::program::invoke(
            &solana_program::system_instruction::allocate(expandable_account.key, new_size as u64),
            &[expandable_account.clone()],
        )?;
    }
    #[cfg(not(feature = "fuzz"))]
    {
        expandable_account.realloc(new_size, false)?;
    }
    Ok(())
}
fn expand_market_fixed(expandable_account: &AccountInfo) -> ProgramResult {
    let market_data: &mut RefMut<&mut [u8]> = &mut expandable_account.try_borrow_mut_data()?;
    let mut dynamic_account: DynamicAccount<&mut MarketFixed, &mut [u8]> =
        get_mut_dynamic_account(market_data);
    dynamic_account.market_expand()?;
    Ok(())
}
/// Generic get mutable dynamic account from the data bytes of the account.
pub fn get_mut_dynamic_account<'a, T: Get>(
    data: &'a mut RefMut<'_, &mut [u8]>,
) -> DynamicAccount<&'a mut T, &'a mut [u8]> {
    let (fixed_data, dynamic) = data.split_at_mut(size_of::<T>());
    let fixed: &mut T = get_mut_helper::<T>(fixed_data, 0_u32);

    let dynamic_account: DynamicAccount<&'a mut T, &'a mut [u8]> =
        DynamicAccount { fixed, dynamic };
    dynamic_account
}

pub fn invoke(ix: &Instruction, account_infos: &[AccountInfo<'_>]) -> ProgramResult {
    #[cfg(target_os = "solana")]
    {
        solana_invoke::invoke_unchecked(ix, account_infos)
    }
    #[cfg(not(target_os = "solana"))]
    {
        solana_program::program::invoke(ix, account_infos)
    }
}

// Uses a MarketRefMut instead of a MarketRef because callers will have mutable data.
pub(crate) fn get_trader_index_with_hint(
    trader_index_hint: Option<DataIndex>,
    dynamic_account: &MarketRefMut,
    payer: &Signer,
) -> Result<DataIndex, ProgramError> {
    let trader_index: DataIndex = match trader_index_hint {
        None => dynamic_account.get_trader_index(payer.key),
        Some(hinted_index) => {
            verify_trader_index_hint(hinted_index, &dynamic_account, &payer)?;
            hinted_index
        }
    };
    Ok(trader_index)
}

fn verify_trader_index_hint(
    hinted_index: DataIndex,
    dynamic_account: &MarketRefMut,
    payer: &Signer,
) -> ProgramResult {
    require!(
        hinted_index % (MARKET_BLOCK_SIZE as DataIndex) == 0,
        crate::program::NixError::WrongIndexHintParams,
        "Invalid trader hint index {} did not align",
        hinted_index,
    )?;
    require!(
        get_helper::<RBNode<ClaimedSeat>>(&dynamic_account.dynamic, hinted_index)
            .get_payload_type()
            == MarketDataTreeNodeType::ClaimedSeat as u8,
        crate::program::NixError::WrongIndexHintParams,
        "Invalid trader hint index {} is not a ClaimedSeat",
        hinted_index,
    )?;
    require!(
        payer
            .key
            .eq(dynamic_account.get_trader_key_by_index(hinted_index)),
        crate::program::NixError::WrongIndexHintParams,
        "Invalid trader hint index {} did not match payer",
        hinted_index
    )?;
    Ok(())
}

fn expand_global_fixed(expandable_account: &AccountInfo) -> ProgramResult {
    let global_data: &mut RefMut<&mut [u8]> = &mut expandable_account.try_borrow_mut_data()?;
    let mut dynamic_account: DynamicAccount<&mut GlobalFixed, &mut [u8]> =
        get_mut_dynamic_account(global_data);
    dynamic_account.global_expand()?;
    Ok(())
}

fn expand_market_loans_fixed(expandable_account: &AccountInfo, n: u32) -> ProgramResult {
    let market_loans_data: &mut RefMut<&mut [u8]> = &mut expandable_account.try_borrow_mut_data()?;
    let mut dynamic_account: DynamicAccount<&mut MarketLoansFixed, &mut [u8]> =
        get_mut_dynamic_account(market_loans_data);
    dynamic_account.expand_loan_account(n)?;
    Ok(())
}
// Expand is always needed because global doesn't free bytes ever.
pub(crate) fn expand_global<'a, 'info, T: NixAccount + Pod + Clone>(
    payer: &Signer<'a, 'info>,
    nix_account: &NixAccountInfo<'a, 'info, T>,
) -> ProgramResult {
    // Expand twice because of two trees at once.
    expand_dynamic(payer, nix_account, GLOBAL_BLOCK_SIZE)?;
    expand_dynamic(payer, nix_account, GLOBAL_BLOCK_SIZE)?;
    expand_global_fixed(nix_account.info)?;
    Ok(())
}
 
