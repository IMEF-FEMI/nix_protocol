use std::{cell::Ref, ops::Deref};

use bytemuck::{Pod, Zeroable};
use hypertree::{get_helper, Get};
use marginfi::{
    constants::LIQUIDITY_VAULT_AUTHORITY_SEED, state::marginfi_group::Bank,
    ID as MARGINFI_PROGRAM_ID,
};

use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey, system_program,
};

use crate::{
    marginfi_utils::{
        MARGINFI_ACCOUNT_DISCRIMINATOR, MARGINFI_BANK_DISCRIMINATOR, MARGINFI_GROUP_DISCRIMINATOR,
    },
    program::NixError,
    require,
    validation::get_nix_marginfi_account_address,
};

use super::NixAccount;

/// Validation for Marginfi accounts.
#[derive(Clone)]
pub struct MarginfiAccountInfo<'a, 'info, T: Pod + Zeroable> {
    pub info: &'a AccountInfo<'info>,

    phantom: std::marker::PhantomData<T>,
}

impl<'a, 'info, T: Pod + Zeroable> MarginfiAccountInfo<'a, 'info, T> {
    pub fn new_group(
        info: &'a AccountInfo<'info>,
    ) -> Result<MarginfiAccountInfo<'a, 'info, T>, ProgramError> {
        validate_marginfi_group(info)?;
        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn new_bank(
        info: &'a AccountInfo<'info>,
    ) -> Result<MarginfiAccountInfo<'a, 'info, T>, ProgramError> {
        validate_marginfi_bank(info)?;
        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn new_account_uninitialized(
        info: &'a AccountInfo<'info>,
        market: &'a AccountInfo<'info>,
        mint: &'a AccountInfo<'info>,
    ) -> Result<MarginfiAccountInfo<'a, 'info, T>, ProgramError> {
        validate_marginfi_account_pda(info, market, mint)?;
        require!(
            info.data_is_empty(),
            ProgramError::InvalidAccountData,
            "Account must be uninitialized",
        )?;
        require!(
            info.owner == &system_program::id(),
            ProgramError::IllegalOwner,
            "Empty accounts must be owned by the system program",
        )?;
        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }
    pub fn new_account(
        info: &'a AccountInfo<'info>,
        market: &Pubkey,
        mint: &Pubkey,
    ) -> Result<MarginfiAccountInfo<'a, 'info, T>, ProgramError> {
        validate_marginfi_account(info, market, mint)?;
        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn get_fixed(&self) -> Result<Ref<'_, T>, ProgramError> {
        let data: Ref<&mut [u8]> = self.info.try_borrow_data()?;
        Ok(Ref::map(data, |data| {
            return bytemuck::from_bytes::<T>(&data[8..]);
        }))
    }
}

impl<'a, 'info, T: Pod + Zeroable> Deref for MarginfiAccountInfo<'a, 'info, T> {
    type Target = AccountInfo<'info>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}
pub fn validate_marginfi_group(account: &AccountInfo) -> ProgramResult {
    let data = account.try_borrow_data()?;

    require!(
        account.owner == &MARGINFI_PROGRAM_ID,
        // account.owner == &MARGINFI_PROGRAM_ID,
        NixError::InvalidMarginfiAccount,
        "Invalid Marginfi account owner: expected: {}, actual: {}",
        MARGINFI_PROGRAM_ID,
        account.owner
    )?;
    require!(
        &data[0..8] == MARGINFI_GROUP_DISCRIMINATOR,
        NixError::InvalidMarginfiAccount.into(),
        "Invalid Marginfi Account >> wrong Discriminator: expected: {:?}, actual: {:?}",
        MARGINFI_GROUP_DISCRIMINATOR,
        &data[0..8]
    )
}
pub fn validate_marginfi_bank(account: &AccountInfo) -> ProgramResult {
    let data = account.try_borrow_data()?;
    require!(
        account.owner == &MARGINFI_PROGRAM_ID,
        NixError::InvalidMarginfiAccount,
        "Invalid Marginfi account owner: expected: {}, actual: {}",
        MARGINFI_PROGRAM_ID,
        account.owner
    )?;
    require!(
        &data[0..8] == MARGINFI_BANK_DISCRIMINATOR,
        NixError::InvalidMarginfiAccount.into(),
        "Invalid Marginfi Account >> wrong Discriminator: expected: {:?}, actual: {:?}",
        MARGINFI_BANK_DISCRIMINATOR,
        &data[0..8]
    )
}

pub fn validate_marginfi_account(
    account: &AccountInfo,
    market: &Pubkey,
    mint: &Pubkey,
) -> ProgramResult {
    let (expected_pda, _bump) = get_nix_marginfi_account_address(market, mint);
    require!(
        account.key == &expected_pda,
        NixError::InvalidMarginfiAccount,
        "Invalid Marginfi Account >> wrong PDA: expected: {:?}, actual: {:?}",
        expected_pda,
        account.key
    )?;

    let data = account.try_borrow_data()?;
    require!(
        account.owner == &MARGINFI_PROGRAM_ID,
        NixError::InvalidMarginfiAccount,
        "Invalid Marginfi account owner: expected: {}, actual: {}",
        MARGINFI_PROGRAM_ID,
        account.owner
    )?;
    require!(
        &data[0..8] == MARGINFI_ACCOUNT_DISCRIMINATOR,
        NixError::InvalidMarginfiAccount.into(),
        "Invalid Marginfi Account >> wrong Discriminator: expected: {:?}, actual: {:?}",
        MARGINFI_ACCOUNT_DISCRIMINATOR,
        &data[0..8]
    )
}

pub fn validate_marginfi_account_pda(
    account: &AccountInfo,
    market: &AccountInfo,
    mint: &AccountInfo,
) -> ProgramResult {
    let (expected_pda, _bump) = get_nix_marginfi_account_address(market.key, mint.key);
    require!(
        account.key == &expected_pda,
        NixError::InvalidMarginfiAccount.into(),
        "Invalid Marginfi Account >> wrong PDA: expected: {:?}, actual: {:?}",
        expected_pda,
        account.key
    )
}

pub fn validate_marginfi_liquidity_vault(
    marginfi_liquidity_vault: &AccountInfo,
    bank: &AccountInfo,
) -> ProgramResult {
    let bank_data = bank.try_borrow_data()?;
    let marginfi_bank = bytemuck::from_bytes::<Bank>(&bank_data);
    require!(
        marginfi_liquidity_vault.key == &marginfi_bank.liquidity_vault,
        NixError::InvalidMarginfiLiquidityVault.into(),
        "Invalid Marginfi liquidity vault >> wrong PDA: expected: {:?}, actual: {:?}",
        marginfi_bank.liquidity_vault,
        marginfi_liquidity_vault.key
    )
}

pub fn validate_marginfi_liquidity_vault_authority(
    marginfi_liquidity_vault_authority: &AccountInfo,
    bank: &AccountInfo,
) -> ProgramResult {
    let (expected_pda, _bump) = get_marginfi_liquidity_vault_authority(bank.key);
    require!(
        marginfi_liquidity_vault_authority.key == &expected_pda,
        NixError::InvalidMarginfiLiquidityVault.into(),
        "Invalid Marginfi liquidity vault authority >> wrong PDA: expected: {:?}, actual: {:?}",
        expected_pda,
        marginfi_liquidity_vault_authority.key
    )
}

pub fn get_fixed<'a, T>(account: &'a AccountInfo) -> Result<Ref<'a, T>, ProgramError>
where
    T: NixAccount + Get + Clone,
{
    let data: Ref<'a, &mut [u8]> = account.try_borrow_data()?;
    Ok(Ref::map(data, |data| get_helper::<T>(data, 0_u32)))
}

pub fn get_marginfi_liquidity_vault_authority(bank: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[LIQUIDITY_VAULT_AUTHORITY_SEED.as_bytes(), bank.as_ref()],
        &MARGINFI_PROGRAM_ID,
    )
}
