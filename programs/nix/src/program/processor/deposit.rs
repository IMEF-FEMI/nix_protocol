use std::cell::RefMut;

use borsh::{BorshDeserialize, BorshSerialize};
use fixed::types::I80F48;
use hypertree::DataIndex;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

use crate::{
    marginfi_utils::cpi_marginfi_deposit, market_signer_seeds_with_bump,  program::NixError, state::MarketRefMut, validation::{
        loaders::DepositContext, MintAccountInfo, Signer, TokenAccountInfo, TokenProgram,
    }
};

use super::{get_mut_dynamic_account, get_trader_index_with_hint};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct DepositParams {
    pub amount: u64,
    pub trader_index_hint: Option<DataIndex>,
}

impl DepositParams {
    pub fn new(amount: u64, trader_index_hint: Option<DataIndex>) -> Self {
        DepositParams {
            amount,
            trader_index_hint,
        }
    }
}

pub(crate) fn process_deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let params: DepositParams = DepositParams::try_from_slice(data)?;
    process_deposit_core(program_id, accounts, params)
}

pub(crate) fn process_deposit_core(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: DepositParams,
) -> ProgramResult {
    let DepositParams {
        amount,
        trader_index_hint,
    } = params;
    // Due to transfer fees, this might not be what you expect.
    let mut deposited_amount: u64 = amount;

    let deposit_context: DepositContext = DepositContext::load(accounts)?;
    let DepositContext {
        payer,
        market,
        market_signer,
        mint,
        trader_token_account,
        token_program,
        vault,
        marginfi_group,
        marginfi_bank,
        marginfi_account,
        marginfi_liquidity_vault,
    } = deposit_context;

    let market_data: &mut RefMut<&mut [u8]> = &mut market.try_borrow_mut_data()?;
    let mut dynamic_account: MarketRefMut = get_mut_dynamic_account(market_data);

    let is_base_a: bool = &trader_token_account.try_borrow_data()?[0..32]
        == dynamic_account.get_base_a_mint().as_ref();

    if *vault.owner == spl_token_2022::id() {
        let before_vault_balance: u64 = vault.get_balance();
        spl_token_2022_transfer_from_trader_to_vault(
            &token_program,
            &trader_token_account,
            Some(&mint),
            if is_base_a {
                dynamic_account.fixed.get_base_a_mint()
            } else {
                dynamic_account.get_base_b_mint()
            },
            &vault,
            &payer,
            amount,
            if is_base_a {
                dynamic_account.fixed.get_base_a_decimals()
            } else {
                dynamic_account.fixed.get_base_b_decimals()
            },
        )?;

        let after_vault_balance: u64 = vault.get_balance();
        deposited_amount = after_vault_balance
            .checked_sub(before_vault_balance)
            .unwrap();
    } else {
        spl_token_transfer_from_trader_to_vault(
            &token_program,
            &trader_token_account,
            &vault,
            &payer,
            amount,
        )?;
    }

    // Before CPI: Load MarginFiAccount data to get initial shares
    let mfi_account: std::cell::Ref<'_, marginfi::state::marginfi_account::MarginfiAccount> =
        marginfi_account.get_fixed()?;
    let balance_before_mfi_shares = mfi_account
        .lending_account
        .balances
        .iter()
        .find(|b| b.active != 0 && b.bank_pk == *marginfi_bank.key)
        .map(|b| I80F48::from(b.asset_shares))
        .unwrap_or_default();
    drop(mfi_account); // Release before CPI

    // Prepare mint option for CPI
    let mint_option = if *vault.owner == spl_token_2022::id() {
        Some(mint)
    } else {
        None
    };

    // deposit CPI to marginfi
    cpi_marginfi_deposit(
        &marginfi_group,
        &marginfi_account,
        &marginfi_bank,
        &marginfi_liquidity_vault,
        market_signer.clone(),
        &vault,
        &token_program,
        deposited_amount,
        None,
        &mint_option,
        market_signer_seeds_with_bump!(market.key, market_signer.bump),
    )?;

    // After CPI: Load MarginFiAccount data to get data again
    let mfi_account: std::cell::Ref<'_, marginfi::state::marginfi_account::MarginfiAccount> =
        marginfi_account.get_fixed()?;
    let balance_after_mfi_shares = mfi_account
        .lending_account
        .balances
        .iter()
        .find(|b| b.active != 0 && b.bank_pk == *marginfi_bank.key)
        .map(|b| I80F48::from(b.asset_shares))
        .unwrap_or_default();
    drop(mfi_account);

    let mfi_asset_shares_gained = balance_after_mfi_shares
        .checked_sub(balance_before_mfi_shares)
        .ok_or_else(|| NixError::NumericalOverflow)?;
    if mfi_asset_shares_gained < I80F48::ZERO {
        return Err(NixError::InvalidMarginfiState.into());
    }

    let trader_index: DataIndex =
        get_trader_index_with_hint(trader_index_hint, &dynamic_account, &payer)?;

    dynamic_account.deposit(trader_index, mfi_asset_shares_gained.into(), is_base_a)?;
    Ok(())
}

/** Transfer from base (quote) trader to base (quote) vault using SPL Token **/
fn spl_token_transfer_from_trader_to_vault<'a, 'info>(
    token_program: &TokenProgram<'a, 'info>,
    trader_account: &TokenAccountInfo<'a, 'info>,
    vault: &TokenAccountInfo<'a, 'info>,
    payer: &Signer<'a, 'info>,
    amount: u64,
) -> ProgramResult {
    crate::program::invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            trader_account.key,
            vault.key,
            payer.key,
            &[],
            amount,
        )?,
        &[
            token_program.as_ref().clone(),
            trader_account.as_ref().clone(),
            vault.as_ref().clone(),
            payer.as_ref().clone(),
        ],
    )
}

/** Transfer from base (quote) trader to base (quote) vault using SPL Token 2022 **/
fn spl_token_2022_transfer_from_trader_to_vault<'a, 'info>(
    token_program: &TokenProgram<'a, 'info>,
    trader_account: &TokenAccountInfo<'a, 'info>,
    mint: Option<&MintAccountInfo<'a, 'info>>,
    mint_pubkey: &Pubkey,
    vault: &TokenAccountInfo<'a, 'info>,
    payer: &Signer<'a, 'info>,
    amount: u64,
    decimals: u8,
) -> ProgramResult {
    crate::program::invoke(
        &spl_token_2022::instruction::transfer_checked(
            token_program.key,
            trader_account.key,
            mint_pubkey,
            vault.key,
            payer.key,
            &[],
            amount,
            decimals,
        )?,
        &[
            token_program.as_ref().clone(),
            trader_account.as_ref().clone(),
            vault.as_ref().clone(),
            mint.unwrap().as_ref().clone(),
            payer.as_ref().clone(),
        ],
    )
}
