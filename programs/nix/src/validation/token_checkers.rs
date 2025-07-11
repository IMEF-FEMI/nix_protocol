use crate::{program::NixError, require, state::MarketFixed};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

use spl_token_2022::{
    check_spl_token_program_account, extension::StateWithExtensions, state::Mint,
};
use std::{cell::Ref, ops::Deref};

use super::get_fixed;

#[derive(Clone)]
pub struct MintAccountInfo<'a, 'info> {
    pub mint: Mint,
    pub info: &'a AccountInfo<'info>,
}

impl<'a, 'info> MintAccountInfo<'a, 'info> {
    pub fn new(info: &'a AccountInfo<'info>) -> Result<MintAccountInfo<'a, 'info>, ProgramError> {
        check_spl_token_program_account(info.owner)?;

        let mint: Mint = StateWithExtensions::<Mint>::unpack(&info.data.borrow())?.base;

        Ok(Self { mint, info })
    }
}

impl<'a, 'info> AsRef<AccountInfo<'info>> for MintAccountInfo<'a, 'info> {
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

#[derive(Clone)]
pub struct MarketSigner<'a, 'info> {
    pub bump: u8,
    pub info: &'a AccountInfo<'info>,
}

impl<'a, 'info> MarketSigner<'a, 'info> {
    pub fn new(
        info: &'a AccountInfo<'info>,
        market_key: &Pubkey,
    ) -> Result<MarketSigner<'a, 'info>, ProgramError> {
        let (expected_market_signer, bump) = get_market_signer_address(market_key);

        require!(
            expected_market_signer == *info.key,
            NixError::IncorrectAccount,
            "Incorrect market signer account",
        )?;
        Ok(Self { bump, info })
    }
}

impl<'a, 'info> AsRef<AccountInfo<'info>> for MarketSigner<'a, 'info> {
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

#[derive(Clone)]
pub struct TokenAccountInfo<'a, 'info> {
    pub info: &'a AccountInfo<'info>,
}

impl<'a, 'info> TokenAccountInfo<'a, 'info> {
    pub fn new(
        info: &'a AccountInfo<'info>,
        mint: &Pubkey,
    ) -> Result<TokenAccountInfo<'a, 'info>, ProgramError> {
        require!(
            info.owner == &spl_token::id() || info.owner == &spl_token_2022::id(),
            ProgramError::IllegalOwner,
            "Token account must be owned by the Token Program",
        )?;
        // The mint key is found at offset 0 of the token account
        require!(
            &info.try_borrow_data()?[0..32] == mint.as_ref(),
            ProgramError::InvalidAccountData,
            "Token account mint mismatch",
        )?;
        Ok(Self { info })
    }

    pub fn get_owner(&self) -> Pubkey {
        Pubkey::new_from_array(
            self.info.try_borrow_data().unwrap()[32..64]
                .try_into()
                .unwrap(),
        )
    }

    pub fn get_balance(&self) -> u64 {
        u64::from_le_bytes(
            self.info.try_borrow_data().unwrap()[64..72]
                .try_into()
                .unwrap(),
        )
    }

    pub fn new_with_owner(
        info: &'a AccountInfo<'info>,
        mint: &Pubkey,
        owner: &Pubkey,
    ) -> Result<TokenAccountInfo<'a, 'info>, ProgramError> {
        let token_account_info = Self::new(info, mint)?;
        // The owner key is found at offset 32 of the token account
        require!(
            &info.try_borrow_data()?[32..64] == owner.as_ref(),
            ProgramError::IllegalOwner,
            "Token account owner mismatch",
        )?;
        Ok(token_account_info)
    }

    pub fn new_with_owner_and_key(
        info: &'a AccountInfo<'info>,
        mint: &Pubkey,
        owner: &Pubkey,
        key: &Pubkey,
    ) -> Result<TokenAccountInfo<'a, 'info>, ProgramError> {
        require!(
            info.key == key,
            ProgramError::InvalidInstructionData,
            "Invalid pubkey for Token Account {:?}",
            info.key
        )?;
        Self::new_with_owner(info, mint, owner)
    }
}

impl<'a, 'info> AsRef<AccountInfo<'info>> for TokenAccountInfo<'a, 'info> {
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

impl<'a, 'info> Deref for TokenAccountInfo<'a, 'info> {
    type Target = AccountInfo<'info>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}

pub fn validate_market_mint(market: &AccountInfo, mint: &AccountInfo) -> ProgramResult {
    check_spl_token_program_account(mint.owner)?;
    let market_fixed: Ref<MarketFixed> = get_fixed::<MarketFixed>(market)?;
    let base_a_mint = market_fixed.get_base_a_mint();
    let base_b_mint = market_fixed.get_base_b_mint();
    require!(
        &mint.key == &base_a_mint || &mint.key == &base_b_mint,
        ProgramError::InvalidAccountData,
        "Mint does not match market mints",
    )
}

#[macro_export]
macro_rules! market_vault_seeds {
    ( $market:expr, $mint:expr ) => {
        &[b"vault", $market.as_ref(), $mint.as_ref()]
    };
}

#[macro_export]
macro_rules! market_vault_seeds_with_bump {
    ( $market:expr, $mint:expr, $bump:expr ) => {
        &[&[b"vault", $market.as_ref(), $mint.as_ref(), &[$bump]]]
    };
}

#[macro_export]
macro_rules! global_vault_seeds {
    ( $mint:expr ) => {
        &[b"global-vault", $mint.as_ref()]
    };
}

#[macro_export]
macro_rules! global_vault_seeds_with_bump {
    ( $mint:expr, $bump:expr ) => {
        &[&[b"global-vault", $mint.as_ref(), &[$bump]]]
    };
}
#[macro_export]
macro_rules! market_fee_receiver_seeds {
    ( $market:expr, $mint:expr ) => {
        &[b"fee-receiver", $market.as_ref(), $mint.as_ref()]
    };
}

#[macro_export]
macro_rules! market_fee_receiver_seeds_with_bump {
    ( $market:expr, $mint:expr, $bump:expr ) => {
        &[&[b"fee-receiver", $market.as_ref(), $mint.as_ref(), &[$bump]]]
    };
}

#[macro_export]
macro_rules! market_signer_seeds {
    ( $market:expr ) => {
        &[b"market-signer", $market.as_ref()]
    };
}

#[macro_export]
macro_rules! market_signer_seeds_with_bump {
    ( $market:expr, $bump:expr ) => {
        &[&[b"market-signer", $market.as_ref(), &[$bump]]]
    };
}

pub fn get_market_signer_address(market: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(market_signer_seeds!(market), &crate::ID)
}
pub fn get_market_fee_receiver_address(market: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(market_fee_receiver_seeds!(market, mint), &crate::ID)
}

pub fn get_vault_address(market: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(market_vault_seeds!(market, mint), &crate::ID)
}

pub fn get_global_vault_address(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(global_vault_seeds!(mint), &crate::ID)
}
