use thiserror::Error;

/// See https://solscan.io/account/JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4#anchorProgramIdl
#[derive(Debug, Error)]
pub enum SwapError {
    /// jup 官方错误
    #[error("Empty route")]
    EmptyRoute,
    #[error("Slippage tolerance exceeded")]
    SlippageToleranceExceeded,
    #[error("Invalid calculation")]
    InvalidCalculation,
    #[error("Missing platform fee account")]
    MissingPlatformFeeAccount,
    #[error("Invalid slippage")]
    InvalidSlippage,
    #[error("Not enough percent to 100")]
    NotEnoughPercent,
    #[error("Token input index is invalid")]
    InvalidInputIndex,
    #[error("Token output index is invalid")]
    InvalidOutputIndex,
    #[error("Not Enough Account keys")]
    NotEnoughAccountKeys,
    #[error("Non zero minimum out amount not supported")]
    NonZeroMinimumOutAmountNotSupported,
    #[error("Invalid route plan")]
    InvalidRoutePlan,
    #[error("Invalid referral authority")]
    InvalidReferralAuthority,
    #[error("Token account doesn't match the ledger")]
    LedgerTokenAccountDoesNotMatch,
    #[error("Invalid token ledger")]
    InvalidTokenLedger,
    #[error("Token program ID is invalid")]
    IncorrectTokenProgramID,
    #[error("Token program not provided")]
    TokenProgramNotProvided,
    #[error("Swap not supported")]
    SwapNotSupported,
    #[error("Exact out amount doesn't match")]
    ExactOutAmountNotMatched,
    #[error("Source mint and destination mint cannot the same")]
    SourceAndDestinationMintCannotBeTheSame,
    #[error("Invalid mint")]
    InvalidMint,
    #[error("Invalid program authority")]
    InvalidProgramAuthority,
    #[error("Invalid output token account")]
    InvalidOutputTokenAccount,
    #[error("Invalid fee wallet")]
    InvalidFeeWallet,
    #[error("Invalid authority")]
    InvalidAuthority,
    #[error("Insufficient funds")]
    InsufficientFunds,
    #[error("Invalid token account")]
    InvalidTokenAccount,

    // 利润保证程序合约错误
    #[error("No profitable arbitrage found")]
    NoProfitableFound,
}

impl SwapError {
    pub fn from_code(code: u64) -> Option<Self> {
        use SwapError::*;
        Some(match code {
            6000 => EmptyRoute,
            6001 => SlippageToleranceExceeded,
            6002 => InvalidCalculation,
            6003 => MissingPlatformFeeAccount,
            6004 => InvalidSlippage,
            6005 => NotEnoughPercent,
            6006 => InvalidInputIndex,
            6007 => InvalidOutputIndex,
            6008 => NotEnoughAccountKeys,
            6009 => NonZeroMinimumOutAmountNotSupported,
            6010 => InvalidRoutePlan,
            6011 => InvalidReferralAuthority,
            6012 => LedgerTokenAccountDoesNotMatch,
            6013 => InvalidTokenLedger,
            6014 => IncorrectTokenProgramID,
            6015 => TokenProgramNotProvided,
            6016 => SwapNotSupported,
            6017 => ExactOutAmountNotMatched,
            6018 => SourceAndDestinationMintCannotBeTheSame,
            6019 => InvalidMint,
            6020 => InvalidProgramAuthority,
            6021 => InvalidOutputTokenAccount,
            6022 => InvalidFeeWallet,
            6023 => InvalidAuthority,
            6024 => InsufficientFunds,
            6025 => InvalidTokenAccount,

            // 利润保护合约错误
            100 => NoProfitableFound,

            _ => return None,
        })
    }
}
