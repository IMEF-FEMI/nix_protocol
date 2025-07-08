use num_enum::TryFromPrimitive;
use shank::ShankInstruction;

#[repr(u8)]
#[derive(TryFromPrimitive, Debug, Copy, Clone, ShankInstruction, PartialEq, Eq)]
#[rustfmt::skip]
pub enum NixInstruction {
    /// Create a market
    #[account(0, writable, signer, name = "admin", desc = "Admin account")]
    #[account(1, writable, name = "market", desc = "Market state account")]
    #[account(2, name = "system_program", desc = "System program")]
    #[account(3, name = "token_program", desc = "Token program")]
    #[account(4, name = "token_program_22", desc = "Token Program 2022")]
    #[account(5, name = "base_a_mint", desc = "Base A mint")]
    #[account(6, name = "base_b_mint", desc = "Base B mint")]
    // Base A accounts
    #[account(7, writable, name = "base_a_fee_receiver", desc = "Base A fee receiver PDA")]
    #[account(8, writable, name = "base_a_vault", desc = "Base A vault PDA")]
    #[account(9, name = "base_a_marginfi_group", desc = "Base A Marginfi group")]
    #[account(10, name = "base_a_marginfi_bank", desc = "Base A Marginfi bank")]
    #[account(11, name = "base_a_marginfi_account", desc = "Base A Marginfi account PDA")]
    // Base B accounts
    #[account(12, writable, name = "base_b_fee_receiver", desc = "Base B fee receiver PDA")]
    #[account(13, writable, name = "base_b_vault", desc = "Base B vault PDA")]
    #[account(14, name = "base_b_marginfi_group", desc = "Base B Marginfi group")]
    #[account(15, name = "base_b_marginfi_bank", desc = "Base B Marginfi bank")]
    #[account(16, name = "base_b_marginfi_account", desc = "Base B Marginfi account PDA")]
    CreateMarket = 0,

    /// Create a market loan account
    #[account(0, writable, signer, name = "admin", desc = "Admin account")]
    #[account(1, writable, name = "market_loan_account", desc = "Market loan state account")]
    #[account(2, name = "system_program", desc = "System program")]
    CreateMarketLoanAccount = 1,

    /// Allocate a seat
    #[account(0, writable, signer, name = "payer", desc = "Payer")]
    #[account(1, writable, name = "market", desc = "Account holding all market state")]
    #[account(2, name = "system_program", desc = "System program")]
    ClaimSeat = 2,

    /// Deposit
    #[account(0, writable, signer, name = "payer", desc = "Payer")]
    #[account(1, writable, name = "market", desc = "Account holding all market state")]
    #[account(2, name = "mint", desc = "Required for token22 transfer_checked")]
    #[account(3, writable, name = "trader_token", desc = "Trader token account")]
    #[account(4, name = "token_program", desc = "Token program(22), should be the version that aligns with the token being used")]
    #[account(5, writable, name = "vault", desc = "vault PDA, seeds are [b'vault', market, mint]")]
    #[account(6, name = "marginfi_group", desc = "Marginfi group")]
    #[account(7, name = "marginfi_bank", desc = "Marginfi bank")]
    #[account(8, name = "marginfi_account", desc = "Marginfi account PDA")]
    #[account(9, name = "marginfi_liquidity_vault", desc = "Marginfi liquidity vault. constraint => bank.liquidity_vault == liquidity_vault")]
    Deposit = 3,
    
    /// Create global account for a given token.
    #[account(0, writable, signer, name = "payer", desc = "Payer")]
    #[account(1, writable, name = "global", desc = "Global account")]
    #[account(2, name = "system_program", desc = "System program")]
    #[account(3, name = "mint", desc = "Mint for this global account")]
    #[account(4, writable, name = "global_vault", desc = "Global vault")]
    #[account(5, name = "token_program", desc = "Token program(22)")]
    GlobalCreate = 4,


    /// Add a trader to the global account.
    #[account(0, writable, signer, name = "payer", desc = "Payer")]
    #[account(1, writable, name = "global", desc = "Global account")]
    #[account(2, name = "system_program", desc = "System program")]
    GlobalAddTrader = 5,


    /// Deposit into global account for a given token.
    #[account(0, writable, signer, name = "payer", desc = "Payer")]
    #[account(1, writable, name = "global", desc = "Global account")]
    #[account(2, name = "mint", desc = "Mint for this global account")]
    #[account(3, writable, name = "global_vault", desc = "Global vault")]
    #[account(4, writable, name = "trader_token", desc = "Trader token account")]
    #[account(5, name = "token_program", desc = "Token program(22)")]
    GlobalDeposit = 6,
    
    /// Place an order on the market
    #[account(0, writable, signer, name = "payer", desc = "Trader placing the order")]
    #[account(1, writable, name = "market", desc = "Market state account")]
    #[account(2, writable, name = "market_loans", desc = "Market loans account")]
    #[account(3, name = "market_signer", desc = "Market signer PDA")]
    #[account(4, name = "system_program", desc = "System program")]
    #[account(5, name = "base_mint", desc = "Base token mint")]
    #[account(6, name = "quote_mint", desc = "Quote token mint")]
    // Optional global trading accounts (up to 2 sets of 4 accounts each)
    #[account(7, writable, name = "global_1", desc = "Global account 1 (optional)")]
    #[account(8, writable, name = "global_vault_1", desc = "Global vault 1 (optional)")]
    #[account(9, writable, name = "market_vault_1", desc = "Market vault 1 (optional)")]
    #[account(10, name = "token_program_1", desc = "Token program 1 (optional)")]
    #[account(11, writable, name = "global_2", desc = "Global account 2 (optional)")]
    #[account(12, writable, name = "global_vault_2", desc = "Global vault 2 (optional)")]
    #[account(13, writable, name = "market_vault_2", desc = "Market vault 2 (optional)")]
    #[account(14, name = "token_program_2", desc = "Token program 2 (optional)")]
    // Marginfi CPI accounts (2 required sets of 5 accounts each)
    #[account(15, name = "marginfi_group_1", desc = "Marginfi group 1")]
    #[account(16, name = "marginfi_bank_1", desc = "Marginfi bank 1")]
    #[account(17, name = "marginfi_account_1", desc = "Marginfi account 1")]
    #[account(18, writable, name = "marginfi_liquidity_vault_1", desc = "Marginfi liquidity vault 1")]
    #[account(19, name = "marginfi_liquidity_vault_authority_1", desc = "Marginfi vault authority 1")]
    #[account(20, name = "marginfi_group_2", desc = "Marginfi group 2")]
    #[account(21, name = "marginfi_bank_2", desc = "Marginfi bank 2")]
    #[account(22, name = "marginfi_account_2", desc = "Marginfi account 2")]
    #[account(23, writable, name = "marginfi_liquidity_vault_2", desc = "Marginfi liquidity vault 2")]
    #[account(24, name = "marginfi_liquidity_vault_authority_2", desc = "Marginfi vault authority 2")]
    PlaceOrder = 7,
    
    /// Cancel an existing order
    #[account(0, writable, signer, name = "payer", desc = "Order owner/signer")]
    #[account(1, writable, name = "market_loans", desc = "Market loans account")]
    #[account(2, writable, name = "market", desc = "Market state account")]
    #[account(3, writable, name = "base_global", desc = "Global account for base mint")]
    #[account(4, name = "system_program", desc = "System program")]
    CancelOrder = 8,

}

impl NixInstruction {
    pub fn to_vec(&self) -> Vec<u8> {
        vec![*self as u8]
    }
}
