use crate::{
    market_signer_seeds_with_bump,  program::NixError, require, state::MarketFixed, validation::{
         loaders::{GlobalTradeAccounts, MarginfiCpiAccounts},  MarginfiAccountInfo, MarketSigner, MintAccountInfo, NixAccountInfo, Program, Signer, TokenAccountInfo, TokenProgram
    }
};
use borsh::BorshSerialize;
use fixed::{ types::I80F48};
use hypertree::trace;
use marginfi::{
    constants::EXP_10_I80F48,
    prelude::MarginfiGroup,
    state::{
        marginfi_account::MarginfiAccount,
        marginfi_group::{Bank, BankConfig},
        price::{OraclePriceFeedAdapter, OraclePriceType, OracleSetup, PriceAdapter, PriceBias},
    },
    ID as MARGINFI_PROGRAM_ID,
};
use sha2::{Digest, Sha256};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    program_error::ProgramError,
};
use std::slice::Iter;

// https://github.com/mrgnlabs/mrgn-ts/blob/6fb11c9ed0547feb1048855cc960880b1d66f965/packages/marginfi-client-v2/src/idl/marginfi-types_0.1.0.ts#L108
pub const MARGINFI_GROUP_DISCRIMINATOR: [u8; 8] = [182, 23, 173, 240, 151, 206, 182, 67];
pub const MARGINFI_BANK_DISCRIMINATOR: [u8; 8] = [142, 49, 166, 242, 50, 66, 97, 188];
pub const MARGINFI_ACCOUNT_DISCRIMINATOR: [u8; 8] = [67, 178, 130, 109, 126, 114, 28, 42];
pub const MARGINFI_LENDING_ACCOUNT_DEPOSIT_DISCRIMINATOR: [u8; 8] =
    [171, 94, 235, 103, 82, 64, 212, 140];
pub const MARGINFI_LENDING_ACCOUNT_WITHDRAW_DISCRIMINATOR: [u8; 8] =
    [36, 72, 74, 19, 210, 210, 192, 192];
pub const MARGINFI_LENDING_ACCOUNT_BORROW_DISCRIMINATOR: [u8; 8] =
    [4, 126, 116, 53, 48, 5, 212, 31];
pub const MARGINFI_LENDING_ACCOUNT_REPAY_DISCRIMINATOR: [u8; 8] =
    [79, 209, 172, 177, 222, 51, 173, 151];
pub const MARGINFI_LENDING_ACCOUNT_WITHDRAW_EMISSION: [u8; 8] =
    [234, 22, 84, 214, 118, 176, 140, 170];
pub const MARGINFI_LENDING_ACCOUNT_SETTLE_EMISSION: [u8; 8] =
    [161, 58, 136, 174, 242, 223, 156, 176];
pub const MARGINFI_ACCOUNT_INITIALIZE_DISCRIMINATOR: [u8; 8] = [43, 78, 61, 255, 148, 52, 249, 154];

#[derive(BorshSerialize)]
pub struct MfiInitializeAccountData {}
#[derive(BorshSerialize)]
pub struct MfiLendingAccountDepositData {
    pub amount: u64,
    pub deposit_up_to_limit: Option<bool>,
}
#[derive(BorshSerialize)]
pub struct MfiLendingAccountWithdrawData {
    pub amount: u64,
    pub withdraw_all: Option<bool>,
}
#[derive(BorshSerialize)]
pub struct MfiLendingAccountBorrowData {
    pub amount: u64,
}
#[derive(BorshSerialize)]
pub struct MfiLendingAccountRepayData {
    pub amount: u64,
    pub repay_all: Option<bool>,
}

/// A minimal tool to convert a hex string like "22f123639" into the byte equivalent.
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|chunk| {
            let high = chunk[0] as char;
            let low = chunk[1] as char;
            let high = high.to_digit(16).expect("Invalid hex character") as u8;
            let low = low.to_digit(16).expect("Invalid hex character") as u8;
            (high << 4) | low
        })
        .collect()
}

pub fn compute_anchor_account_discriminator(struct_name: &str) -> [u8; 8] {
    let preimage = format!("account:{}", struct_name);
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    let hash = hasher.finalize();
    hash[0..8].try_into().unwrap()
}
pub fn compute_anchor_fn_discriminator(struct_name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", struct_name);
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    let hash = hasher.finalize();
    hash[0..8].try_into().unwrap()
}
#[test]
fn test_marginfi_discriminant() {
    assert_eq!(
        compute_anchor_account_discriminator("MarginfiGroup").to_vec(),
        MARGINFI_GROUP_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("marginfi_account_initialize").to_vec(),
        MARGINFI_ACCOUNT_INITIALIZE_DISCRIMINATOR
    );

    assert_eq!(
        compute_anchor_account_discriminator("Bank").to_vec(),
        MARGINFI_BANK_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_account_discriminator("MarginfiAccount").to_vec(),
        MARGINFI_ACCOUNT_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_deposit").to_vec(),
        MARGINFI_LENDING_ACCOUNT_DEPOSIT_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_withdraw").to_vec(),
        MARGINFI_LENDING_ACCOUNT_WITHDRAW_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_borrow").to_vec(),
        MARGINFI_LENDING_ACCOUNT_BORROW_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_repay").to_vec(),
        MARGINFI_LENDING_ACCOUNT_REPAY_DISCRIMINATOR
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_withdraw_emissions").to_vec(),
        MARGINFI_LENDING_ACCOUNT_WITHDRAW_EMISSION
    );
    assert_eq!(
        compute_anchor_fn_discriminator("lending_account_settle_emissions").to_vec(),
        MARGINFI_LENDING_ACCOUNT_SETTLE_EMISSION
    );
}

pub fn initialize_marginfi_account<'a, 'info>(
    marginfi_group: &'a MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    marginfi_account: &'a MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    admin: &'a Signer<'a, 'info>,
    system_program: &'a Program<'a, 'info>,
    market: &'a NixAccountInfo<'a, 'info, MarketFixed>,
    authority: &'a AccountInfo<'info>,
    authority_bump: u8,
) -> ProgramResult {


    invoke_signed(
        &Instruction {
            program_id: MARGINFI_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new_readonly(*marginfi_group.key, false),
                AccountMeta::new(*marginfi_account.as_ref().key, false),
                AccountMeta::new_readonly(*authority.as_ref().key, true), //authority
                AccountMeta::new(*admin.key, true),                       // fee payer (signer)
                AccountMeta::new_readonly(*system_program.as_ref().key, false),
            ],
            data: MARGINFI_ACCOUNT_INITIALIZE_DISCRIMINATOR.to_vec(),
        },
        &[
            marginfi_group.as_ref().clone(),
            marginfi_account.as_ref().clone(),
            authority.as_ref().clone(),
            admin.as_ref().clone(),
            system_program.as_ref().clone(),
        ],
        market_signer_seeds_with_bump!(market.key, authority_bump),
    )
    .map_err(|_| NixError::MarginfiCpiFailed)?;

    //account is expected to have been initialized in the marginfi program
    let marginfi_account_data = marginfi_account.try_borrow_data()?;
    let initialized_marginfi_account =
        *bytemuck::try_from_bytes::<MarginfiAccount>(&marginfi_account_data)
            .map_err(|_| ProgramError::InvalidAccountData)
            .unwrap();

    require!(
        initialized_marginfi_account
            .group
            .eq(marginfi_group.as_ref().key)
            && initialized_marginfi_account
                .authority
                .eq(marginfi_account.as_ref().key),
        NixError::MarginfiAccountInitializationFailed,
        "Marginfi account not initialized correctly",
    )?;
    Ok(())
}

// CPI to MarginFi: Deposit
pub fn cpi_marginfi_deposit<'a, 'info>(
    marginfi_group: &MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    marginfi_account: &MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    marginfi_bank: &MarginfiAccountInfo<'a, 'info, Bank>,
    marginfi_liquidity_vault: &TokenAccountInfo<'a, 'info>,
    authority: MarketSigner<'a,'info>,
    vault: &TokenAccountInfo<'a, 'info>,
    token_program: &TokenProgram<'a, 'info>,
    amount: u64,
    deposit_up_to_limit: Option<bool>,
    mint: &Option<MintAccountInfo<'a, 'info>>,
    authority_pda_seeds: &[&[&[u8]]],
) -> ProgramResult {
    trace!("CPI: MarginFi Deposit amount {}", amount);
    let ix_data_args = MfiLendingAccountDepositData {
        amount,
        deposit_up_to_limit,
    };
    let mut data_vec = MARGINFI_LENDING_ACCOUNT_DEPOSIT_DISCRIMINATOR.to_vec();
    data_vec.extend_from_slice(
        &ix_data_args
            .try_to_vec()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    let mut cpi_account_metas = vec![
        AccountMeta::new(*marginfi_group.key, false),
        AccountMeta::new(*marginfi_account.key, false),
        AccountMeta::new(*authority.as_ref().key, true),
        AccountMeta::new(*marginfi_bank.key, false),
        AccountMeta::new(*vault.key, false),
        AccountMeta::new(*marginfi_liquidity_vault.key, false),
        AccountMeta::new_readonly(*token_program.key, false),
    ];
    if let Some(mint_ai) = &mint {
        cpi_account_metas.push(AccountMeta::new_readonly(*mint_ai.as_ref().key, false));
        //add mint account for token 22 accounts
    }

    let instruction = Instruction {
        program_id: MARGINFI_PROGRAM_ID,
        accounts: cpi_account_metas,
        data: data_vec,
    };

    let mut cpi_account_infos = vec![
        marginfi_group.as_ref().clone(),
        marginfi_account.as_ref().clone(),
        authority.as_ref().clone(),
        marginfi_bank.as_ref().clone(),
        vault.as_ref().clone(),
        marginfi_liquidity_vault.as_ref().clone(),
        token_program.as_ref().clone(),
    ];

    // Add mint account info if provided
    if let Some(mint_ai) = &mint {
        cpi_account_infos.push(mint_ai.as_ref().clone());
    }

    invoke_signed(&instruction, &cpi_account_infos, authority_pda_seeds).map_err(|_e| {
        trace!("MarginFi Deposit CPI failed: {:?}", _e);
        NixError::MarginfiCpiFailed.into()
    })
}

// CPI to MarginFi: Deposit
pub fn cpi_marginfi_deposit_place_order<'a, 'info>(
    marginfi_cpi_accts: &MarginfiCpiAccounts<'a, 'info>,
    authority: MarketSigner<'a,'info>,
    source: &TokenAccountInfo<'a, 'info>,
    token_program: &TokenProgram<'a, 'info>,
    mint: Option<&MintAccountInfo<'a, 'info>>,
    authority_pda_seeds: &[&[&[u8]]],
) -> ProgramResult {
    trace!("CPI: MarginFi Deposit amount {}", amount);
    let ix_data_args = MfiLendingAccountDepositData {
        amount: source.get_balance(), //incoming vault
        deposit_up_to_limit: None,
    };
    let mut data_vec = MARGINFI_LENDING_ACCOUNT_DEPOSIT_DISCRIMINATOR.to_vec();
    data_vec.extend_from_slice(
        &ix_data_args
            .try_to_vec()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    let mut cpi_account_metas = vec![
        AccountMeta::new(*marginfi_cpi_accts.marginfi_group.key, false),
        AccountMeta::new(*marginfi_cpi_accts.marginfi_account.key, false),
        AccountMeta::new(*authority.as_ref().key, true), 
        AccountMeta::new(*marginfi_cpi_accts.marginfi_bank.key, false),
        AccountMeta::new(*source.key, false),
        AccountMeta::new(*marginfi_cpi_accts.marginfi_liquidity_vault.key, false),
        AccountMeta::new_readonly(*token_program.key, false),
    ];
    if let Some(mint_ai) = &mint {
        cpi_account_metas.push(AccountMeta::new_readonly(*mint_ai.as_ref().key, false));
        //add mint account for token 22 accounts
    }

    let instruction = Instruction {
        program_id: MARGINFI_PROGRAM_ID,
        accounts: cpi_account_metas,
        data: data_vec,
    };

    let mut cpi_account_infos = vec![
        marginfi_cpi_accts.marginfi_group.as_ref().clone(),
        marginfi_cpi_accts.marginfi_account.as_ref().clone(),
        authority.as_ref().clone(),
        marginfi_cpi_accts.marginfi_bank.as_ref().clone(),
        source.as_ref().clone(),
        marginfi_cpi_accts.marginfi_liquidity_vault.as_ref().clone(),
        token_program.as_ref().clone(),
    ];

    // Add mint account info if provided
    if let Some(mint_ai) = &mint {
        cpi_account_infos.push(mint_ai.as_ref().clone());
    }

    invoke_signed(&instruction, &cpi_account_infos, authority_pda_seeds).map_err(|_e| {
        trace!("MarginFi Deposit CPI failed: {:?}", _e);
        NixError::MarginfiCpiFailed.into()
    })
}

// CPI to MarginFi: Borrow
pub fn cpi_marginfi_borrow<'a, 'info>(
    marginfi_cpi_accounts_opts: &[Option<MarginfiCpiAccounts<'a, 'info>>; 2],
    global_trade_accounts_opts: &[Option<GlobalTradeAccounts<'a, 'info>>; 2],
    amount: u64,
    mint: Option<&MintAccountInfo<'a, 'info>>,
    authority: MarketSigner<'a,'info>,
    authority_pda_seeds: &[&[&[u8]]],
    accounts: &'a [AccountInfo<'a>],
) -> ProgramResult
where
    'a: 'info,
{
    //we're borrowing base tokens from quote account (with quote vault as authority)
    // let base_global_trade_accounts_opts = global_trade_accounts_opts[0];
    // let quote_global_trade_accounts_opts = global_trade_accounts_opts[1];
    let destination = &global_trade_accounts_opts[0]
        .clone()
        .unwrap()
        .market_vault_opt
        .unwrap();

    let token_program = global_trade_accounts_opts[0]
        .clone()
        .unwrap()
        .token_program_opt
        .unwrap();

    let account_iter: &mut Iter<AccountInfo<'a>> = &mut accounts.iter();

    // borrow from marginfi quote account into base vault
    let base_marginfi_cpi_accts = marginfi_cpi_accounts_opts[0].as_ref().unwrap();
    let quote_marginfi_cpi_accts = marginfi_cpi_accounts_opts[1].as_ref().unwrap();

    trace!("CPI: MarginFi Deposit amount {}", amount);
    let ix_data_args = MfiLendingAccountBorrowData { amount };
    let mut data_vec = MARGINFI_LENDING_ACCOUNT_BORROW_DISCRIMINATOR.to_vec();
    data_vec.extend_from_slice(
        &ix_data_args
            .try_to_vec()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    let mut cpi_account_metas = vec![
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_group.key, false),
        AccountMeta::new(*quote_marginfi_cpi_accts.marginfi_account.key, false),
        AccountMeta::new(*authority.as_ref().key, true), //authority
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_bank.key, false), //debt bank
        AccountMeta::new(*destination.key, false), //destination for borrowed tokens
        AccountMeta::new(
            *base_marginfi_cpi_accts
                .marginfi_liquidity_vault_authority
                .key,
            false,
        ),
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_liquidity_vault.key, false),
        AccountMeta::new_readonly(*token_program.key, false),
    ];
    if let Some(mint_ai) = &mint {
        cpi_account_metas.push(AccountMeta::new_readonly(*mint_ai.as_ref().key, false));
        //add mint account for token 22 accounts
    }

    let instruction = Instruction {
        program_id: MARGINFI_PROGRAM_ID,
        accounts: cpi_account_metas,
        data: data_vec,
    };

    let mut cpi_account_infos = vec![
        base_marginfi_cpi_accts.marginfi_group.as_ref().clone(),
        quote_marginfi_cpi_accts.marginfi_account.as_ref().clone(),
        authority.as_ref().clone(),
        base_marginfi_cpi_accts.marginfi_bank.as_ref().clone(),
        destination.as_ref().clone(),
        base_marginfi_cpi_accts
            .marginfi_liquidity_vault_authority
            .clone(),
        base_marginfi_cpi_accts
            .marginfi_liquidity_vault
            .as_ref()
            .clone(),
        token_program.as_ref().clone(),
    ];

    // Add mint account info if provided
    if let Some(mint_ai) = &mint {
        cpi_account_infos.push(mint_ai.as_ref().clone());
    }

    let base_bank_fixed = base_marginfi_cpi_accts.marginfi_bank.get_fixed()?;
    let base_num_oracle_ais = match base_bank_fixed.config.oracle_setup {
        OracleSetup::StakedWithPythPush => 3,
        _ => 1,
    };
    let base_expected_oracle_keys = base_bank_fixed.config.oracle_keys;

    let quote_bank_fixed = quote_marginfi_cpi_accts.marginfi_bank.get_fixed()?;
    let quote_num_oracle_ais = match quote_bank_fixed.config.oracle_setup {
        OracleSetup::StakedWithPythPush => 3,
        _ => 1,
    };
    let quote_expected_oracle_keys = quote_bank_fixed.config.oracle_keys;

    let mut base_oracle_accounts = Vec::with_capacity(base_num_oracle_ais);
    for i in 0..base_num_oracle_ais {
        let account = next_account_info(account_iter)?;

        require!(
            base_expected_oracle_keys[i] == *account.key,
            NixError::InvalidOracleAccount,
            "Invalid Oracle Account >> expected: {:?}, actual: {:?}",
            base_expected_oracle_keys[i],
            account.key
        )?;
        base_oracle_accounts.push(account.clone());
    }

    let mut quote_oracle_accounts = Vec::with_capacity(quote_num_oracle_ais);
    for i in 0..quote_num_oracle_ais {
        let account = next_account_info(account_iter)?;

        require!(
            quote_expected_oracle_keys[i] == *account.key,
            NixError::InvalidOracleAccount,
            "Invalid Oracle Account >> expected: {:?}, actual: {:?}",
            quote_expected_oracle_keys[i],
            account.key
        )?;
        quote_oracle_accounts.push(account.clone());
    }
    cpi_account_infos.push(base_marginfi_cpi_accts.marginfi_bank.as_ref().clone());
    cpi_account_infos.extend_from_slice(&base_oracle_accounts);
    cpi_account_infos.push(quote_marginfi_cpi_accts.marginfi_bank.as_ref().clone());
    cpi_account_infos.extend_from_slice(&quote_oracle_accounts);

    invoke_signed(&instruction, &cpi_account_infos, authority_pda_seeds).map_err(|_e| {
        trace!("MarginFi Deposit CPI failed: {:?}", _e);
        NixError::MarginfiCpiFailed.into()
    })
}

// CPI to MarginFi: withdraw
pub fn cpi_marginfi_withdraw<'a, 'info>(
    marginfi_cpi_accounts_opts: &[Option<MarginfiCpiAccounts<'a, 'info>>; 2],
    global_trade_accounts_opts: &[Option<GlobalTradeAccounts<'a, 'info>>; 2],
    amount: u64,
    mint: Option<&MintAccountInfo<'a, 'info>>,
    authority: MarketSigner<'a,'info>,
    authority_pda_seeds: &[&[&[u8]]],
    accounts: &'a [AccountInfo<'a>],
) -> ProgramResult
where
    'a: 'info,
{
    // withdraw from base marginfi account so we can repay into quote marginfi account
    let destination = &global_trade_accounts_opts[0]
        .clone()
        .unwrap()
        .market_vault_opt
        .unwrap();

    let token_program = global_trade_accounts_opts[0]
        .clone()
        .unwrap()
        .token_program_opt
        .unwrap();

    let account_iter: &mut Iter<AccountInfo<'a>> = &mut accounts.iter();

    let base_marginfi_cpi_accts = marginfi_cpi_accounts_opts[0].as_ref().unwrap();
    let quote_marginfi_cpi_accts = marginfi_cpi_accounts_opts[1].as_ref().unwrap();
    trace!("CPI: MarginFi Deposit amount {}", amount);
    let ix_data_args = MfiLendingAccountWithdrawData {
        amount,
        withdraw_all: None,
    };

    // CRITICAL BUG FIX: Use correct withdraw discriminator instead of borrow
    // OLD: let mut data_vec = MARGINFI_LENDING_ACCOUNT_BORROW_DISCRIMINATOR.to_vec();
    let mut data_vec = MARGINFI_LENDING_ACCOUNT_WITHDRAW_DISCRIMINATOR.to_vec();
    data_vec.extend_from_slice(
        &ix_data_args
            .try_to_vec()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    let mut cpi_account_metas = vec![
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_group.key, false),
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_account.key, false),
        AccountMeta::new(*authority.as_ref().key, true), //authority
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_bank.key, false), //debt bank
        AccountMeta::new(*destination.key, false), //destination for borrowed tokens
        AccountMeta::new(
            *base_marginfi_cpi_accts
                .marginfi_liquidity_vault_authority
                .key,
            false,
        ),
        AccountMeta::new(*base_marginfi_cpi_accts.marginfi_liquidity_vault.key, false),
        AccountMeta::new_readonly(*token_program.key, false),
    ];
    if let Some(mint_ai) = &mint {
        cpi_account_metas.push(AccountMeta::new_readonly(*mint_ai.as_ref().key, false));
        //add mint account for token 22 accounts
    }

    let instruction = Instruction {
        program_id: MARGINFI_PROGRAM_ID,
        accounts: cpi_account_metas,
        data: data_vec,
    };

    let mut cpi_account_infos = vec![
        base_marginfi_cpi_accts.marginfi_group.as_ref().clone(),
        quote_marginfi_cpi_accts.marginfi_account.as_ref().clone(),
        authority.as_ref().clone(),
        base_marginfi_cpi_accts.marginfi_bank.as_ref().clone(),
        destination.as_ref().clone(),
        base_marginfi_cpi_accts
            .marginfi_liquidity_vault_authority
            .clone(),
        base_marginfi_cpi_accts
            .marginfi_liquidity_vault
            .as_ref()
            .clone(),
        token_program.as_ref().clone(),
    ];

    // Add mint account info if provided
    if let Some(mint_ai) = &mint {
        cpi_account_infos.push(mint_ai.as_ref().clone());
    }
    let base_bank_fixed = base_marginfi_cpi_accts.marginfi_bank.get_fixed()?;
    let base_num_oracle_ais = match base_bank_fixed.config.oracle_setup {
        OracleSetup::StakedWithPythPush => 3,
        _ => 1,
    };
    let base_expected_oracle_keys = base_bank_fixed.config.oracle_keys;

    let quote_bank_fixed = quote_marginfi_cpi_accts.marginfi_bank.get_fixed()?;
    let quote_num_oracle_ais = match quote_bank_fixed.config.oracle_setup {
        OracleSetup::StakedWithPythPush => 3,
        _ => 1,
    };
    let quote_expected_oracle_keys = quote_bank_fixed.config.oracle_keys;

    let mut base_oracle_accounts = Vec::with_capacity(base_num_oracle_ais);
    for i in 0..base_num_oracle_ais {
        let account = next_account_info(account_iter)?;

        require!(
            base_expected_oracle_keys[i] == *account.key,
            NixError::InvalidOracleAccount,
            "Invalid Oracle Account >> expected: {:?}, actual: {:?}",
            base_expected_oracle_keys[i],
            account.key
        )?;
        base_oracle_accounts.push(account.clone());
    }

    let mut quote_oracle_accounts = Vec::with_capacity(quote_num_oracle_ais);
    for i in 0..quote_num_oracle_ais {
        let account = next_account_info(account_iter)?;

        require!(
            quote_expected_oracle_keys[i] == *account.key,
            NixError::InvalidOracleAccount,
            "Invalid Oracle Account >> expected: {:?}, actual: {:?}",
            quote_expected_oracle_keys[i],
            account.key
        )?;
        quote_oracle_accounts.push(account.clone());
    }
    cpi_account_infos.push(base_marginfi_cpi_accts.marginfi_bank.as_ref().clone());
    cpi_account_infos.extend_from_slice(&base_oracle_accounts);
    cpi_account_infos.push(quote_marginfi_cpi_accts.marginfi_bank.as_ref().clone());
    cpi_account_infos.extend_from_slice(&quote_oracle_accounts);

    invoke_signed(&instruction, &cpi_account_infos, authority_pda_seeds).map_err(|_e| {
        trace!("MarginFi Deposit CPI failed: {:?}", _e);
        NixError::MarginfiCpiFailed.into()
    })
}

// CPI to MarginFi: Repay
pub fn cpi_marginfi_repay<'a, 'info>(
    marginfi_cpi_accts: &MarginfiCpiAccounts<'a, 'info>,
    authority: MarketSigner<'a,'info>,
    source: &'a TokenAccountInfo<'a, 'info>,
    token_program: &TokenProgram<'a, 'info>,
    mint: Option<&MintAccountInfo<'a, 'info>>,
    authority_pda_seeds: &[&[&[u8]]],
) -> ProgramResult {
    trace!("CPI: MarginFi Deposit amount {}", amount);
    // CRITICAL BUG FIX: Use correct repay data structure and discriminator
    // OLD: let ix_data_args = MfiLendingAccountBorrowData {
    //     amount: source.get_balance(), //incoming vault
    // };
    let ix_data_args = MfiLendingAccountRepayData {
        amount: source.get_balance(), //incoming vault
        repay_all: None,
    };
    // OLD: let mut data_vec = MARGINFI_LENDING_ACCOUNT_BORROW_DISCRIMINATOR.to_vec();
    let mut data_vec = MARGINFI_LENDING_ACCOUNT_REPAY_DISCRIMINATOR.to_vec();
    data_vec.extend_from_slice(
        &ix_data_args
            .try_to_vec()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    let mut cpi_account_metas = vec![
        AccountMeta::new(*marginfi_cpi_accts.marginfi_group.key, false),
        AccountMeta::new(*marginfi_cpi_accts.marginfi_account.key, false),
        AccountMeta::new(*authority.as_ref().key, true), //authority
        AccountMeta::new(*marginfi_cpi_accts.marginfi_bank.key, false),
        AccountMeta::new(*source.key, false), //source token account
        AccountMeta::new(*marginfi_cpi_accts.marginfi_liquidity_vault.key, false),
        AccountMeta::new_readonly(*token_program.key, false),
    ];
    if let Some(mint_ai) = &mint {
        cpi_account_metas.push(AccountMeta::new_readonly(*mint_ai.as_ref().key, false));
        //add mint account for token 22 accounts
    }

    let instruction = Instruction {
        program_id: MARGINFI_PROGRAM_ID,
        accounts: cpi_account_metas,
        data: data_vec,
    };

    let mut cpi_account_infos = vec![
        marginfi_cpi_accts.marginfi_group.as_ref().clone(),
        marginfi_cpi_accts.marginfi_account.as_ref().clone(),
        authority.as_ref().clone(),
        marginfi_cpi_accts.marginfi_bank.as_ref().clone(),
        source.as_ref().clone(),
        marginfi_cpi_accts.marginfi_liquidity_vault.as_ref().clone(),
        token_program.as_ref().clone(),
    ];

    // Add mint account info if provided
    if let Some(mint_ai) = &mint {
        cpi_account_infos.push(mint_ai.as_ref().clone());
    }

    invoke_signed(&instruction, &cpi_account_infos, authority_pda_seeds).map_err(|_e| {
        trace!("MarginFi Deposit CPI failed: {:?}", _e);
        NixError::MarginfiCpiFailed.into()
    })
}

pub fn get_oracle_price<'a>(
    oracle_accounts: &'a [AccountInfo<'a>],
    bank_config: &BankConfig,
    clock: &Clock,
    price_bias: Option<PriceBias>,
    oracle_price_type: OraclePriceType,
) -> Result<I80F48, ProgramError> {
    let adapter =
        OraclePriceFeedAdapter::try_from_bank_config(bank_config, oracle_accounts, clock)?;

    let price = adapter.get_price_of_type(oracle_price_type, price_bias, bank_config.oracle_max_confidence)?;
    Ok(price)
}

/// Converts token amount to asset shares
pub fn convert_tokens_to_asset_shares(
    token_amount: u64,
    bank: &Bank,
) -> Result<I80F48, ProgramError> {
    I80F48::from_num(token_amount)
        .checked_div(I80F48::from(bank.asset_share_value))
        .ok_or(NixError::NumericalOverflow.into())
}

pub fn convert_asset_shares_to_tokens(
    asset_shares: I80F48,
    bank: &Bank,
) -> Result<u64, ProgramError> {
    Ok(asset_shares
        .checked_mul(I80F48::from(bank.asset_share_value))
        .ok_or(NixError::NumericalOverflow)?
        .checked_floor()
        .ok_or(NixError::NumericalOverflow)?
        .to_num::<u64>())
}

pub fn get_required_quote_collateral_to_back_loan<'a, 'info>(
    base_marginfi_bank: &'a Bank,
    quote_marginfi_bank: &'a Bank,
    base_oracle_price_usd: I80F48,
    quote_oracle_price_usd: I80F48,
    buffer_f: I80F48,
    num_base_atoms: u64,
) -> Result<u64, ProgramError> {
    // Calculate effective collateral weight by applying buffer
    let effective_quote_collateral_weight =
        I80F48::from(quote_marginfi_bank.config.asset_weight_init)
            .checked_mul(buffer_f)
            .ok_or(NixError::NumericalOverflow)?;

    // Convert base tokens to USD value == loan value usd
    let base_value_usd = I80F48::from_num(num_base_atoms)
        .checked_mul(base_oracle_price_usd)
        .ok_or(NixError::NumericalOverflow)?
        .checked_div(EXP_10_I80F48[base_marginfi_bank.mint_decimals as usize])
        .ok_or(NixError::NumericalOverflow)?;

    // Calculate required collateral value in USD
    // Formula: (base_value_usd * liability_weight) / effective_collateral_weight
    let required_quote_collateral_value_usd = base_value_usd
        .checked_mul(I80F48::from(
            base_marginfi_bank.config.liability_weight_init,
        ))
        .ok_or(NixError::NumericalOverflow)?
        .checked_div(effective_quote_collateral_weight)
        .ok_or(NixError::NumericalOverflow)?;

    // Convert USD value to quote token amount
    let required_collateral_tokens_i80f48 = required_quote_collateral_value_usd
        .checked_mul(EXP_10_I80F48[quote_marginfi_bank.mint_decimals as usize])
        .ok_or(NixError::NumericalOverflow)?
        .checked_div(quote_oracle_price_usd)
        .ok_or(NixError::NumericalOverflow)?;

    // Convert to u64 and round up to ensure sufficient collateral
    let required_collateral_tokens = required_collateral_tokens_i80f48
        .checked_ceil()
        .ok_or(NixError::NumericalOverflow)?
        .to_num::<u64>();
    Ok(required_collateral_tokens)
}

/// Returns the amount of tokens required to repay a given amount of liability shares.
pub fn get_token_amount_to_repay_liability_shares(
    liability_shares: I80F48,
    bank: &Bank,
) -> Result<u64, ProgramError> {
    let liability_share_value: I80F48 = bank.liability_share_value.into();

    // Calculate the liability amount (the actual token value of the debt)
    let liability_amount_i80f48 = liability_shares
        .checked_mul(liability_share_value)
        .ok_or(NixError::NumericalOverflow)?; // Assuming NixError is your custom error type

    // Round up to the nearest whole token unit and convert to u64 
    //checked_ceil here would round up in favour of the maker to ensure at least the actual repay amount is obtained  
    let repay_amount = liability_amount_i80f48
        .checked_ceil()
        .ok_or(NixError::NumericalOverflow)?
        .to_num::<u64>();

    Ok(repay_amount)
}
/// Converts a token amount to liability shares for a given bank.
/// This is the inverse of `get_token_amount_to_repay_base_liability_shares`.
pub fn convert_tokens_to_liability_shares(
    token_amount: u64,
    bank: &Bank,
) -> Result<I80F48, ProgramError> {
    let liability_share_value: I80F48 = bank.liability_share_value.into();

    // Convert token amount to asset shares
    let liability_shares = I80F48::from_num(token_amount)
        .checked_div(liability_share_value)
        .ok_or(NixError::NumericalOverflow)?;
    Ok(liability_shares)
}
