use base64::{Engine as _, engine::general_purpose};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteReuqest {
    pub input_mint: String,
    pub output_mint: String,
    pub amount: u64,
    pub slippage_bps: u64,
    pub dexes: Vec<String>,
    pub exclude_dexes: Vec<String>,
    pub only_direct_routes: bool,
    pub platform_fee_bps: u32,
    pub dynamic_slippage: bool,
    // marketInfos: serde_json::Value,
}

impl Serialize for QuoteReuqest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // 判断是否应该序列化 dexes
        let use_dexes = !self.dexes.is_empty();
        // 判断是否应该序列化 exclude_dexes (仅当 dexes 为空且 exclude_dexes 非空时)
        let use_exclude_dexes = !use_dexes && !self.exclude_dexes.is_empty();

        // 计算需要序列化的字段数
        let field_count = 7 + (if use_dexes || use_exclude_dexes { 1 } else { 0 });

        let mut state = serializer.serialize_struct("QuoteRequest", field_count)?;

        state.serialize_field("inputMint", &self.input_mint)?;
        state.serialize_field("outputMint", &self.output_mint)?;
        state.serialize_field("amount", &self.amount)?;
        state.serialize_field("slippageBps", &self.slippage_bps)?;

        // 选择性地序列化 dexes 或 exclude_dexes
        if use_dexes {
            let dexes_string = self.dexes.join(",");
            state.serialize_field("dexes", &dexes_string)?;
        } else if use_exclude_dexes {
            let exclude_dexes_string = self.exclude_dexes.join(",");
            state.serialize_field("excludeDexes", &exclude_dexes_string)?;
        }

        state.serialize_field("onlyDirectRoutes", &self.only_direct_routes)?;
        state.serialize_field("platformFeeBps", &self.platform_fee_bps)?;
        state.serialize_field("dynamicSlippage", &self.dynamic_slippage)?;

        state.end()
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    pub input_mint: String,
    pub in_amount: String,
    pub output_mint: String,
    pub out_amount: String,
    pub other_amount_threshold: String,
    pub swap_mode: String,
    pub slippage_bps: u64,
    pub platform_fee: Option<PlatformFee>,
    pub price_impact_pct: String,
    pub route_plan: Vec<RoutePlan>,
    pub context_slot: u64,
    pub time_taken: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformFee {
    pub amount: String,
    pub fee_bps: u64,
    pub fee_mint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    pub swap_info: SwapInfo,
    pub percent: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    pub amm_key: String,
    pub label: String,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: String,
    pub out_amount: String,
    pub fee_amount: String,
    pub fee_mint: String,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SwapRequest {
    pub quote_response: QuoteResponse,
    pub user_public_key: String,
    pub payer: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_and_unwrap_sol: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_account: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_legacy_transaction: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prioritization_fee_lamports: Option<PrioritizationFeeLamports>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrioritizationFeeLamports {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_level_with_max_lamports: Option<PriorityLevelWithMaxLamports>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jito_tip_lamports: Option<u64>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PriorityLevelWithMaxLamports {
    pub priority_level: Option<String>,
    pub max_lamports: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    pub compute_budget_instructions: Vec<EncodedInstruction>,
    pub setup_instructions: Vec<EncodedInstruction>,
    pub swap_instruction: EncodedInstruction,
    #[serde(default)]
    pub cleanup_instruction: Option<EncodedInstruction>,
    pub other_instructions: Vec<EncodedInstruction>,
    pub address_lookup_table_addresses: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedInstruction {
    pub program_id: String,
    pub data: String,                      // base64 编码
    pub accounts: Vec<EncodedAccountMeta>, // pubkey 字符串
}
impl From<EncodedInstruction> for Instruction {
    fn from(encoded: EncodedInstruction) -> Self {
        Instruction {
            program_id: encoded
                .program_id
                .parse::<Pubkey>()
                .expect("Invalid program_id"),
            data: general_purpose::STANDARD
                .decode(encoded.data)
                .expect("Invalid base64 data"),
            accounts: encoded
                .accounts
                .into_iter()
                .map(Into::into)
                .collect::<Vec<AccountMeta>>(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedAccountMeta {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl From<EncodedAccountMeta> for AccountMeta {
    fn from(encoded: EncodedAccountMeta) -> Self {
        AccountMeta {
            pubkey: encoded.pubkey.parse::<Pubkey>().expect("Invalid pubkey"),
            is_signer: encoded.is_signer,
            is_writable: encoded.is_writable,
        }
    }
}

#[derive(Debug)]
pub struct SwapData {
    pub data1: QuoteResponse,
    pub data2: QuoteResponse,
}
