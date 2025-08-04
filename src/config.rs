use once_cell::sync::OnceCell;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use solana_sdk::signer::keypair::Keypair;
use std::time::Duration;

use crate::util;

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct Config {
    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_keypair_file")]
    pub private_key: String,

    #[serde(default = "default_frequency")]
    pub frequency: u64,

    #[serde(default)]
    pub simulate_transaction: bool,

    #[serde(default)]
    pub skip_preflight: bool,

    #[serde(default = "default_rpc_endpoint")]
    pub rpc_endpoint: String,

    #[serde(default = "default_jup_v6_api_base_url")]
    pub jup_v6_api_base_url: String,

    #[serde(default = "default_max_latency")]
    pub max_latency_ms: u64,

    #[serde(default = "default_http_request_timeout")]
    pub http_request_timeout: u64,

    #[serde(default = "default_min_profit_amount")]
    pub min_profit_threshold_amount: u64,

    #[serde(default = "default_min_profit_amount")]
    pub min_profit_amount: u64,

    #[serde(default)]
    pub prioritization_fee_lamports: u64,

    #[serde(default)]
    pub ips: String,

    #[serde(default)]
    pub swap: SwapConfig,

    #[serde(default)]
    pub jito: JitoConfig,
}

impl Config {
    pub fn http_request_timeout_to_duration(&self) -> Duration {
        if self.http_request_timeout == 0 {
            Duration::ZERO
        } else {
            Duration::from_millis(self.http_request_timeout)
        }
    }

    pub fn max_latency_ms_to_duration(&self) -> Duration {
        Duration::from_millis(self.max_latency_ms)
    }

    pub fn keypair(&self) -> Keypair {
        util::load_keypair(&self.private_key).unwrap()
    }
}

#[derive(Deserialize, Default, Clone, Debug, Serialize)]
pub struct SwapConfig {
    #[serde(default)]
    pub wrap_and_unwrap_sol: bool,

    #[serde(default = "default_input_mint")]
    pub input_mint: String,

    #[serde(default = "default_output_mint")]
    pub output_mint: String,

    #[serde(default, deserialize_with = "parse_input_amount")]
    pub input_amount: u64,

    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u64,

    #[serde(default)]
    pub dexes: Vec<String>,
    #[serde(default)]
    pub exclude_dexes: Vec<String>,

    #[serde(default)]
    pub only_direct_routes: bool,
    #[serde(default)]
    pub platform_fee_bps: u32,
    #[serde(default)]
    pub dynamic_slippage: bool,
}

fn parse_input_amount<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| DeError::custom("Invalid number for input_amount")),
        Value::String(s) => {
            let s = s.to_lowercase().replace(" ", "");
            if let Some(val) = s.strip_suffix("sol") {
                let sol: f64 = val.parse().map_err(DeError::custom)?;
                Ok((sol * 1_000_000_000f64) as u64)
            } else if let Some(val) = s.strip_suffix("usdc") {
                let usdc: f64 = val.parse().map_err(DeError::custom)?;
                Ok((usdc * 1_000_000f64) as u64)
            } else {
                s.parse::<u64>().map_err(DeError::custom)
            }
        }
        _ => Err(DeError::custom("Unsupported input_amount format")),
    }
}

#[derive(Deserialize, Default, Clone, Debug, Serialize)]
pub struct JitoConfig {
    #[serde(default)]
    pub bundle_submit: bool,

    #[serde(default = "default_jito_rpc_endpoint")]
    pub rpc_endpoint: String,

    #[serde(default = "default_fixed_tip_amount")]
    pub fixed_tip_amount: u64,

    #[serde(default)]
    pub tip_rate_enabled: bool,

    #[serde(default)]
    pub tip_rate: u8,

    #[serde(default = "default_jito_min_tip_amount")]
    pub min_tip_amount: u64,

    #[serde(default = "default_jito_max_tip_amount")]
    pub max_tip_amount: u64,

    #[serde(default)]
    pub bundle_statuses_checking: bool,
}

fn default_min_profit_amount() -> u64 {
    8_000_000
}
fn default_jito_min_tip_amount() -> u64 {
    1000
}
fn default_jito_max_tip_amount() -> u64 {
    5000
}

fn default_fixed_tip_amount() -> u64 {
    1000
}

// 下面写默认值函数
fn default_frequency() -> u64 {
    500
}
fn default_rpc_endpoint() -> String {
    "https://api.mainnet-beta.solana.com".to_string()
}
fn default_jup_v6_api_base_url() -> String {
    "https://lite-api.jup.ag/swap/v1".to_string()
}
fn default_keypair_file() -> String {
    let home_dir = dirs::home_dir().expect("Cannot find home directory");
    home_dir
        .join(".config")
        .join("solana")
        .join("id.json")
        .display()
        .to_string()
}

fn default_log_level() -> String {
    "INFO".to_string()
}
fn default_max_latency() -> u64 {
    0
}
fn default_http_request_timeout() -> u64 {
    3000
}
fn default_input_mint() -> String {
    "So11111111111111111111111111111111111111112".to_string()
}
fn default_output_mint() -> String {
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()
}
fn default_slippage_bps() -> u64 {
    50
}
fn default_jito_rpc_endpoint() -> String {
    "https://tokyo.mainnet.block-engine.jito.wtf".to_string()
}

static CONFIG: OnceCell<Config> = OnceCell::new();

pub fn get_config() -> &'static Config {
    CONFIG.get_or_init(|| {
        let toml_str = std::fs::read_to_string("config.toml").expect("Failed to read config.toml");
        toml::from_str(&toml_str).expect("Failed to parse config.toml")
    })
}
