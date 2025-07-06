use std::cell::RefMut;

use fixed::types::I80F48;
use hypertree::{DataIndex, NIL};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, keccak, program::invoke_signed,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, sysvar::Sysvar,
};
use spl_token_2022::{
    extension::{
        transfer_fee::TransferFeeConfig, transfer_hook::TransferHook, BaseStateWithExtensions,
        StateWithExtensions,
    },
    state::Mint,
};

use crate::{
    global_vault_seeds_with_bump,
    logs::{emit_stack, GlobalCleanupLog},
    program::{get_mut_dynamic_account, invoke, NixError},
    require,
    state::{
        market_loan::{ActiveLoan, MarketLoansFixed, MarketLoansRefMut},
        order_type_can_take, GlobalFixed, GlobalRefMut, OrderType, RestingOrder,
        GAS_DEPOSIT_LAMPORTS, NO_EXPIRATION_LAST_VALID_SLOT,
    },
    validation::{
        loaders::GlobalTradeAccounts, MintAccountInfo, NixAccountInfo, Program, Signer,
        TokenAccountInfo, TokenProgram,
    },
};

/// Canonical discriminant of the given struct. It is the hash of program ID and
/// the name of the type.
pub fn get_discriminant<T>() -> Result<u64, ProgramError> {
    let type_name: &str = std::any::type_name::<T>();
    let discriminant: u64 = u64::from_le_bytes(
        keccak::hashv(&[crate::ID.as_ref(), type_name.as_bytes()]).as_ref()[..8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );
    Ok(discriminant)
}

/// Send CPI for creating a new account on chain.
pub fn create_account<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    new_account: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
    program_owner: &Pubkey,
    rent: &Rent,
    space: u64,
    seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            new_account.key,
            rent.minimum_balance(space as usize),
            space,
            program_owner,
        ),
        &[payer.clone(), new_account.clone(), system_program.clone()],
        &[seeds
            .iter()
            .map(|seed| seed.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_slice()],
    )
}
pub fn get_now_slot() -> u32 {
    // If we cannot get the clock (happens in tests, then only match with
    // orders without expiration). We assume that the clock cannot be
    // maliciously manipulated to clear all orders with expirations on the
    // orderbook.
    #[cfg(feature = "no-clock")]
    let now_slot: u64 = 0;
    #[cfg(not(feature = "no-clock"))]
    let now_slot: u64 = solana_program::clock::Clock::get()
        .unwrap_or(solana_program::clock::Clock {
            slot: u64::MAX,
            epoch_start_timestamp: i64::MAX,
            epoch: u64::MAX,
            leader_schedule_epoch: u64::MAX,
            unix_timestamp: i64::MAX,
        })
        .slot;
    now_slot as u32
}

pub fn get_now_unix_timestamp() -> i64 {
    // If we cannot get the clock (happens in tests, then only match with
    // orders without expiration). We assume that the clock cannot be
    // maliciously manipulated to clear all orders with expirations on the
    // orderbook.
    #[cfg(feature = "no-clock")]
    let now_unix_timestamp: i64 = 0;
    #[cfg(not(feature = "no-clock"))]
    let now_timestamp = solana_program::clock::Clock::get()
        .unwrap_or(solana_program::clock::Clock {
            slot: u64::MAX,
            epoch_start_timestamp: i64::MAX,
            epoch: u64::MAX,
            leader_schedule_epoch: u64::MAX,
            unix_timestamp: i64::MAX,
        })
        .unix_timestamp;
    now_timestamp
}
pub(crate) fn get_now_epoch() -> u64 {
    #[cfg(feature = "no-clock")]
    let now_epoch: u64 = 0;
    #[cfg(not(feature = "no-clock"))]
    let now_epoch: u64 = solana_program::clock::Clock::get()
        .unwrap_or(solana_program::clock::Clock {
            slot: u64::MAX,
            epoch_start_timestamp: i64::MAX,
            epoch: u64::MAX,
            leader_schedule_epoch: u64::MAX,
            unix_timestamp: i64::MAX,
        })
        .slot;
    now_epoch
}
pub(crate) fn assert_can_take(order_type: OrderType) -> ProgramResult {
    require!(
        order_type_can_take(order_type),
        crate::program::NixError::PostOnlyCrosses,
        "Post only order would cross",
    )?;
    Ok(())
}

pub(crate) fn remove_from_global(
    global_trade_accounts_opt: &Option<GlobalTradeAccounts>,
) -> ProgramResult {
    if global_trade_accounts_opt.is_none() {
        // Payer is forfeiting the right to claim the gas prepayment. This
        // results in a stranded gas prepayment on the global account.
        return Ok(());
    }
    let global_trade_accounts: &GlobalTradeAccounts = &global_trade_accounts_opt.as_ref().unwrap();
    let GlobalTradeAccounts {
        global,
        gas_receiver_opt,
        ..
    } = global_trade_accounts;

    remove_from_global_core(
        global,
        gas_receiver_opt,
        &global_trade_accounts.system_program,
    )
}

pub(crate) fn remove_from_global_core<'a, 'info>(
    global: &NixAccountInfo<'a, 'info, GlobalFixed>,
    gas_receiver_opt: &Option<Signer<'a, 'info>>,
    system_program: &Option<Program<'a, 'info>>,
) -> ProgramResult {
    if system_program.is_some() {
        **global.lamports.borrow_mut() -= GAS_DEPOSIT_LAMPORTS;
        **gas_receiver_opt.as_ref().unwrap().lamports.borrow_mut() += GAS_DEPOSIT_LAMPORTS;
    }
    Ok(())
}
pub(crate) fn try_to_add_new_loans<'a, 'info>(
    market_loans_account: &NixAccountInfo<'a, 'info, MarketLoansFixed>,
    matched_loans: Vec<ActiveLoan>,
) -> ProgramResult {
    let market_loans_data: &mut RefMut<&mut [u8]> =
        &mut market_loans_account.try_borrow_mut_data()?;
    let mut market_loans_dynamic_account: MarketLoansRefMut =
        get_mut_dynamic_account(market_loans_data);
    market_loans_dynamic_account.add_loans(&matched_loans)?;
    Ok(())
}

pub(crate) fn try_to_add_to_global(
    global_trade_accounts: &GlobalTradeAccounts,
    resting_order: &RestingOrder,
) -> ProgramResult {
    let GlobalTradeAccounts {
        global,
        gas_payer_opt,
        ..
    } = global_trade_accounts;

    {
        let global_data: &mut RefMut<&mut [u8]> = &mut global.try_borrow_mut_data()?;
        let mut global_dynamic_account: GlobalRefMut = get_mut_dynamic_account(global_data);
        global_dynamic_account.add_order(resting_order, gas_payer_opt.as_ref().unwrap().key)?;
    }

    // Need to CPI because otherwise we get:
    //
    // instruction spent from the balance of an account it does not own
    //
    // Done here instead of inside the object because the borrow checker needs
    // to get the data on global which it cannot while there is a mut self
    // reference.
    invoke(
        &solana_program::system_instruction::transfer(
            &gas_payer_opt.as_ref().unwrap().info.key,
            &global.key,
            GAS_DEPOSIT_LAMPORTS,
        ),
        &[
            gas_payer_opt.as_ref().unwrap().info.clone(),
            global.info.clone(),
        ],
    )?;

    Ok(())
}

pub(crate) fn assert_not_already_expired(last_valid_slot: u32, now_slot: u32) -> ProgramResult {
    require!(
        last_valid_slot == NO_EXPIRATION_LAST_VALID_SLOT || last_valid_slot > now_slot,
        crate::program::NixError::AlreadyExpired,
        "Placing an already expired order. now: {} last_valid: {}",
        now_slot,
        last_valid_slot
    )?;
    Ok(())
}

pub(crate) fn assert_valid_order_type(order_type: OrderType, is_bid: bool) -> ProgramResult {
    if is_bid && order_type == OrderType::Global {
        return Err(NixError::InvalidGlobalBidOrder.into());
    }
    if !is_bid && order_type == OrderType::Reverse {
        return Err(NixError::InvalidAskReverseOrder.into());
    }
    Ok(())
}
pub(crate) fn assert_already_has_seat(trader_index: DataIndex) -> ProgramResult {
    require!(
        trader_index != NIL,
        crate::program::NixError::AlreadyClaimedSeat,
        "Need to claim a seat first",
    )?;
    Ok(())
}

pub(crate) fn try_to_move_global_tokens<'a, 'info>(
    global_trade_accounts_opt: &'a Option<GlobalTradeAccounts<'a, 'info>>,
    mint: &'a MintAccountInfo<'a, 'info>,
    resting_order_trader: &Pubkey,
    desired_global_atoms: u64,
) -> Result<bool, ProgramError> {
    require!(
        global_trade_accounts_opt.is_some(),
        crate::program::NixError::MissingGlobal,
        "Missing global accounts when adding a global",
    )?;
    let global_trade_accounts: &GlobalTradeAccounts = &global_trade_accounts_opt.as_ref().unwrap();
    let GlobalTradeAccounts {
        global,
        global_vault_opt,
        gas_receiver_opt,
        market_vault_opt,
        token_program_opt,
        ..
    } = global_trade_accounts;

    let global_data: &mut RefMut<&mut [u8]> = &mut global.try_borrow_mut_data()?;
    let mut global_dynamic_account: GlobalRefMut = get_mut_dynamic_account(global_data);

    let num_deposited_atoms: I80F48 = global_dynamic_account
        .get_balance_atoms(resting_order_trader)
        .into();
    // Intentionally does not allow partial fills against a global order. The
    // reason for this is to punish global orders that are not backed. There is
    // no technical blocker for supporting partial fills against a global. It is
    // just because of the mechanism design where we want global to only be used
    // when needed, not just for all orders.
    let desired_global_atoms_i80f48: I80F48 = I80F48::from(desired_global_atoms);
    if desired_global_atoms_i80f48 > num_deposited_atoms {
        emit_stack(GlobalCleanupLog {
            cleaner: *gas_receiver_opt.as_ref().unwrap().key,
            maker: *resting_order_trader,
            amount_desired: desired_global_atoms,
            amount_deposited: num_deposited_atoms.to_num::<u64>(),
        })?;
        return Ok(false);
    }

    // Update the GlobalTrader
    global_dynamic_account.reduce(resting_order_trader, desired_global_atoms_i80f48)?;

    let mint_key: &Pubkey = global_dynamic_account.fixed.get_mint();

    let global_vault_bump: u8 = global_dynamic_account.fixed.get_vault_bump();

    let global_vault: &TokenAccountInfo<'a, 'info> = global_vault_opt.as_ref().unwrap();
    let market_vault: &TokenAccountInfo<'a, 'info> = market_vault_opt.as_ref().unwrap();
    let token_program: &TokenProgram<'a, 'info> = token_program_opt.as_ref().unwrap();

    if *token_program.key == spl_token_2022::id() {
        // Prevent transfer from global to market vault if a token has a non-zero fee.
        let mint_account_info: &MintAccountInfo = &mint;
        if StateWithExtensions::<Mint>::unpack(&mint_account_info.info.data.borrow())?
            .get_extension::<TransferFeeConfig>()
            .is_ok_and(|f| f.get_epoch_fee(get_now_epoch()).transfer_fee_basis_points != 0.into())
        {
            solana_program::msg!("Treating global order as unbacked because it has a transfer fee");
            return Ok(false);
        }
        if StateWithExtensions::<Mint>::unpack(&mint_account_info.info.data.borrow())?
            .get_extension::<TransferHook>()
            .is_ok_and(|f| f.program_id.0 != Pubkey::default())
        {
            solana_program::msg!(
                "Treating global order as unbacked because it has a transfer hook"
            );
            return Ok(false);
        }

        invoke_signed(
            &spl_token_2022::instruction::transfer_checked(
                token_program.key,
                global_vault.key,
                mint_account_info.info.key,
                market_vault.key,
                global_vault.key,
                &[],
                desired_global_atoms,
                mint_account_info.mint.decimals,
            )?,
            &[
                token_program.as_ref().clone(),
                global_vault.as_ref().clone(),
                mint_account_info.as_ref().clone(),
                market_vault.as_ref().clone(),
            ],
            global_vault_seeds_with_bump!(mint_key, global_vault_bump),
        )?;
    } else {
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key,
                global_vault.key,
                market_vault.key,
                global_vault.key,
                &[],
                desired_global_atoms,
            )?,
            &[
                token_program.as_ref().clone(),
                global_vault.as_ref().clone(),
                market_vault.as_ref().clone(),
            ],
            global_vault_seeds_with_bump!(mint_key, global_vault_bump),
        )?;
    }

    Ok(true)
}

#[test]
fn test_get_discriminant() {
    //TODO: Update this when updating program id.
    assert_eq!(
        get_discriminant::<crate::state::MarketFixed>().unwrap(),
        5986819525067784620
    );
}
