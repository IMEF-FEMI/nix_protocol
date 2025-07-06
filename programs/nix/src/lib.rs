#![allow(unexpected_cfgs)]

use hypertree::trace;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};
pub mod logs;
pub mod macros;
pub mod marginfi_utils;
pub mod program;
pub mod quantities;
pub mod state;
pub mod utils;
pub mod validation;
solana_program::declare_id!("Nixjf1STQfCHXdpapnADG41pirqoy4QaUdQoUu8cL5i");

use program::{
    claim_seat::process_claim_seat, create_market::process_create_market, create_market_loan_account::process_create_market_loan_account, deposit::process_deposit, global_add_trader::process_global_add_trader, global_create::process_global_create, global_deposit::process_global_deposit, place_order::process_place_order, NixInstruction
};

pub fn process_instruction<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction_data: &[u8],
) -> ProgramResult{
    let (tag, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    let instruction = NixInstruction::try_from(*tag).or(Err(ProgramError::InvalidAccountData))?;

    trace!("Instruction: {:?}", instruction);

    match instruction {
        NixInstruction::CreateMarket => {
            process_create_market(program_id, accounts, data)?;
        }
        NixInstruction::CreateMarketLoanAccount => {
            process_create_market_loan_account(program_id, accounts, data)?;
        }
        NixInstruction::ClaimSeat => {
            process_claim_seat(program_id, accounts, data)?;
        }
        NixInstruction::Deposit => {
            process_deposit(program_id, accounts, data)?;
        }
        NixInstruction::GlobalCreate => {
            process_global_create(program_id, accounts, data)?;
        }
        NixInstruction::GlobalAddTrader => {
            process_global_add_trader(program_id, accounts, data)?;
        }
        NixInstruction::GlobalDeposit => {
            process_global_deposit(program_id, accounts, data)?;
        }
        NixInstruction::PlaceOrder => {
            process_place_order(program_id, accounts, data)?;
        }

        NixInstruction::CancelOrder => {
            process_cancel_order(program_id, accounts, data)?;
        }
    }
    Ok(()) 
}

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;

use crate::program::cancel_order::process_cancel_order;

#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "nix",
    project_url: "",
    contacts: "",
    policy: "",
    preferred_languages: "en",
    source_code: "",
    auditors: ""
}
