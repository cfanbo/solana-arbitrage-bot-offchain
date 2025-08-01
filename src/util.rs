use crate::types::EncodedInstruction;
use anyhow::{Result, anyhow};
// use base64::{Engine as _, engine::general_purpose};
use crate::config;
use rand::seq::IndexedRandom;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::bs58;
use solana_sdk::instruction::Instruction;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::{program_pack::Pack, pubkey::Pubkey}; // 导入 Pack trait
use spl_token::state::Mint;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use tracing::warn;

pub async fn get_jito_tip_fee_account() -> Result<Pubkey> {
    // let tip_account_str = jito_sdk.get_random_tip_account().await?;
    // let _tip_account = Pubkey::from_str(&tip_account_str)?;
    // Ok(tip_account)
    let accounts = [
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
        "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    ];

    let mut rng = rand::rng();
    let selected = accounts
        .choose(&mut rng)
        .ok_or(anyhow!("Account list is empty"))?;

    let pubkey = selected.parse::<Pubkey>()?;
    Ok(pubkey)
}

// pub fn read_keypair_file(keypair_path: Option<&str>) -> Result<Keypair> {
//     let path: PathBuf = match keypair_path {
//         Some(p) => PathBuf::from(p),
//         None => PathBuf::try_from(config::get_config().private_key.as_str())
//             .unwrap_or_else(|_| default_solana_keypair_path()),
//     };

//     solana_sdk::signer::keypair::read_keypair_file(&path)
//         .map_err(|e| anyhow!("Failed to read keypair file: {}", e))
// }

// solana config directory
#[allow(unused)]
fn default_solana_keypair_path() -> PathBuf {
    let home_dir = dirs::home_dir().expect("Cannot find home directory");
    home_dir.join(".config").join("solana").join("id.json")
}

pub fn latency_too_high(elapsed: Duration) -> bool {
    // 启动HTTP请求延时过高功能
    let max_latency = config::get_config().max_latency_ms_to_duration();
    let result = !max_latency.is_zero() && elapsed >= max_latency;
    result
}

pub fn calculation_jito_tip_amount(profit: i64) -> i64 {
    let jito = config::get_config().jito.clone();
    if jito.tip_rate_enabled {
        let max_tip_amount = jito.max_tip_amount as i64;
        let min_tip_amount = jito.min_tip_amount as i64;

        let rate = jito.tip_rate as i64;

        let tip = profit * rate / 100;
        if tip > max_tip_amount {
            warn!(
                "Jito 小费 {} 超出最大允许配置 {}， 本次小费金额 {}",
                tip, max_tip_amount, max_tip_amount
            );
            max_tip_amount
        } else if tip < min_tip_amount {
            warn!(
                "Jito 小费 {} 远小于Jito最低小费{}， 本次小费金额 {}",
                tip, min_tip_amount, min_tip_amount
            );
            min_tip_amount
        } else {
            tip
        }
    } else {
        jito.fixed_tip_amount as i64
    }
}

pub fn exclude_set_compute_unit_price_ixs(encoded_ixs: &[EncodedInstruction]) -> Vec<Instruction> {
    // https://github.com/solana-labs/solana/blob/master/sdk/src/compute_budget.rs#L25
    encoded_ixs
        .iter()
        .filter_map(|ix| {
            let inst = Instruction::from(ix.clone());
            if inst.data.first() != Some(&0x03) {
                Some(inst)
            } else {
                None
            }
        })
        .collect()
}

pub fn find_set_compute_unit_limit_ix(encoded_ixs: &[EncodedInstruction]) -> Option<Instruction> {
    for ix in encoded_ixs {
        let ix = Instruction::from(ix.clone());
        if ix.data.first() == Some(&0x02) {
            return Some(ix);
        }
    }
    None
}

pub async fn check_mint_address(client: &RpcClient, mint_address: &str) -> Result<Mint> {
    let mint_account = Pubkey::from_str(mint_address)?;

    // 检查账号是否存在
    let account = client.get_account(&mint_account).await?;
    // 检查是否为 Mint 账号
    Mint::unpack(&account.data).map_err(|e| anyhow::anyhow!("Failed to unpack Mint data: {}", e))
}

pub fn sol_to_usd(sol_amount: f64) {
    let sol_price = 172.75;
    let usd = sol_amount * sol_price;
    println!("${} - ¥{}", usd, usd * 7.2);
}

/// 自动解析私钥（支持路径、数组字符串、base58 字符串）
pub fn load_keypair(input: &str) -> Result<Keypair> {
    // 尝试读取为路径（如果存在该文件）
    if Path::new(input).exists() {
        let content = fs::read_to_string(input)?;
        let bytes: Vec<u8> = serde_json::from_str(&content)?;
        return Ok(Keypair::try_from(bytes.as_slice())?);
    }

    // 尝试解析为 JSON 数组字符串
    if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(input) {
        return Ok(Keypair::try_from(bytes.as_slice())?);
    }

    // 尝试 base58 解码（如果你希望支持 base58）
    if let Ok(decoded) = bs58::decode(input).into_vec() {
        return Ok(Keypair::try_from(decoded.as_slice())?);
    }

    Err(anyhow!("Invalid private_key input format"))
}

use std::net::Ipv4Addr;
pub fn parse_ipv4_string(ip_str: &str) -> Result<Vec<Ipv4Addr>> {
    ip_str
        .split(',')
        .map(|s| s.trim()) // 去除前后空格
        .filter(|s| !s.is_empty()) // 过滤空字符串
        .map(|s| Ipv4Addr::from_str(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("无效的IPv4地址: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::compute_budget::ComputeBudgetInstruction;
    use solana_sdk::signature::Signer;
    use solana_sdk::signer::keypair::Keypair;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_find_set_compute_unit_limit_ix() {
        // SetComputeUnitLimit 指令 (0x02 + 小端 u32 = 1,000,000 = 0x40420f00)
        let ix1 = ComputeBudgetInstruction::set_compute_unit_price(2000);
        let ix2 = ComputeBudgetInstruction::set_compute_unit_limit(3333);

        if ix1.data.first() == Some(&0x01) {
            println!("ok");
        }
        if ix2.data.first() == Some(&0x02) {
            println!("ok");
        }

        let encoded_instructions = vec![
            EncodedInstruction {
                program_id: "ComputeBudget111111111111111111111111111111".to_string(),
                accounts: vec![],
                data: "AsBcFQA=".to_string(),
            },
            EncodedInstruction {
                program_id: "ComputeBudget111111111111111111111111111111".to_string(),
                accounts: vec![],
                data: "AwQXAQAAAAAA".to_string(),
            },
        ];
        let ix = find_set_compute_unit_limit_ix(&encoded_instructions).unwrap();
        println!("limit_ix = {:#?}", ix);
        assert_eq!(ix.data.first(), Some(&0x02));
    }

    #[test]
    fn test_exclude_set_compute_unit_price_ixs() {
        let encoded_instructions = vec![
            EncodedInstruction {
                program_id: "ComputeBudget111111111111111111111111111111".to_string(),
                accounts: vec![],
                data: "AsBcFQA=".to_string(),
            },
            EncodedInstruction {
                program_id: "ComputeBudget111111111111111111111111111111".to_string(),
                accounts: vec![],
                data: "AwQXAQAAAAAA".to_string(),
            },
        ];
        let ixs = exclude_set_compute_unit_price_ixs(&encoded_instructions);
        println!("ixs = {:#?}", ixs);
        assert_eq!(ixs[0].data.first(), Some(&0x02));
    }

    #[tokio::test]
    async fn test_check_mint_acount() {
        let rpc_url = "https://api.mainnet-beta.solana.com".to_string();
        let client = RpcClient::new(rpc_url);

        {
            // USDT
            let mint_address = "5goWRao6a3yNC4d6UjMdQxonkCMvKBwdpubU3qhfcdf1"; // USDC Mint
            let ret = check_mint_address(&client, mint_address).await.unwrap();
            assert!(ret.decimals > 0);
            assert!(ret.supply > 0);
        }

        {
            // TOKEN ACCOUNT
            let mint_address = "Vb2RCwVXr2KEKX7Eor6rYxbodYBi7WXGQySPTR2iKDe";
            let ret = check_mint_address(&client, mint_address).await;
            assert!(ret.is_err());
        }

        {
            let mint_address = "TEST";
            let ret = check_mint_address(&client, mint_address).await;
            assert!(ret.is_err());
        }
    }

    #[tokio::test]
    async fn test_load_keypair_from_array_string() {
        let keypair = Keypair::new();
        let keypair_bytes = keypair.to_bytes();
        let json_array = serde_json::to_string(&keypair_bytes.to_vec()).unwrap();
        println!("json_array = {}", json_array);

        let loaded = load_keypair(&json_array).unwrap();
        assert_eq!(keypair.pubkey(), loaded.pubkey());
    }

    #[tokio::test]
    async fn test_load_keypair_from_file() {
        let keypair = Keypair::new();
        let keypair_bytes = keypair.to_bytes();
        let json_array = serde_json::to_string(&keypair_bytes.to_vec()).unwrap();
        println!("json_array = {}", json_array);

        let mut tmp_file = NamedTempFile::new().unwrap();
        write!(tmp_file, "{}", json_array).unwrap();
        let path = tmp_file.path().to_str().unwrap();

        let loaded = load_keypair(path).unwrap();
        assert_eq!(keypair.pubkey(), loaded.pubkey());
    }

    #[tokio::test]
    async fn test_load_keypair_from_base58() {
        let keypair = Keypair::new();
        let encoded = bs58::encode(keypair.to_bytes()).into_string();
        println!("encoded = {}", encoded);

        let loaded = load_keypair(&encoded).unwrap();
        assert_eq!(keypair.pubkey(), loaded.pubkey());
    }

    #[tokio::test]
    async fn test_load_keypair_invalid_input() {
        let result = load_keypair("this is not valid");
        assert!(result.is_err());
    }
}
