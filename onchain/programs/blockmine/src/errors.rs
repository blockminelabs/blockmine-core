use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The protocol is currently paused.")]
    ProtocolPaused,
    #[msg("Only the configured admin can perform this action.")]
    Unauthorized,
    #[msg("The current block is already closed.")]
    BlockClosed,
    #[msg("The current block has expired.")]
    BlockExpired,
    #[msg("The current block is not stale yet.")]
    BlockNotStale,
    #[msg("The submitted solution does not satisfy the current target.")]
    InvalidSolution,
    #[msg("The provided reward vault does not match the configured vault.")]
    InvalidRewardVault,
    #[msg("The provided treasury vault does not match the configured treasury vault.")]
    InvalidTreasuryVault,
    #[msg("The provided mint does not match the configured BLOC mint.")]
    InvalidMint,
    #[msg("The configured difficulty range is invalid.")]
    InvalidDifficulty,
    #[msg("Difficulty adjustment interval must be greater than zero.")]
    InvalidAdjustmentInterval,
    #[msg("Submit fee must be fixed at 0.01 SOL.")]
    InvalidSubmitFee,
    #[msg("Halving interval must be greater than zero.")]
    InvalidHalvingInterval,
    #[msg("Math overflow.")]
    MathOverflow,
    #[msg("The reward vault has insufficient balance for this payout.")]
    InsufficientVaultBalance,
    #[msg("No more rewards remain in the reward vault.")]
    NoRewardsRemaining,
    #[msg("Treasury fee must be exactly 100 basis points (1%) in V1.")]
    InvalidTreasuryFee,
    #[msg("The provided session delegate is invalid.")]
    InvalidSessionDelegate,
    #[msg("The provided session miner is invalid.")]
    InvalidSessionMiner,
    #[msg("The mining session is inactive.")]
    SessionInactive,
    #[msg("The mining session has expired.")]
    SessionExpired,
    #[msg("The requested session expiry is invalid.")]
    InvalidSessionExpiry,
    #[msg("Stale-block recovery is disabled because block TTL is not configured.")]
    StaleBlockRecoveryDisabled,
}
