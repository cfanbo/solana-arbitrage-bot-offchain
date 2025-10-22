use borsh::{BorshDeserialize, BorshSerialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::pubkey::{Pubkey, pubkey};
use solana_sdk::instruction::{AccountMeta, Instruction};
use spl_associated_token_account::get_associated_token_address;
use std::cell::Cell;
use std::sync::Arc;
use tracing::error;

const KAMINO_ROGRAM_ID: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const SYSVAR: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
const TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const LENDING_MARKET_AUTH: &[u8] = b"lma";

// trait
pub trait FlashLoan: Send {
    fn borrow(&self, borrow_instruction_index: u8) -> Option<Instruction>;
    fn repay(&self) -> Option<Instruction>;
}

// == NoFlashLoan
pub struct NoFlashLoan;
impl FlashLoan for NoFlashLoan {
    fn borrow(&self, _borrow_instruction_index: u8) -> Option<Instruction> {
        None
    }
    fn repay(&self) -> Option<Instruction> {
        None
    }
}

// == Kamino
pub struct Kamino {
    pub user: Pubkey,
    pub liquidity_amount: u64,
    pub reserve_pubkey: Pubkey,
    pub reserve: Reserve,
    pub borrow_instruction_index: Cell<u8>,
}

impl Kamino {
    pub async fn new(
        rpc_client: Arc<RpcClient>,
        user: Pubkey,
        liquidity_amount: u64,
        reserve_pubkey: Pubkey,
        mint: Pubkey,
    ) -> Kamino {
        let account_data = rpc_client.get_account_data(&reserve_pubkey).await.unwrap();

        let reserve: Reserve;
        match borsh::from_slice::<Reserve>(&account_data[8..]) {
            Ok(tmp_reserve) => {
                // println!("Successfully parsed Reserve: {:#?}", tmp_reserve.collateral);
                assert!(
                    tmp_reserve.liquidity.mint_pubkey.eq(&mint),
                    "don't match mint {mint:?} for user"
                );
                reserve = tmp_reserve;
            }
            Err(e) => {
                error!("Failed to parse Reserve: {}", e);
                error!(
                    "Available data length after discriminator: {}",
                    account_data.len() - 8
                );
                panic!("Expected Reserve size: {}", std::mem::size_of::<Reserve>());
            }
        }

        Kamino {
            user,
            liquidity_amount,
            reserve_pubkey,
            reserve,
            borrow_instruction_index: Cell::new(0),
        }
    }
}

fn lending_market_auth(lending_market: &Pubkey) -> Pubkey {
    let (lending_market_authority, _market_authority_bump) = Pubkey::find_program_address(
        &[LENDING_MARKET_AUTH, lending_market.as_ref()],
        &KAMINO_ROGRAM_ID,
    );
    lending_market_authority
}

#[derive(BorshSerialize)]
struct BorrowArgs {
    pub liquidity_amount: u64,
}
#[derive(BorshSerialize)]
struct RepayArgs {
    pub liquidity_amount: u64,
    pub borrow_instruction_index: u8,
}

impl FlashLoan for Kamino {
    fn borrow(&self, borrow_instruction_index: u8) -> Option<Instruction> {
        let lending_market = self.reserve.lending_market;
        let reserve_market_authority = lending_market_auth(&lending_market);

        let user_ata =
            get_associated_token_address(&self.user, &self.reserve.liquidity.mint_pubkey);
        let accounts = vec![
            // #1 - User Transfer Authority:
            AccountMeta::new(self.user, true),
            // #2 - Lending Market Authority
            AccountMeta::new_readonly(reserve_market_authority, false),
            // #3 - Lending Market:
            AccountMeta::new_readonly(lending_market, false),
            // https://kamino.com/borrow/reserve/7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF
            // // #4 - Reserve: 解析data数据结构  [d4...]
            AccountMeta::new(self.reserve_pubkey, false),
            // https://kamino.com/borrow/reserve/7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF/d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q
            // // #5 - Reserve Liquidity Mint:
            AccountMeta::new_readonly(self.reserve.liquidity.mint_pubkey, false),
            // // #6 - Reserve Source Liquidity:
            AccountMeta::new(self.reserve.liquidity.supply_vault, false),
            // data.suplyVault
            // // #7 - User Destination Liquidity:
            AccountMeta::new(user_ata, false),
            // #8 - Reserve Liquidity Fee Receiver:
            AccountMeta::new(self.reserve.liquidity.fee_vault, false),
            // data.liquidity.feeVault
            // #9 - Referrer Token State:
            AccountMeta::new_readonly(KAMINO_ROGRAM_ID, false),
            // #10 - Referrer Account:
            AccountMeta::new_readonly(KAMINO_ROGRAM_ID, false),
            // #11 - Sysvar Info:
            AccountMeta::new_readonly(SYSVAR, false),
            // #12 - Token Program:
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        ];

        // 参数
        let args = BorrowArgs {
            liquidity_amount: self.liquidity_amount,
        };
        let data = {
            // borrow:  87e734a70734d4c1 0065cd1d00000000
            let discriminator = [0x87, 0xe7, 0x34, 0xa7, 0x07, 0x34, 0xd4, 0xc1];

            let mut d = vec![]; // buy 指令 discriminator 一般是0，根据IDL确认
            d.extend(discriminator);
            d.extend(borsh::to_vec(&args).unwrap());
            d
        };

        self.borrow_instruction_index.set(borrow_instruction_index);

        Some(Instruction {
            program_id: KAMINO_ROGRAM_ID,
            accounts: accounts,
            data: data,
        })
    }

    fn repay(&self) -> Option<Instruction> {
        let lending_market = self.reserve.lending_market;
        let reserve_market_authority = lending_market_auth(&lending_market);

        let user_ata =
            get_associated_token_address(&self.user, &self.reserve.liquidity.mint_pubkey);
        let accounts = vec![
            AccountMeta::new(self.user, true),
            AccountMeta::new_readonly(reserve_market_authority, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new(self.reserve_pubkey, false),
            AccountMeta::new_readonly(self.reserve.liquidity.mint_pubkey, false),
            AccountMeta::new(self.reserve.liquidity.supply_vault, false),
            AccountMeta::new(user_ata, false),
            AccountMeta::new(self.reserve.liquidity.fee_vault, false),
            AccountMeta::new_readonly(KAMINO_ROGRAM_ID, false),
            AccountMeta::new_readonly(KAMINO_ROGRAM_ID, false),
            AccountMeta::new_readonly(SYSVAR, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        ];

        // 参数
        let args = RepayArgs {
            liquidity_amount: self.liquidity_amount,
            borrow_instruction_index: self.borrow_instruction_index.get(), // 固定值
        };
        let data = {
            // repay:  b97500cb60f5b4ba  0065cd1d00000000 02
            let discriminator = [0xb9, 0x75, 0x00, 0xcb, 0x60, 0xf5, 0xb4, 0xba];

            let mut d = vec![]; // buy 指令 discriminator 一般是0，根据IDL确认
            d.extend(discriminator);
            d.extend(borsh::to_vec(&args).unwrap());
            d
        };

        Some(Instruction {
            program_id: KAMINO_ROGRAM_ID,
            accounts: accounts,
            data: data,
        })
    }
}

// https://github.com/Kamino-Finance/klend/blob/c8043038d99b100212f9829db675bb9d0279e796/programs/klend/src/state/reserve.rs#L60-L99
#[derive(Clone, Copy, BorshDeserialize, BorshSerialize, PartialEq, Eq)]
#[repr(C)]
pub struct Reserve {
    pub version: u64,
    pub last_update: LastUpdate,
    pub lending_market: Pubkey,
    pub farm_collateral: Pubkey,
    pub farm_debt: Pubkey,
    pub liquidity: ReserveLiquidity,
    pub reserve_liquidity_padding: [u64; 150],
    pub collateral: ReserveCollateral,
    pub reserve_collateral_padding: [u64; 150],
    pub config: ReserveConfig,
    pub config_padding: [u64; 116],
    pub borrowed_amount_outside_elevation_group: u64,
    pub borrowed_amounts_against_this_reserve_in_elevation_groups: [u64; 32],
    pub padding: [u64; 207],
}

#[derive(Debug, Default, PartialEq, Eq, BorshDeserialize, BorshSerialize, Clone, Copy)]
#[repr(C)]
pub struct ReserveCollateral {
    pub mint_pubkey: Pubkey,
    pub mint_total_supply: u64,
    pub supply_vault: Pubkey,
    pub padding1: [u128; 32],
    pub padding2: [u128; 32],
}

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Eq, Clone, Copy, Default)]
#[repr(C)]
pub struct ReserveConfig {
    pub status: u8,
    pub asset_tier: u8,
    pub host_fixed_interest_rate_bps: u16,
    pub reserved_1: [u8; 9],
    pub protocol_order_execution_fee_pct: u8,
    pub protocol_take_rate_pct: u8,
    pub protocol_liquidation_fee_pct: u8,
    pub loan_to_value_pct: u8,
    pub liquidation_threshold_pct: u8,
    pub min_liquidation_bonus_bps: u16,
    pub max_liquidation_bonus_bps: u16,
    pub bad_debt_liquidation_bonus_bps: u16,
    pub deleveraging_margin_call_period_secs: u64,
    pub deleveraging_threshold_decrease_bps_per_day: u64,
    pub fees: ReserveFees,
    pub borrow_rate_curve: BorrowRateCurve,
    pub borrow_factor_pct: u64,
    pub deposit_limit: u64,
    pub borrow_limit: u64,
    pub token_info: TokenInfo,
    pub deposit_withdrawal_cap: WithdrawalCaps,
    pub debt_withdrawal_cap: WithdrawalCaps,
    pub elevation_groups: [u8; 20],
    pub disable_usage_as_coll_outside_emode: u8,
    pub utilization_limit_block_borrowing_above_pct: u8,
    pub autodeleverage_enabled: u8,
    pub proposer_authority_locked: u8,
    pub borrow_limit_outside_elevation_group: u64,
    pub borrow_limit_against_this_collateral_in_elevation_group: [u64; 32],
    pub deleveraging_bonus_increase_bps_per_day: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
#[repr(C)]
pub struct BorrowRateCurve {
    pub points: [CurvePoint; 11],
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
pub struct CurvePoint {
    pub utilization_rate_bps: u32,
    pub borrow_rate_bps: u32,
}

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Eq, Clone, Copy, Default)]
#[repr(C)]
pub struct TokenInfo {
    pub name: [u8; 32],
    pub heuristic: PriceHeuristic,
    pub max_twap_divergence_bps: u64,
    pub max_age_price_seconds: u64,
    pub max_age_twap_seconds: u64,
    pub scope_configuration: ScopeConfiguration,
    pub switchboard_configuration: SwitchboardConfiguration,
    pub pyth_configuration: PythConfiguration,
    pub block_price_usage: u8,
    pub reserved: [u8; 7],
    pub _padding: [u64; 19],
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq, Default, Clone, Copy)]
#[repr(transparent)]
pub struct PythConfiguration {
    pub price: Pubkey,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq, Default, Clone, Copy)]
#[repr(C)]
pub struct SwitchboardConfiguration {
    pub price_aggregator: Pubkey,
    pub twap_aggregator: Pubkey,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq, Default, Clone, Copy)]
#[repr(C)]
pub struct PriceHeuristic {
    pub lower: u64,
    pub upper: u64,
    pub exp: u64,
}
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
#[repr(C)]
pub struct ScopeConfiguration {
    pub price_feed: Pubkey,
    pub price_chain: [u16; 4],
    pub twap_chain: [u16; 4],
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct LastUpdate {
    slot: u64,
    stale: u8,
    price_status: u8,
    placeholder: [u8; 6],
}

#[derive(BorshDeserialize, BorshSerialize, PartialEq, Eq, Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct WithdrawalCaps {
    pub config_capacity: i64,
    pub current_total: i64,
    pub last_interval_start_timestamp: u64,
    pub config_interval_length_seconds: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Default, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
pub struct ReserveFees {
    pub borrow_fee_sf: u64,
    pub flash_loan_fee_sf: u64,
    pub padding: [u8; 8],
}

#[derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Clone, Copy)]
#[repr(C)]
pub struct ReserveLiquidity {
    pub mint_pubkey: Pubkey,
    pub supply_vault: Pubkey,
    pub fee_vault: Pubkey,
    pub available_amount: u64,
    pub borrowed_amount_sf: u128,
    pub market_price_sf: u128,
    pub market_price_last_updated_ts: u64,
    pub mint_decimals: u64,
    pub deposit_limit_crossed_timestamp: u64,
    pub borrow_limit_crossed_timestamp: u64,
    pub cumulative_borrow_rate_bsf: BigFractionBytes,
    pub accumulated_protocol_fees_sf: u128,
    pub accumulated_referrer_fees_sf: u128,
    pub pending_referrer_fees_sf: u128,
    pub absolute_referral_rate_sf: u128,
    pub token_program: Pubkey,
    pub padding2: [u64; 51],
    pub padding3: [u128; 32],
}

#[derive(Debug, PartialEq, BorshSerialize, BorshDeserialize, Eq, Clone, Copy)]
#[repr(C)]
pub struct BigFractionBytes {
    pub value: [u64; 4],
    pub padding: [u64; 2],
}

/// LendingMarket 主结构体
#[derive(Debug, BorshDeserialize)]
pub struct LendingMarket {
    pub version: u64,
    pub bump_seed: u64,
    pub lending_market_owner: Pubkey,
    pub lending_market_owner_cached: Pubkey,
    pub quote_currency: [u8; 32],
    pub referral_fee_bps: u16,
    pub emergency_mode: u8,
    pub autodeleverage_enabled: u8,
    pub borrow_disabled: u8,
    pub price_refresh_trigger_to_max_age_pct: u8,
    pub liquidation_max_debt_close_factor_pct: u8,
    pub insolvency_risk_unhealthy_ltv_pct: u8,
    pub min_full_liquidation_value_threshold: u64,
    pub max_liquidatable_debt_market_value_at_once: u64,
    pub reserved0: [u8; 8],
    pub global_allowed_borrow_value: u64,
    pub risk_council: Pubkey,
    pub reserved1: [u8; 8],
    pub elevation_groups: [ElevationGroup; 32],
    pub elevation_group_padding: [u64; 90],
    pub min_net_value_in_obligation_sf: u128,
    pub min_value_skip_liquidation_ltv_checks: u64,
    pub name: [u8; 32],
    pub min_value_skip_liquidation_bf_checks: u64,
    pub individual_autodeleverage_margin_call_period_secs: u64,
    pub min_initial_deposit_amount: u64,
    pub obligation_order_execution_enabled: u8,
    pub immutable: u8,
    pub obligation_order_creation_enabled: u8,
    pub padding2: [u8; 5],
    pub padding1: [u64; 169],
}

/// ElevationGroup 子结构体
#[derive(Debug, Clone, PartialEq, BorshDeserialize)]
pub struct ElevationGroup {
    pub id: u64,
    pub max_liquidation_bonus_bps: u64,
    pub liv_pct: u64,
    pub liquidation_threshold_pct: u64,
    pub allow_new_loans: u64,
    pub max_reserves_as_collateral: u64,
    pub padding0: u64,
    pub debt_reserve: Pubkey,
    pub padding1: [u8; 4],
}

#[cfg(test)]
mod tests {
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    use super::*;
    use crate::flashloan::LendingMarket;

    #[tokio::test]
    async fn market_auth() {
        let lending_market = pubkey!("7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF");
        let auth = lending_market_auth(&lending_market);
        assert_eq!(
            auth,
            Pubkey::from_str("9DrvZvyWh1HuAoZxvYWMvkf2XCzryCpGgHqrMjyDWpmo").unwrap()
        );
    }

    #[tokio::test]
    async fn kamino_market_account_info() {
        let lending_market =
            Pubkey::from_str("H6rHXmXoCQvq8Ue81MqNh7ow5ysPa1dSozwW3PU1dDH6").unwrap();
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let account_data = rpc.get_account_data(&lending_market).await.unwrap();
        println!("=== Account Data Analysis ===");
        println!("Data length: {}", account_data.len());
        println!(
            "First 8 bytes as u64: {}",
            u64::from_le_bytes(account_data[..8].try_into().unwrap())
        );

        // 尝试解析为已知结构
        if account_data.len() >= 16 {
            let discriminator = u64::from_le_bytes(account_data[8..16].try_into().unwrap());
            println!("Discriminator: {}", discriminator);
        }

        // 尝试解析为 LendingMarket 结构
        match borsh::from_slice::<LendingMarket>(&account_data[8..]) {
            Ok(market) => {
                println!("Successfully parsed LendingMarket: {:?}", market);
            }
            Err(e) => {
                println!("Failed to parse LendingMarket: {}", e);
                println!(
                    "Available data length after discriminator: {}",
                    account_data.len() - 8
                );
                println!(
                    "Expected LendingMarket size: {}",
                    std::mem::size_of::<LendingMarket>()
                );
            }
        }
    }

    #[tokio::test]
    async fn kamino_reserve_state() {
        let lending_market =
            Pubkey::from_str("d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q").unwrap();
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let account_data = rpc.get_account_data(&lending_market).await.unwrap();
        println!("=== Account Data Analysis ===");
        println!("Data length: {}", account_data.len());

        match borsh::from_slice::<Reserve>(&account_data[8..]) {
            Ok(reserve) => {
                println!("Successfully parsed Reserve: {:#?}", reserve.collateral);
            }
            Err(e) => {
                println!("Failed to parse Reserve: {}", e);
                println!(
                    "Available data length after discriminator: {}",
                    account_data.len() - 8
                );
                println!("Expected Reserve size: {}", std::mem::size_of::<Reserve>());
            }
        }
    }

    #[tokio::test]
    async fn kamino_borrow_ix() {
        let mint_sol_pubkey = pubkey!("So11111111111111111111111111111111111111112");
        let reserve_pubkey =
            Pubkey::from_str("d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q").unwrap();

        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

        let user = pubkey!("2RcgrmyhctsPmYuJmkYApXGj9yqYD3YTybdT4fydxMDZ");
        let kamino = Kamino::new(
            Arc::new(rpc),
            user,
            500000000,
            reserve_pubkey,
            mint_sol_pubkey,
        )
        .await;
        let ix = kamino.borrow(0);
        println!("FlashLoan Instruction: {:#?}", ix);
    }

    #[tokio::test]
    async fn kamino_repay_ix() {
        let mint_sol_pubkey = pubkey!("So11111111111111111111111111111111111111112");
        let reserve_pubkey =
            Pubkey::from_str("d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q").unwrap();

        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

        let user = pubkey!("2RcgrmyhctsPmYuJmkYApXGj9yqYD3YTybdT4fydxMDZ");
        let kamino = Kamino::new(
            Arc::new(rpc),
            user,
            500000000,
            reserve_pubkey,
            mint_sol_pubkey,
        )
        .await;
        let ix = kamino.repay();
        println!("FlashLoan Instruction: {:#?}", ix);
    }
}
