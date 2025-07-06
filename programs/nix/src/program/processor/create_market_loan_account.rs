use crate::{
    logs::{emit_stack, CreateMarketLoanAccountLog},
    program::expand_market_loans_if_needed,
    state::MarketLoansFixed,
    validation::loaders::CreateMarketLoanAccountContext,
};
use hypertree::{get_mut_helper, trace};
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};
use std::mem::size_of;

pub(crate) fn process_create_market_loan_account(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    process_create_market_loan_account_core(_program_id, accounts, data)
}

pub(crate) fn process_create_market_loan_account_core(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    _data: &[u8],
) -> ProgramResult {
    trace!("process_create_market_loan_account accts={accounts:?}");
    let create_context: CreateMarketLoanAccountContext =
        CreateMarketLoanAccountContext::load(accounts)?;

    let CreateMarketLoanAccountContext {
        admin,
        market_loan_account,
        market,
        ..
    } = &create_context;

    // Do not need to initialize with the system program because it is
    // assumed that it is done already and loaded with rent. That is not at
    // a PDA because we do not want to be restricted to a single market loan
    // account for a market. If there is lock contention and hotspotting for
    // one market loan account or the account is getting close to its full capacity,
    // it could be useful to have a second where it
    // is easier to land transactions and record active loans.

    // Setup the empty market loan account
    let empty_market_loans_fixed: MarketLoansFixed = MarketLoansFixed::new_empty(*market.key);
    assert_eq!(
        market_loan_account.data_len(),
        size_of::<MarketLoansFixed>()
    );

    let market_loan_bytes: &mut [u8] = &mut market_loan_account.try_borrow_mut_data()?[..];
    *get_mut_helper::<MarketLoansFixed>(market_loan_bytes, 0_u32) = empty_market_loans_fixed;

    emit_stack(CreateMarketLoanAccountLog {
        market: *market.key,
        market_loan_account_key: *market_loan_account.key,
        admin: *admin.key,
    })?;
    expand_market_loans_if_needed(&admin, &market_loan_account, 1)?;
    Ok(())
}
