use crate::{
    logs::{emit_stack, CreateMarketLog},
    marginfi_utils::initialize_marginfi_account,
    program::expand_market_if_needed,
    state::MarketFixed,
    utils::create_account,
    validation::{
        get_market_fee_receiver_address, get_market_signer_address, get_vault_address,
        loaders::CreateMarketContext, EmptyAccount, MarginfiAccountInfo, MintAccountInfo,
        NixAccountInfo, Program, Signer, TokenProgram,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use hypertree::{get_mut_helper, trace};
use marginfi::state::{marginfi_account::MarginfiAccount, marginfi_group::MarginfiGroup};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::invoke, program_pack::Pack,
    pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};
use spl_token_2022::{
    extension::{
        mint_close_authority::MintCloseAuthority, permanent_delegate::PermanentDelegate,
        BaseStateWithExtensions, ExtensionType, PodStateWithExtensions, StateWithExtensions,
    },
    pod::PodMint,
    state::{Account, Mint},
};
use std::mem::size_of;

use std::cell::Ref;
#[derive(BorshDeserialize, BorshSerialize)]
pub struct CreateMarketParams {
    protocol_fee_rate_bps: u64,
    marginfi_market_buffer_bps: u64,
}

pub(crate) fn process_create_market(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let params: CreateMarketParams = CreateMarketParams::try_from_slice(data)?;
    process_create_market_core(_program_id, accounts, params)
}

pub(crate) fn process_create_market_core(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: CreateMarketParams,
) -> ProgramResult {
    trace!("process_create_market accts={accounts:?}");
    let create_market_context: CreateMarketContext = CreateMarketContext::load(accounts)?;

    let CreateMarketContext {
        admin,
        market,
        market_signer,
        system_program,
        token_program,
        token_program_22,
        base_a_mint,
        base_b_mint,
        base_a_fee_receiver,
        base_b_fee_receiver,
        base_a_vault,
        base_b_vault,
        base_a_marginfi_group,
        base_b_marginfi_group,
        base_a_marginfi_account,
        base_b_marginfi_account,
        ..
    } = &create_market_context;

    let (_, market_signer_bump) = get_market_signer_address(market.key);
    for (mint, vault, fee_receiver, marginfi_group, marginfi_account) in [
        (
            base_a_mint,
            base_a_vault,
            base_a_fee_receiver,
            base_a_marginfi_group,
            base_a_marginfi_account,
        ),
        (
            base_b_mint,
            base_b_vault,
            base_b_fee_receiver,
            base_b_marginfi_group,
            base_b_marginfi_account,
        ),
    ] {
        // Process token type operations
        process_token_type(
            admin,
            market,
            market_signer.as_ref(),
            market_signer_bump,
            mint,
            vault,
            fee_receiver,
            marginfi_group,
            marginfi_account,
            system_program,
            token_program,
            token_program_22,
        )?;
    }
    // Do not need to initialize with the system program because it is
    // assumed that it is done already and loaded with rent. That is not at
    // a PDA because we do not want to be restricted to a single market for
    // a pair. If there is lock contention and hotspotting for one market,
    // it could be useful to have a second where it is easier to land
    // transactions. That protection is worth the possibility that users
    // would use an inactive market when multiple exist.

    // Setup the empty market
    let empty_market_fixed: MarketFixed = MarketFixed::new_empty(
        &create_market_context,
        params.protocol_fee_rate_bps,
        params.marginfi_market_buffer_bps,
    );
    assert_eq!(market.data_len(), size_of::<MarketFixed>());

    let market_bytes: &mut [u8] = &mut market.try_borrow_mut_data()?[..];
    *get_mut_helper::<MarketFixed>(market_bytes, 0_u32) = empty_market_fixed;

    emit_stack(CreateMarketLog {
        base_a_mint: *base_a_mint.as_ref().key,
        base_b_mint: *base_b_mint.as_ref().key,
        market_key: *market.key,
        admin: *admin.key,
    })?;
    expand_market_if_needed(&admin, &market)?;
    Ok(())
}

fn process_token_type<'a, 'info>(
    admin: &'a Signer<'a, 'info>,
    market: &'a NixAccountInfo<'a, 'info, MarketFixed>,
    market_signer: &'a AccountInfo<'info>,
    market_signer_bump: u8,
    mint: &'a MintAccountInfo<'a, 'info>,
    vault: &'a EmptyAccount<'a, 'info>,
    fee_receiver: &'a EmptyAccount<'a, 'info>,
    marginfi_group: &'a MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    marginfi_account: &'a MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    system_program: &'a Program<'a, 'info>,
    token_program: &'a TokenProgram<'a, 'info>,
    token_program_22: &'a TokenProgram<'a, 'info>,
) -> ProgramResult {
    // 1. Create vault and fee receiver
    create_vault_and_fee_receiver(
        admin,
        mint,
        vault,
        fee_receiver,
        system_program,
        token_program,
        token_program_22,
        market,
        market_signer,
    )?;

    // 2. Initialize Marginfi account
    initialize_marginfi_account(
        marginfi_group,
        marginfi_account,
        admin,
        system_program, // system_program
        market,
        market_signer,
        market_signer_bump,
    )?;

    Ok(())
}

fn create_vault_and_fee_receiver<'a, 'info>(
    admin: &'a Signer<'a, 'info>,
    mint: &'a MintAccountInfo<'a, 'info>,
    vault: &'a EmptyAccount<'a, 'info>,
    fee_receiver: &'a EmptyAccount<'a, 'info>,
    system_program: &'a Program<'a, 'info>,
    token_program: &'a TokenProgram<'a, 'info>,
    token_program_22: &'a TokenProgram<'a, 'info>,
    market: &'a NixAccountInfo<'a, 'info, MarketFixed>,
    market_signer: &'a AccountInfo<'info>,
) -> ProgramResult {
    let rent: Rent = Rent::get()?;

    let mint_info = mint.as_ref();
    let vault_info = vault.as_ref();
    let fee_receiver_info = fee_receiver.as_ref();

    if *mint_info.owner == spl_token_2022::id() {
        let mint_data = mint_info.data.borrow();
        let pool_mint: StateWithExtensions<'_, Mint> =
            StateWithExtensions::<Mint>::unpack(&mint_data)?;
        // Closable mints can be replaced with different ones, breaking some saved info on the market.
        if let Ok(extension) = pool_mint.get_extension::<MintCloseAuthority>() {
            let close_authority: Option<Pubkey> = extension.close_authority.into();
            if close_authority.is_some() {
                solana_program::msg!("Warning, you are creating a market with a close authority.");
            }
        }
        // Permanent delegates can steal your tokens. This will break all
        // accounting in the market, so there is no assertion of security
        // against loss of funds on these markets.
        if let Ok(extension) = pool_mint.get_extension::<PermanentDelegate>() {
            let permanent_delegate: Option<Pubkey> = extension.delegate.into();
            if permanent_delegate.is_some() {
                solana_program::msg!(
                    "Warning, you are creating a market with a permanent delegate. There is no loss of funds protection for funds on this market"
                );
            }
        }
    }

    // We don't have to deserialize the mint, just check the owner.
    let is_mint_22: bool = *mint_info.owner == spl_token_2022::id();
    let token_program_for_mint: Pubkey = if is_mint_22 {
        spl_token_2022::id()
    } else {
        spl_token::id()
    };

    let (_vault_key, vault_bump) = get_vault_address(market.key, mint_info.key);
    let vault_seeds: Vec<Vec<u8>> = vec![
        b"vault".to_vec(),
        market.key.as_ref().to_vec(),
        mint_info.key.as_ref().to_vec(),
        vec![vault_bump],
    ];

    let (_fee_receiver_key, fee_receiver_bump) =
        get_market_fee_receiver_address(market.key, mint_info.key);

    let fee_receiver_seeds: Vec<Vec<u8>> = vec![
        b"fee-receiver".to_vec(),
        market.key.as_ref().to_vec(),
        mint_info.key.as_ref().to_vec(),
        vec![fee_receiver_bump],
    ];

    let space = if is_mint_22 {
        let mint_data: Ref<'_, &mut [u8]> = mint_info.data.borrow();
        let mint_with_extension = PodStateWithExtensions::<PodMint>::unpack(&mint_data).unwrap();
        let mint_extensions = mint_with_extension.get_extension_types()?;
        let required_extensions =
            ExtensionType::get_required_init_account_extensions(&mint_extensions);
        ExtensionType::try_calculate_account_len::<Account>(&required_extensions)?
    } else {
        spl_token::state::Account::LEN
    };

    // Create vault
    create_account(
        admin.as_ref(),
        vault_info,
        system_program.as_ref(),
        &token_program_for_mint,
        &rent,
        space as u64,
        vault_seeds,
    )?;
    let init_vault_instruction = if is_mint_22 {
        spl_token_2022::instruction::initialize_account3(
            &token_program_for_mint,
            vault_info.key,
            mint_info.key,
            market_signer.key,
        )?
    } else {
        spl_token::instruction::initialize_account3(
            &token_program_for_mint,
            vault_info.key,
            mint_info.key,
            market_signer.key,
        )?
    };
    invoke(
        &init_vault_instruction,
        &[
            admin.as_ref().clone(),
            vault_info.clone(),
            mint_info.clone(),
            if is_mint_22 {
                token_program_22.as_ref()
            } else {
                token_program.as_ref()
            }
            .clone(),
        ],
    )?;

    // Create fee receiver
    create_account(
        admin,
        fee_receiver_info,
        system_program.as_ref(),
        &token_program_for_mint,
        &rent,
        space as u64,
        fee_receiver_seeds,
    )?;
    let fee_receiver_instruction = if is_mint_22 {
        spl_token_2022::instruction::initialize_account3(
            &token_program_for_mint,
            fee_receiver_info.key,
            mint_info.key,
            market_signer.key,
        )?
    } else {
        spl_token::instruction::initialize_account3(
            &token_program_for_mint,
            fee_receiver_info.key,
            mint_info.key,
            market_signer.key,
        )?
    };
    invoke(
        &fee_receiver_instruction,
        &[
            admin.as_ref().clone(),
            fee_receiver_info.clone(),
            mint_info.clone(),
            if is_mint_22 {
                token_program_22.as_ref()
            } else {
                token_program.as_ref()
            }
            .clone(),
        ],
    )?;

    Ok(())
}
