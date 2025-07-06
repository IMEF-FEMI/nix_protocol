use bytemuck::Pod;
use hypertree::{get_helper, Get};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};
use std::{cell::Ref, mem::size_of, ops::Deref};

use crate::require;

/// Validation for Nix accounts.
#[derive(Clone)]
pub struct NixAccountInfo<'a, 'info, T: NixAccount + Pod + Clone> {
    pub info: &'a AccountInfo<'info>,

    phantom: std::marker::PhantomData<T>,
}

impl<'a, 'info, T: NixAccount + Get + Clone> NixAccountInfo<'a, 'info, T> {
    pub fn new(
        info: &'a AccountInfo<'info>,
    ) -> Result<NixAccountInfo<'a, 'info, T>, ProgramError> {
        verify_owned_by_nix(info.owner)?;

        let bytes: Ref<&mut [u8]> = info.try_borrow_data()?;
        let (header_bytes, _) = bytes.split_at(size_of::<T>());
        let header: &T = get_helper::<T>(header_bytes, 0_u32);
        header.verify_discriminant()?;

        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn new_init(
        info: &'a AccountInfo<'info>,
    ) -> Result<NixAccountInfo<'a, 'info, T>, ProgramError> {
        verify_owned_by_nix(info.owner)?;
        verify_uninitialized::<T>(info)?;
        Ok(Self {
            info,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn get_fixed(&self) -> Result<Ref<'_, T>, ProgramError> {
        let data: Ref<&mut [u8]> = self.info.try_borrow_data()?;
        Ok(Ref::map(data, |data| {
            return get_helper::<T>(data, 0_u32);
        }))
    }
}

impl<'a, 'info, T: NixAccount + Pod + Clone> Deref for NixAccountInfo<'a, 'info, T> {
    type Target = AccountInfo<'info>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}

impl<'a, 'info, T: NixAccount + Pod + Clone> AsRef<AccountInfo<'info>>
    for NixAccountInfo<'a, 'info, T>
{
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

pub trait NixAccount {
    fn verify_discriminant(&self) -> ProgramResult;
}

fn verify_owned_by_nix(owner: &Pubkey) -> ProgramResult {
    require!(
        owner == &crate::ID,
        ProgramError::IllegalOwner,
        "Account must be owned by the Nix program expected:{} actual:{}",
        crate::ID,
        owner
    )?;
    Ok(())
}

fn verify_uninitialized<T: Pod>(info: &AccountInfo) -> ProgramResult {
    let bytes: Ref<&mut [u8]> = info.try_borrow_data()?;
    require!(
        size_of::<T>() == bytes.len(),
        ProgramError::InvalidAccountData,
        "Incorrect length for uninitialized header expected: {} actual: {}",
        size_of::<T>(),
        bytes.len()
    )?;

    // This can't happen because for Market, we increase the size of the account
    // with a free block when it gets init, so the first check fails. For
    // global, we dont use new_init because the account is a PDA, so it is not
    // at an existing account. Keep the check for thoroughness in case a new
    // type is ever added.
    require!(
        bytes.iter().all(|&byte| byte == 0),
        ProgramError::InvalidAccountData,
        "Expected zeroed",
    )?;
    Ok(())
}

pub fn validate_init_market_account<'a, 'info, T: Pod + Clone>(
    info: &'a AccountInfo<'info>,
) -> ProgramResult {
    verify_owned_by_nix(info.owner)?;
    verify_uninitialized::<T>(info)
}

macro_rules! global_seeds {
    ( $mint:expr ) => {
        &[b"global", $mint.as_ref()]
    };
}

#[macro_export]
macro_rules! global_seeds_with_bump {
    ( $mint:expr, $bump:expr ) => {
        &[&[b"global", $mint.as_ref(), &[$bump]]]
    };
}
#[macro_export]
macro_rules! nix_marginfi_account_seeds {
    ($market:expr, $mint:expr) => {
        &[b"nix_marginfi_account", $market.as_ref(), $mint.as_ref()]
    };
}

#[macro_export]
macro_rules! nix_marginfi_account_seeds_with_bump {
    ( $market:expr, $mint:expr, $bump:expr ) => {
        &[&[
            b"nix_marginfi_account",
            $market.as_ref(),
            $mint.as_ref(),
            &[$bump],
        ]]
    };
}

pub fn get_nix_marginfi_account_address(market: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(nix_marginfi_account_seeds!(market, mint), &crate::ID)
}

pub fn get_global_address(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(global_seeds!(mint), &crate::ID)
}
