use solana_program::program_error::ProgramError;
use thiserror::Error;

// use crate::program::error;

#[derive(Debug, Error)]
#[repr(u32)]
pub enum NixError {
    #[error("Invalid market parameters error")]
    InvalidMarketParameters = 0,
    #[error("Invalid deposit accounts error")]
    InvalidDepositAccounts = 1,
    #[error("Invalid withdraw accounts error")]
    InvalidWithdrawAccounts = 2,
    #[error("Invalid cancel error")]
    InvalidCancel = 3,
    #[error("Internal free list corruption error")]
    InvalidFreeList = 4,
    #[error("Cannot claim a second seat for the same trader")]
    AlreadyClaimedSeat = 5,
    #[error("Matched on a post only order")]
    PostOnlyCrosses = 6,
    #[error("New order is already expired")]
    AlreadyExpired = 7,
    #[error("Less than minimum out amount")]
    InsufficientOut = 8,
    #[error("Invalid place order from wallet params")]
    InvalidPlaceOrderFromWalletParams = 9,
    #[error("Index hint did not match actual index")]
    WrongIndexHintParams = 10,
    #[error("Price is not positive")]
    PriceNotPositive = 11,
    #[error("Order settlement would overflow")]
    OrderWouldOverflow = 12,
    #[error("Order is too small to settle any value")]
    OrderTooSmall = 13,
    #[error("Numerical overflow in token calculation")]
    NumericalOverflow = 14,
    #[error("Missing Global account")]
    MissingGlobal = 15,
    #[error("Insufficient funds on global account to rest an order")]
    GlobalInsufficient = 16,
    #[error("Account key did not match expected")]
    IncorrectAccount = 17,
    #[error("Mint not allowed for market")]
    InvalidMint = 18,
    #[error("Cannot claim a new global seat, use evict")]
    TooManyGlobalSeats = 19,
    #[error("Global order cannot be bid")]
    InvalidGlobalBidOrder = 20,
    #[error("Can only evict the lowest depositor")]
    InvalidEvict = 21,
    #[error("Tried to clean order that was not eligible to be cleaned")]
    InvalidClean = 22,
    #[error("Invalid Marginfi Account")]
    InvalidMarginfiAccount = 23,
    #[error("Marginfi bank does not have an oracle configured")]
    OracleNotSetup = 24,
    #[error("Incorrect oracle account")]
    IncorrectOracleAccount = 25,
    #[error("Marginfi account initialization failed")]
    MarginfiAccountInitializationFailed = 26,
    #[error("Invalid oracle Account")]
    InvalidOracleAccount = 27,
    #[error("Pricing math error")]
    PriceOracleMathError = 28,
    #[error("Oracle price is stale")]
    StaleOracle = 29,
    #[error("Invalid Price")]
    InvalidPrice = 30,
    #[error("Invalid switchboard decimal conversion")]
    InvalidSwitchboardDecimalConversion = 31,
    #[error("PushPush Oracle: wrong account owner")]
    PythPushWrongAccountOwner = 32,
    #[error("Invalid Fee Receiver PDA")]
    InvalidFeeReceiver = 33,
    #[error("Invalid Vault PDA")]
    InvalidVault = 34,
    #[error("Invalid Marginfi Group")]
    InvalidMarginfiGroup = 35,
    #[error("Invalid Marginfi Bank")]
    InvalidMarginfiBank = 36,
    #[error("Invalid Marginfi Vault")]
    InvalidMarginfiLiquidityVault = 37,
    #[error("Marginfi CPI failed")]
    MarginfiCpiFailed = 38,
    #[error("Invalid Marginfi state")]
    InvalidMarginfiState = 39,
    #[error("Maximum number of active loans exceeded")]
    MaxActiveLoansExceeded = 40,
    #[error("Invalid Active Loan")]
    InvalidActiveLoan = 41,
    #[error("Invalid ReverseOrder ")]
    InvalidAskReverseOrder = 42,
    #[error("Invalid Admin Key")]
    InvalidAdminKey = 43,
    #[error("Invalid Global Mint")]
    InvalidGlobalMint = 44,
}

impl From<NixError> for ProgramError {
    fn from(e: NixError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

#[macro_export]
macro_rules! require {
  ($test:expr, $err:expr, $($arg:tt)*) => {
    if $test {
        Ok(())
    } else {
        #[cfg(target_os = "solana")]
        solana_program::msg!("[{}:{}] {}", std::file!(), std::line!(), std::format_args!($($arg)*));
        #[cfg(not(target_os = "solana"))]
        std::println!("[{}:{}] {}", std::file!(), std::line!(), std::format_args!($($arg)*));
        Err(($err))
    }
  };
}
