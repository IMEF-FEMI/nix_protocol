use bytemuck::{Pod, Zeroable};
use hypertree::PodBool;
use shank::ShankAccount;
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

use crate::state::OrderType;

/// Serialize and log an event
///
/// Note that this is done instead of a self-CPI, which would be more reliable
/// as explained here
/// <https://github.com/coral-xyz/anchor/blob/59ee310cfa18524e7449db73604db21b0e04780c/lang/attribute/event/src/lib.rs#L104>
/// because the goal of this program is to minimize the number of input
/// accounts, so including the signer for the self CPI is not worth it.
/// Also, be compatible with anchor parsing clients.

#[inline(never)] // ensure fresh stack frame
pub fn emit_stack<T: bytemuck::Pod + Discriminant>(e: T) -> Result<(), ProgramError> {
    // stack buffer, stack frames are 4kb
    let mut buffer: [u8; 3000] = [0u8; 3000];
    buffer[..8].copy_from_slice(&T::discriminant());
    *bytemuck::from_bytes_mut::<T>(&mut buffer[8..8 + std::mem::size_of::<T>()]) = e;

    solana_program::log::sol_log_data(&[&buffer[..(std::mem::size_of::<T>() + 8)]]);
    Ok(())
}

pub trait Discriminant {
    fn discriminant() -> [u8; 8];
}

macro_rules! discriminant {
    ($type_name:ident, $value:ident) => {
        impl Discriminant for $type_name {
            fn discriminant() -> [u8; 8] {
                u64::to_le_bytes(crate::utils::get_discriminant::<$type_name>().unwrap())
            }
        }
    };
}

discriminant!(CreateMarketLog, test_create_market_log);
discriminant!(CreateMarketLoanAccountLog, test_create_market_loan_account_log);
discriminant!(ClaimSeatLog, test_claim_seat_log);

discriminant!(GlobalCreateLog, test_global_create_log);
discriminant!(GlobalAddTraderLog, test_global_add_trader_log);

discriminant!(GlobalDepositLog, test_global_deposit_log);
discriminant!(GlobalCleanupLog, test_global_cleanup_log);

discriminant!(FillLog, test_fill_log);
discriminant!(PlaceOrderLog, test_fill_log);
discriminant!(CancelOrderLog, test_cancel_order_log);

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct CreateMarketLog {
    pub base_a_mint: Pubkey,
    pub base_b_mint: Pubkey,
    pub market_key: Pubkey,
    pub admin: Pubkey,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct CreateMarketLoanAccountLog {
    pub market: Pubkey,
    pub market_loan_account_key: Pubkey,
    pub admin: Pubkey,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct ClaimSeatLog {
    pub market: Pubkey,
    pub trader: Pubkey,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct GlobalCreateLog {
    pub global: Pubkey,
    pub creator: Pubkey,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct GlobalAddTraderLog {
    pub global: Pubkey,
    pub trader: Pubkey,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct GlobalDepositLog {
    pub global: Pubkey,
    pub trader: Pubkey,
    pub deposited_amount: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct GlobalCleanupLog {
    pub cleaner: Pubkey,
    pub maker: Pubkey,
    pub amount_desired: u64,
    pub amount_deposited: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct FillLog {
    pub market: Pubkey,
    pub maker: Pubkey,
    pub taker: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub rate_bps: u16,
    pub _padding: [u8; 6],

    pub base_atoms: u64,
    pub quote_atoms: u64,
    pub maker_sequence_number: u64,
    pub taker_sequence_number: u64,
    pub taker_is_buy: PodBool,
    pub is_maker_global: PodBool,
    pub _padding1: [u8; 14],
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct PlaceOrderLog {
    pub market: Pubkey,
    pub trader: Pubkey,
    pub rate_bps: u16,
    pub _padding: [u8; 6],
    pub base_atoms: u64,
    pub order_sequence_number: u64,
    pub order_index: u32,
    pub last_valid_slot: u32,
    pub order_type: OrderType,
    pub is_bid: PodBool,
    pub _padding1: [u8; 6],
}
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, ShankAccount)]
pub struct CancelOrderLog {
    pub market: Pubkey,
    pub trader: Pubkey,
    pub order_sequence_number: u64,
}