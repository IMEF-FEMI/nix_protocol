use hypertree::{get_helper, trace};
use marginfi::state::{
    marginfi_account::MarginfiAccount,
    marginfi_group::{Bank, MarginfiGroup},
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program,
};

use crate::{
    program::NixError,
    require,
    state::{market_loan::MarketLoansFixed, GlobalFixed, MarketFixed},
    validation::{
        validate_marginfi_liquidity_vault, validate_marginfi_liquidity_vault_authority,
        MarketSigner,
    },
};

use super::{
    get_market_fee_receiver_address, get_vault_address, EmptyAccount, MarginfiAccountInfo,
    MintAccountInfo, NixAccountInfo, Program, Signer, TokenAccountInfo, TokenProgram,
};
use std::{cell::Ref, slice::Iter};
/// CreateMarket account infos
pub(crate) struct CreateMarketContext<'a, 'info> {
    pub admin: Signer<'a, 'info>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
    pub market_signer: MarketSigner<'a, 'info>,
    pub base_a_mint: MintAccountInfo<'a, 'info>,
    pub base_b_mint: MintAccountInfo<'a, 'info>,
    pub base_a_fee_receiver: EmptyAccount<'a, 'info>,
    pub base_b_fee_receiver: EmptyAccount<'a, 'info>,
    pub base_a_vault: EmptyAccount<'a, 'info>,
    pub base_b_vault: EmptyAccount<'a, 'info>,
    pub base_a_marginfi_group: MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    pub base_a_marginfi_bank: MarginfiAccountInfo<'a, 'info, Bank>,
    pub base_a_marginfi_account: MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    pub base_b_marginfi_group: MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    pub base_b_marginfi_bank: MarginfiAccountInfo<'a, 'info, Bank>,
    pub base_b_marginfi_account: MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    pub system_program: Program<'a, 'info>,
    pub token_program: TokenProgram<'a, 'info>,
    pub token_program_22: TokenProgram<'a, 'info>,
}

impl<'a, 'info> CreateMarketContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let admin: Signer = Signer::new_payer(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new_init(next_account_info(account_iter)?)?;
        let market_signer = MarketSigner::new(next_account_info(account_iter)?, market.key)?;

        let base_a_mint: MintAccountInfo = MintAccountInfo::new(next_account_info(account_iter)?)?;
        let base_b_mint: MintAccountInfo = MintAccountInfo::new(next_account_info(account_iter)?)?;
        let base_a_fee_receiver: EmptyAccount =
            EmptyAccount::new(next_account_info(account_iter)?)?;
        let base_b_fee_receiver: EmptyAccount =
            EmptyAccount::new(next_account_info(account_iter)?)?;
        let base_a_vault: EmptyAccount = EmptyAccount::new(next_account_info(account_iter)?)?;
        let base_b_vault: EmptyAccount = EmptyAccount::new(next_account_info(account_iter)?)?;

        let base_a_marginfi_group: MarginfiAccountInfo<MarginfiGroup> =
            MarginfiAccountInfo::<MarginfiGroup>::new_group(next_account_info(account_iter)?)?;

        let base_a_marginfi_bank: MarginfiAccountInfo<Bank> =
            MarginfiAccountInfo::<Bank>::new_bank(next_account_info(account_iter)?)?;

        let base_a_marginfi_account: MarginfiAccountInfo<MarginfiAccount> =
            MarginfiAccountInfo::<MarginfiAccount>::new_account_uninitialized(
                next_account_info(account_iter)?,
                market.info,
                base_a_mint.info,
            )?;

        let base_b_marginfi_group: MarginfiAccountInfo<MarginfiGroup> =
            MarginfiAccountInfo::<MarginfiGroup>::new_group(next_account_info(account_iter)?)?;

        let base_b_marginfi_bank: MarginfiAccountInfo<Bank> =
            MarginfiAccountInfo::<Bank>::new_bank(next_account_info(account_iter)?)?;

        let base_b_marginfi_account: MarginfiAccountInfo<MarginfiAccount> =
            MarginfiAccountInfo::<MarginfiAccount>::new_account_uninitialized(
                next_account_info(account_iter)?,
                market.info,
                base_a_mint.info,
            )?;

        let system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;

        let (expected_base_a_fee_receiver, _) =
            get_market_fee_receiver_address(market.key, base_a_mint.info.key);
        let (expected_base_b_fee_receiver, _) =
            get_market_fee_receiver_address(market.key, base_a_mint.info.key);
        require!(
            expected_base_a_fee_receiver == *base_a_fee_receiver.info.key,
            NixError::IncorrectAccount,
            "Incorrect fee receiver account",
        )?;
        require!(
            expected_base_b_fee_receiver == *base_b_fee_receiver.info.key,
            NixError::IncorrectAccount,
            "Incorrect fee receiver account",
        )?;
        let (expected_base_a_vault, _) = get_vault_address(market.key, base_a_mint.info.key);

        require!(
            expected_base_a_vault == *base_a_vault.info.key,
            NixError::IncorrectAccount,
            "Incorrect vault account",
        )?;
        let (expected_base_b_vault, _) = get_vault_address(market.key, base_b_mint.info.key);
        require!(
            expected_base_b_vault == *base_b_vault.info.key,
            NixError::IncorrectAccount,
            "Incorrect vault account",
        )?;
        let token_program: TokenProgram = TokenProgram::new(next_account_info(account_iter)?)?;
        let token_program_22: TokenProgram = TokenProgram::new(next_account_info(account_iter)?)?;

        Ok(Self {
            admin,
            market,
            market_signer,
            base_a_mint,
            base_b_mint,
            base_a_fee_receiver,
            base_b_fee_receiver,
            base_a_vault,
            base_b_vault,
            base_a_marginfi_group,
            base_a_marginfi_bank,
            base_a_marginfi_account,
            base_b_marginfi_group,
            base_b_marginfi_bank,
            base_b_marginfi_account,
            system_program,
            token_program,
            token_program_22,
        })
    }
}

/// ClaimSeat account infos
pub(crate) struct ClaimSeatContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
    pub _system_program: Program<'a, 'info>,
}

impl<'a, 'info> ClaimSeatContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new(next_account_info(account_iter)?)?;
        let _system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;
        Ok(Self {
            payer,
            market,
            _system_program,
        })
    }
}

/// Deposit into a market account infos
pub(crate) struct DepositContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
    pub market_signer: MarketSigner<'a, 'info>,
    pub mint: MintAccountInfo<'a, 'info>,
    pub trader_token_account: TokenAccountInfo<'a, 'info>,
    pub token_program: TokenProgram<'a, 'info>,
    pub vault: TokenAccountInfo<'a, 'info>,
    pub marginfi_group: MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    pub marginfi_bank: MarginfiAccountInfo<'a, 'info, Bank>,
    pub marginfi_account: MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    pub marginfi_liquidity_vault: TokenAccountInfo<'a, 'info>,
}

impl<'a, 'info> DepositContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new_payer(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new(next_account_info(account_iter)?)?;

        let market_fixed: Ref<MarketFixed> = market.get_fixed()?;
        let base_a_mint: &Pubkey = market_fixed.get_base_a_mint();
        let base_b_mint: &Pubkey = market_fixed.get_base_b_mint();
        let market_signer = MarketSigner::new(next_account_info(account_iter)?, market.key)?;
        let trader_token_account_info: &AccountInfo<'info> = next_account_info(account_iter)?;

        // Infer the mint key from the token account.
        let (
            mint,
            expected_vault_address,
            expected_marginfi_group,
            expected_marginfi_bank,
            expected_marginfi_account,
        ) = if &trader_token_account_info.try_borrow_data()?[0..32] == base_a_mint.as_ref() {
            (
                base_a_mint,
                market_fixed.get_base_a_vault(),
                market_fixed.get_base_a_marginfi_group(),
                market_fixed.get_base_a_marginfi_bank(),
                market_fixed.get_base_a_marginfi_account(),
            )
        } else if &trader_token_account_info.try_borrow_data()?[0..32] == base_b_mint.as_ref() {
            (
                base_b_mint,
                market_fixed.get_base_b_vault(),
                market_fixed.get_base_b_marginfi_group(),
                market_fixed.get_base_b_marginfi_bank(),
                market_fixed.get_base_b_marginfi_account(),
            )
        } else {
            return Err(NixError::InvalidDepositAccounts.into());
        };

        trace!("trader token account {:?}", trader_token_account_info.key);
        let trader_token_account: TokenAccountInfo =
            TokenAccountInfo::new_with_owner(trader_token_account_info, mint, payer.key)?;

        trace!("vault token account {:?}", expected_vault_address);
        let vault: TokenAccountInfo = TokenAccountInfo::new_with_owner_and_key(
            next_account_info(account_iter)?,
            mint,
            &expected_vault_address,
            &expected_vault_address,
        )?;

        let token_program: TokenProgram = TokenProgram::new(next_account_info(account_iter)?)?;
        let mint: MintAccountInfo = MintAccountInfo::new(next_account_info(account_iter)?)?;

        let marginfi_group: MarginfiAccountInfo<MarginfiGroup> =
            MarginfiAccountInfo::<MarginfiGroup>::new_group(next_account_info(account_iter)?)?;

        require!(
            expected_marginfi_group == marginfi_group.info.key,
            NixError::InvalidMarginfiGroup,
            "Invalid Marginfi Group >> expected: {:?}, actual: {:?}",
            expected_marginfi_group,
            marginfi_group.info.key
        )?;
        let marginfi_bank: MarginfiAccountInfo<Bank> =
            MarginfiAccountInfo::<Bank>::new_bank(next_account_info(account_iter)?)?;

        require!(
            expected_marginfi_bank == marginfi_bank.info.key,
            NixError::InvalidMarginfiBank,
            "Invalid Marginfi bank >> expected: {:?}, actual: {:?}",
            expected_marginfi_bank,
            marginfi_bank.info.key
        )?;
        let marginfi_account: MarginfiAccountInfo<MarginfiAccount> =
            MarginfiAccountInfo::<MarginfiAccount>::new_account(
                next_account_info(account_iter)?,
                market.info.key,
                mint.info.key,
            )?;
        require!(
            expected_marginfi_account == marginfi_account.info.key,
            NixError::InvalidMarginfiAccount,
            "Invalid Marginfi account >> expected: {:?}, actual: {:?}",
            expected_marginfi_account,
            marginfi_account.info.key
        )?;

        let marginfi_liquidity_vault: TokenAccountInfo =
            TokenAccountInfo::new(next_account_info(account_iter)?, mint.info.key)?;
        validate_marginfi_liquidity_vault(marginfi_liquidity_vault.as_ref(), &marginfi_bank)?;

        // Drop the market ref so it can be passed through the return.
        // This is necessary to avoid borrowing issues with the market_fixed reference.
        drop(market_fixed);
        Ok(Self {
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
        })
    }
}

/// Global create
pub(crate) struct GlobalCreateContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub global: EmptyAccount<'a, 'info>,
    pub system_program: Program<'a, 'info>,
    pub global_mint: MintAccountInfo<'a, 'info>,
    pub global_vault: EmptyAccount<'a, 'info>,
    pub token_program: TokenProgram<'a, 'info>,
}

impl<'a, 'info> GlobalCreateContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new_payer(next_account_info(account_iter)?)?;
        let global: EmptyAccount = EmptyAccount::new(next_account_info(account_iter)?)?;
        let system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;
        let global_mint: MintAccountInfo = MintAccountInfo::new(next_account_info(account_iter)?)?;
        // Address of the global vault is verified in the handler because the
        // create will only work if the signer seeds match.
        let global_vault: EmptyAccount = EmptyAccount::new(next_account_info(account_iter)?)?;
        let token_program: TokenProgram = TokenProgram::new(next_account_info(account_iter)?)?;
        Ok(Self {
            payer,
            global,
            system_program,
            global_mint,
            global_vault,
            token_program,
        })
    }
}

/// Global add trader
pub(crate) struct GlobalAddTraderContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub global: NixAccountInfo<'a, 'info, GlobalFixed>,
    pub _system_program: Program<'a, 'info>,
}

impl<'a, 'info> GlobalAddTraderContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new_payer(next_account_info(account_iter)?)?;
        let global: NixAccountInfo<GlobalFixed> =
            NixAccountInfo::<GlobalFixed>::new(next_account_info(account_iter)?)?;
        let _system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;
        Ok(Self {
            payer,
            global,
            _system_program,
        })
    }
}

/// Global deposit
pub(crate) struct GlobalDepositContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub global: NixAccountInfo<'a, 'info, GlobalFixed>,
    pub mint: MintAccountInfo<'a, 'info>,
    pub global_vault: TokenAccountInfo<'a, 'info>,
    pub trader_token: TokenAccountInfo<'a, 'info>,
    pub token_program: TokenProgram<'a, 'info>,
}

impl<'a, 'info> GlobalDepositContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new(next_account_info(account_iter)?)?;
        let global: NixAccountInfo<GlobalFixed> =
            NixAccountInfo::<GlobalFixed>::new(next_account_info(account_iter)?)?;

        let mint: MintAccountInfo = MintAccountInfo::new(next_account_info(account_iter)?)?;

        let global_data: Ref<&mut [u8]> = global.data.borrow();
        let global_fixed: &GlobalFixed = get_helper::<GlobalFixed>(&global_data, 0_u32);
        let expected_global_vault_address: &Pubkey = global_fixed.get_vault();

        let global_vault: TokenAccountInfo = TokenAccountInfo::new_with_owner_and_key(
            next_account_info(account_iter)?,
            mint.info.key,
            &expected_global_vault_address,
            &expected_global_vault_address,
        )?;
        drop(global_data);

        let token_account_info: &AccountInfo<'info> = next_account_info(account_iter)?;
        let trader_token: TokenAccountInfo =
            TokenAccountInfo::new_with_owner(token_account_info, mint.info.key, payer.key)?;
        let token_program: TokenProgram = TokenProgram::new(next_account_info(account_iter)?)?;
        Ok(Self {
            payer,
            global,
            mint,
            global_vault,
            trader_token,
            token_program,
        })
    }
}

/// Accounts needed to make a global trade. Scope is beyond just crate so
/// clients can place orders on markets in testing.
#[derive(Clone)]
pub struct GlobalTradeAccounts<'a, 'info> {
    pub global: NixAccountInfo<'a, 'info, GlobalFixed>,

    // These are required when matching a global order, not necessarily when
    // cancelling since tokens dont move in that case.
    pub global_vault_opt: Option<TokenAccountInfo<'a, 'info>>,
    pub market_vault_opt: Option<TokenAccountInfo<'a, 'info>>,
    pub token_program_opt: Option<TokenProgram<'a, 'info>>,

    pub system_program: Option<Program<'a, 'info>>,

    // Trader is sending or cancelling the order. They are the one who will pay
    // or receive gas prepayments.
    pub gas_payer_opt: Option<Signer<'a, 'info>>,
    pub gas_receiver_opt: Option<Signer<'a, 'info>>,
    pub market: Pubkey,
}

#[derive(Clone)]
pub struct MarginfiCpiAccounts<'a, 'info> {
    pub marginfi_group: MarginfiAccountInfo<'a, 'info, MarginfiGroup>,
    pub marginfi_bank: MarginfiAccountInfo<'a, 'info, Bank>,
    pub marginfi_account: MarginfiAccountInfo<'a, 'info, MarginfiAccount>,
    pub marginfi_liquidity_vault: TokenAccountInfo<'a, 'info>,
    pub marginfi_liquidity_vault_authority: &'a AccountInfo<'info>,
}
pub(crate) struct PlaceOrderContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
    pub market_loans: NixAccountInfo<'a, 'info, MarketLoansFixed>,
    pub market_signer: MarketSigner<'a, 'info>,
    pub base_mint: MintAccountInfo<'a, 'info>,
    pub quote_mint: MintAccountInfo<'a, 'info>,

    // One for each side. First is base, then is quote.
    pub global_trade_accounts_opts: [Option<GlobalTradeAccounts<'a, 'info>>; 2],
    pub marginfi_cpi_accounts_opts: [Option<MarginfiCpiAccounts<'a, 'info>>; 2],
}

impl<'a, 'info> PlaceOrderContext<'a, 'info> {
    pub fn load(
        accounts: &'a [AccountInfo<'info>],
        use_a_tree: bool,
    ) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        // Does not have to be writable, but this ix will fail if removing a
        // global or requiring expanding.
        let payer: Signer = Signer::new(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new(next_account_info(account_iter)?)?;
        let market_loans: NixAccountInfo<MarketLoansFixed> =
            NixAccountInfo::<MarketLoansFixed>::new(next_account_info(account_iter)?)?;
        let market_signer = MarketSigner::new(next_account_info(account_iter)?, market.key)?;

        let system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;

        let mut global_trade_accounts_opts: [Option<GlobalTradeAccounts<'a, 'info>>; 2] =
            [None, None];
        let mut marginfi_cpi_accounts_opts: [Option<MarginfiCpiAccounts<'a, 'info>>; 2] =
            [None, None];

        {
            let market_fixed: Ref<MarketFixed> = market.get_fixed()?;

            // determine primary base (this will determine which of the trees we will use)
            let (
                base_mint_key,
                quote_mint_key,
                base_vault_key,
                quote_vault_key,
                base_group_key,
                quote_group_key,
                base_bank_key,
                quote_bank_key,
                base_account_key,
                quote_account_key,
            ) = if use_a_tree {
                (
                    *market_fixed.get_base_a_mint(),
                    *market_fixed.get_base_b_mint(),
                    *market_fixed.get_base_a_vault(),
                    *market_fixed.get_base_b_vault(),
                    *market_fixed.get_base_a_marginfi_group(),
                    *market_fixed.get_base_b_marginfi_group(),
                    *market_fixed.get_base_a_marginfi_bank(),
                    *market_fixed.get_base_b_marginfi_bank(),
                    *market_fixed.get_base_a_marginfi_account(),
                    *market_fixed.get_base_b_marginfi_account(),
                )
            } else {
                (
                    *market_fixed.get_base_b_mint(),
                    *market_fixed.get_base_a_mint(),
                    *market_fixed.get_base_b_vault(),
                    *market_fixed.get_base_a_vault(),
                    *market_fixed.get_base_b_marginfi_group(),
                    *market_fixed.get_base_a_marginfi_group(),
                    *market_fixed.get_base_b_marginfi_bank(),
                    *market_fixed.get_base_a_marginfi_bank(),
                    *market_fixed.get_base_b_marginfi_account(),
                    *market_fixed.get_base_a_marginfi_account(),
                )
            };
            drop(market_fixed);

            let mint1_ai = next_account_info(account_iter)?;
            let mint2_ai = next_account_info(account_iter)?;

            let (base_mint_ai, quote_mint_ai) =
                if base_mint_key == *mint1_ai.key && quote_mint_key == *mint2_ai.key {
                    (&mint1_ai, &mint2_ai)
                } else if base_mint_key == *mint2_ai.key && quote_mint_key == *mint1_ai.key {
                    (&mint2_ai, &mint1_ai)
                } else {
                    return Err(NixError::InvalidMint.into());
                };
            let base_mint: MintAccountInfo<'a, 'info> = MintAccountInfo::new(base_mint_ai)?;
            let quote_mint: MintAccountInfo<'a, 'info> = MintAccountInfo::new(quote_mint_ai)?;

            for _ in 0..2 {
                let next_account_info_or: Result<&AccountInfo<'info>, ProgramError> =
                    next_account_info(account_iter);
                if next_account_info_or.is_ok() {
                    let global_or: Result<NixAccountInfo<'a, 'info, GlobalFixed>, ProgramError> =
                        NixAccountInfo::<GlobalFixed>::new(next_account_info_or?);

                    // If a client blindly fills in the global account and vault,
                    // then handle that case and allow them to try to work without
                    // the global accounts.
                    if global_or.is_err() {
                        let _global_vault: Result<&AccountInfo<'info>, ProgramError> =
                            next_account_info(account_iter);
                        let _market_vault: Result<&AccountInfo<'info>, ProgramError> =
                            next_account_info(account_iter);
                        let _token_program: Result<&AccountInfo<'info>, ProgramError> =
                            next_account_info(account_iter);
                        continue;
                    }

                    let global: NixAccountInfo<'a, 'info, GlobalFixed> = global_or.unwrap();
                    let global_data: Ref<&mut [u8]> = global.data.borrow();
                    let global_fixed: &GlobalFixed = get_helper::<GlobalFixed>(&global_data, 0_u32);
                    let expected_global_vault_address: &Pubkey = global_fixed.get_vault();

                    let mint: MintAccountInfo<'a, 'info> =
                        if global_fixed.get_mint() == base_mint.info.key {
                            base_mint.clone()
                        } else {
                            quote_mint.clone()
                        };

                    let (index, expected_market_vault_address) =
                        if base_mint.info.key == mint.info.key {
                            (0, &base_vault_key)
                        } else {
                            require!(
                                quote_mint.info.key == mint.info.key,
                                NixError::MissingGlobal,
                                "Unexpected global mint",
                            )?;
                            (1, &quote_vault_key)
                        };

                    let global_vault: TokenAccountInfo<'a, 'info> =
                        TokenAccountInfo::new_with_owner_and_key(
                            next_account_info(account_iter)?,
                            mint.info.key,
                            &expected_global_vault_address,
                            &expected_global_vault_address,
                        )?;
                    drop(global_data);

                    let market_vault: TokenAccountInfo<'a, 'info> =
                        TokenAccountInfo::new_with_owner_and_key(
                            next_account_info(account_iter)?,
                            mint.info.key,
                            &expected_market_vault_address,
                            &expected_market_vault_address,
                        )?;
                    let token_program: TokenProgram<'a, 'info> =
                        TokenProgram::new(next_account_info(account_iter)?)?;

                    global_trade_accounts_opts[index] = Some(GlobalTradeAccounts {
                        global,
                        global_vault_opt: Some(global_vault),
                        market_vault_opt: Some(market_vault),
                        token_program_opt: Some(token_program),
                        system_program: Some(system_program.clone()),
                        gas_payer_opt: Some(payer.clone()),
                        gas_receiver_opt: Some(payer.clone()),
                        market: *market.info.key,
                    })
                }
            }

            for _ in 0..2 {
                let marginfi_group_account_raw = next_account_info(account_iter)?;

                let (
                    index,
                    mint,
                    expected_marginfi_group,
                    expected_marginfi_bank,
                    expected_marginfi_account,
                ) = if *marginfi_group_account_raw.key == base_group_key {
                    (
                        0,
                        base_mint.info.key,
                        base_group_key,
                        base_bank_key,
                        base_account_key,
                    )
                } else if quote_group_key == *marginfi_group_account_raw.key {
                    (
                        1,
                        quote_mint.info.key,
                        quote_group_key,
                        quote_bank_key,
                        quote_account_key,
                    )
                } else {
                    return Err(NixError::InvalidDepositAccounts.into());
                };

                let marginfi_group: MarginfiAccountInfo<MarginfiGroup> =
                    MarginfiAccountInfo::<MarginfiGroup>::new_group(marginfi_group_account_raw)?;

                require!(
                    expected_marginfi_group == *marginfi_group.info.key,
                    NixError::InvalidMarginfiGroup,
                    "Invalid Marginfi Group >> expected: {:?}, actual: {:?}",
                    expected_marginfi_group,
                    marginfi_group.info.key
                )?;
                let marginfi_bank: MarginfiAccountInfo<Bank> =
                    MarginfiAccountInfo::<Bank>::new_bank(next_account_info(account_iter)?)?;

                require!(
                    expected_marginfi_bank == *marginfi_bank.info.key,
                    NixError::InvalidMarginfiBank,
                    "Invalid Marginfi bank >> expected: {:?}, actual: {:?}",
                    expected_marginfi_bank,
                    marginfi_bank.info.key
                )?;
                let marginfi_account: MarginfiAccountInfo<MarginfiAccount> =
                    MarginfiAccountInfo::<MarginfiAccount>::new_account(
                        next_account_info(account_iter)?,
                        market.info.key,
                        mint,
                    )?;
                require!(
                    expected_marginfi_account == *marginfi_account.info.key,
                    NixError::InvalidMarginfiAccount,
                    "Invalid Marginfi account >> expected: {:?}, actual: {:?}",
                    expected_marginfi_account,
                    marginfi_account.info.key
                )?;

                let marginfi_liquidity_vault: TokenAccountInfo =
                    TokenAccountInfo::new(next_account_info(account_iter)?, mint)?;
                validate_marginfi_liquidity_vault(
                    marginfi_liquidity_vault.as_ref(),
                    &marginfi_bank,
                )?;

                let marginfi_liquidity_vault_authority = next_account_info(account_iter)?;
                validate_marginfi_liquidity_vault_authority(
                    marginfi_liquidity_vault_authority,
                    marginfi_bank.info,
                )?;

                marginfi_cpi_accounts_opts[index] = Some(MarginfiCpiAccounts {
                    marginfi_group,
                    marginfi_bank,
                    marginfi_account,
                    marginfi_liquidity_vault,
                    marginfi_liquidity_vault_authority,
                });
            }

            Ok(Self {
                payer,
                market,
                market_loans,
                market_signer,
                base_mint,
                quote_mint,
                global_trade_accounts_opts,
                marginfi_cpi_accounts_opts,
            })
        }
    }
}

/// CreateMarketLoanAccount account infos
pub(crate) struct CreateMarketLoanAccountContext<'a, 'info> {
    pub admin: Signer<'a, 'info>,
    pub market_loan_account: NixAccountInfo<'a, 'info, MarketLoansFixed>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
}

impl<'a, 'info> CreateMarketLoanAccountContext<'a, 'info> {
    pub fn load(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let admin: Signer = Signer::new(next_account_info(account_iter)?)?;
        let market_loan_account: NixAccountInfo<MarketLoansFixed> =
            NixAccountInfo::<MarketLoansFixed>::new(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new(next_account_info(account_iter)?)?;

        let market_fixed: Ref<MarketFixed> = market.get_fixed()?;
        require!(
            market_fixed.get_admin() == admin.key,
            NixError::InvalidAdminKey,
            "Invalid admin. expected {}, got {}",
            market_fixed.get_admin(),
            admin.key,
        )?;
        drop(market_fixed);

        Ok(Self {
            admin,
            market_loan_account,
            market,
        })
    }
}

#[derive(Clone)]
pub struct CancelOrderGlobalTradeAccounts<'a, 'info> {
    pub global: NixAccountInfo<'a, 'info, GlobalFixed>,
    pub system_program: Option<Program<'a, 'info>>,
    pub gas_receiver_opt: Option<Signer<'a, 'info>>,
    pub market: Pubkey,
}

pub(crate) struct CancelOrderContext<'a, 'info> {
    pub payer: Signer<'a, 'info>,
    pub market_loans: NixAccountInfo<'a, 'info, MarketLoansFixed>,
    pub market: NixAccountInfo<'a, 'info, MarketFixed>,
    pub base_global: NixAccountInfo<'a, 'info, GlobalFixed>,
    pub system_program: Program<'a, 'info>,
}

impl<'a, 'info> CancelOrderContext<'a, 'info> {
    pub fn load(
        accounts: &'a [AccountInfo<'info>],
        use_a_tree: bool,
    ) -> Result<Self, ProgramError> {
        let account_iter: &mut Iter<AccountInfo<'info>> = &mut accounts.iter();

        let payer: Signer = Signer::new(next_account_info(account_iter)?)?;
        let market_loans: NixAccountInfo<MarketLoansFixed> =
            NixAccountInfo::<MarketLoansFixed>::new(next_account_info(account_iter)?)?;
        let market: NixAccountInfo<MarketFixed> =
            NixAccountInfo::<MarketFixed>::new(next_account_info(account_iter)?)?;

        let market_fixed: Ref<MarketFixed> = market.get_fixed()?;

        let base_mint_key = if use_a_tree {
            *market_fixed.get_base_a_mint()
        } else {
            *market_fixed.get_base_b_mint()
        };
        drop(market_fixed);

        let base_global: NixAccountInfo<GlobalFixed> =
            NixAccountInfo::<GlobalFixed>::new(next_account_info(account_iter)?)?;
        let base_global_fixed = base_global.get_fixed()?;
        let base_global_mint: &Pubkey = base_global_fixed.get_mint();
        require!(
            base_global_mint == &base_mint_key,
            NixError::InvalidGlobalMint,
            "Invalid base global mint. expected {}, got {}",
            base_mint_key,
            base_global_mint,
        )?;
        drop(base_global_fixed);

        let system_program: Program =
            Program::new(next_account_info(account_iter)?, &system_program::id())?;

        Ok(Self {
            payer,
            market_loans,
            market,
            base_global,
            system_program,
        })
    }
}
