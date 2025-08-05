use crate::blockhash::LatestBlockhash;
use crate::config::SwapConfig;
use crate::http_client::{HttpClient, IpSelectAlgorithm};
use crate::types::{
    PrioritizationFeeLamports, PriorityLevelWithMaxLamports, QuoteResponse, QuoteReuqest, SwapData,
    SwapRequest, SwapResponse,
};
use crate::{config, constants, error::SwapError, util};
use anyhow::{Result, anyhow};
use backoff::ExponentialBackoff;
use backoff::future::retry;
use base64::Engine as _;
use jito_sdk_rust::{JitoJsonRpcSDK, http_client::IpSelectAlgorithm as JitoAlgorithm};
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_program::address_lookup_table::state::AddressLookupTable;
use solana_sdk::instruction::AccountMeta;
use solana_sdk::timing::timestamp;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::{AddressLookupTableAccount, VersionedMessage, v0::Message as V0Message},
    pubkey::Pubkey,
    signature::Keypair,
    signature::Signature,
    signer::Signer,
    transaction::VersionedTransaction,
};
use solana_sdk_ids::system_program;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{Duration, Instant, sleep};
use tracing::{Level, debug, error, info};

/// 加载 ALT（Address Lookup Table）账户
async fn load_alt_accounts(
    rpc: &RpcClient,
    alt_addresses: &[String],
) -> Vec<AddressLookupTableAccount> {
    let mut accounts = Vec::with_capacity(alt_addresses.len());

    for alt in alt_addresses {
        let address_lookup_table_key = Pubkey::from_str(alt).unwrap();
        let raw_account = rpc.get_account(&address_lookup_table_key).await.unwrap();
        let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();
        accounts.push(AddressLookupTableAccount {
            key: address_lookup_table_key,
            addresses: address_lookup_table.addresses.to_vec(),
        });
    }

    accounts
}

// #[derive(Debug)]
pub struct Engine {
    http_client: HttpClient,
    swap_channel_tx: Sender<SwapData>,
}

impl Engine {
    pub async fn new() -> Self {
        let config = config::get_config();

        let ip_pool = util::parse_ipv4_string(&config.ips).unwrap();
        let http_client = HttpClient::initialize(ip_pool, IpSelectAlgorithm::RoundRobin).unwrap();

        let rpc_endpoint = config.rpc_endpoint.clone();
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            rpc_endpoint.clone(),
            CommitmentConfig::confirmed(),
        ));

        let (tx, rx) = mpsc::channel(100);

        // daemon
        Engine::daemon_processor(rpc_client.clone(), http_client.clone(), rx).await;

        Self {
            http_client,
            swap_channel_tx: tx,
        }
    }

    async fn daemon_processor(
        rpc_client: Arc<RpcClient>,
        http_client: HttpClient,
        mut rx: Receiver<SwapData>,
    ) {
        let config = config::get_config();

        let payer = Arc::new(config.keypair());
        let user_pubkey = payer.pubkey();

        let rpc_endpoint = config.rpc_endpoint.clone();

        let lastest_blockhash = LatestBlockhash::start(rpc_client.clone()).await;
        // BlockhashCache::init(&rpc_endpoint, Duration::from_secs(30)).await;

        let balance: u64;
        // WELCOME
        {
            let SwapConfig {
                input_mint,
                output_mint,
                input_amount,
                slippage_bps,
                ..
            } = config.swap.clone();

            if input_mint.eq(&output_mint.clone()) {
                panic!("INPUT_MINT must not be equal to OUTPUT_MINT ")
            }
            assert!(
                util::check_mint_address(&rpc_client.clone(), &input_mint)
                    .await
                    .is_ok(),
                "INPUT_MINT 无效"
            );
            assert!(
                util::check_mint_address(&rpc_client.clone(), &output_mint)
                    .await
                    .is_ok(),
                "OUTPUT_MINT 无效"
            );

            println!("Current Configuration Information");
            println!("  钱包地址: {}", user_pubkey);

            balance = rpc_client.get_balance(&user_pubkey).await.unwrap();
            println!("  钱包余额: {}", balance);
            println!("  INPUT_MINT: {}", input_mint);
            println!("  OUTPUT_MINT: {}", output_mint);
            println!("  INPUT_AMOUNT: {}", input_amount);
            println!("  滑点: {}%", slippage_bps as f64 / 100.0);
            println!("  Solana RPC 端点: {}", rpc_endpoint);
            println!("  JUP_V6_API_BASE_URL: {}", config.jup_v6_api_base_url);
            println!(
                "  Http 请求超时: {:?}",
                config.http_request_timeout_to_duration()
            );
            println!(
                "  报价最大延时(两次Http请求): {:?}",
                config.max_latency_ms_to_duration()
            );
            println!(
                "  日志级别: {}",
                Level::from_str(&config.log_level).unwrap_or(Level::INFO)
            );
            println!(
                "  利润阈值: {} Lamports (不含任何优先费或Jito Tip)",
                config.min_profit_threshold_amount
            );
            println!("  启用 Jito Bundle 提交: {}", config.jito.bundle_submit);
            if config.jito.bundle_submit {
                println!(
                    "       Jito Tip 计算方式: {}",
                    if config.jito.tip_rate_enabled {
                        "百分比"
                    } else {
                        "固定"
                    }
                );

                println!("       Endpoint Base URL: {}", config.jito.rpc_endpoint);
                if config.jito.tip_rate_enabled {
                    println!("      百分比: {} % (利润)", config.jito.tip_rate_enabled,);
                    println!("       最大小费: {} Lamports", config.jito.max_tip_amount);
                } else {
                    println!(
                        "       Jito 小费金额: {} Lamports",
                        config.jito.fixed_tip_amount
                    );
                }
            }
            println!("-----------------------------------------------------\n");
        }

        // 余额检查
        if !config.simulate_transaction {
            if balance == 0 {
                panic!("账户 {} 余额不足! {} lamports", &user_pubkey, balance);
            }
        }

        let jsdk = if !config.ips.is_empty() {
            let ip_pool = util::parse_ipv4_string(&config.ips).unwrap();
            info!("Jito IP Pool Enabled");
            JitoJsonRpcSDK::new_with_ip_pool(
                &config.jito.rpc_endpoint,
                None,
                ip_pool.into_iter().map(IpAddr::V4).collect(),
                JitoAlgorithm::Random,
            )
            .unwrap()
        } else {
            JitoJsonRpcSDK::new(&config.jito.rpc_endpoint, None)
        };
        let jito_sdk = Arc::new(jsdk);

        // [线程] jito bundle_id 状态检查
        let (jito_tx, mut jito_rx) = tokio::sync::mpsc::channel::<String>(1000);
        if config.jito.bundle_submit && config.jito.bundle_statuses_checking {
            let jito_sdk_clone = Arc::clone(&jito_sdk);
            tokio::spawn(async move {
                while let Some(bundle_uuid) = jito_rx.recv().await {
                    let jito_sdk_clone = Arc::clone(&jito_sdk_clone);
                    tokio::spawn(async move {
                        match check_bundle_id_status(&jito_sdk_clone, &bundle_uuid).await {
                            Ok(ret) => println!("👌 {:?} [{:?}]", bundle_uuid, ret),
                            Err(e) => println!("{}", e),
                        }
                    });
                }
            });
        }

        info!("后台处理线程已启动...");
        let jito_sdk_clone = Arc::clone(&jito_sdk);
        let lastest_blockhash = lastest_blockhash.clone();
        // [线程] 创建一个后台任务持续处理消息
        tokio::spawn(async move {
            // 循环接收消息直到通道关闭
            while let Some(data) = rx.recv().await {
                // debug!("quote_response = {:#?}", &data);
                let jito_tx_clone = jito_tx.clone();
                let http_client = http_client.clone();
                let rpc_client = rpc_client.clone();
                let payer = payer.clone();
                let lastest_blockhash = lastest_blockhash.clone();

                let jito_sdk_clone = Arc::clone(&jito_sdk_clone);
                tokio::spawn(async move {
                    let start_time = Instant::now();
                    match Engine::send_transaction(
                        http_client,
                        Arc::clone(&rpc_client),
                        &jito_sdk_clone,
                        jito_tx_clone,
                        data,
                        user_pubkey.clone(),
                        &payer,
                        lastest_blockhash,
                    )
                    .await
                    {
                        Ok(_) => {
                            debug!("⏱️ transaction slapsed_time : {:.4?}", start_time.elapsed());
                        }
                        Err(e) => {
                            debug!("⏱️ transaction slapsed_time : {:.4?}", start_time.elapsed());
                            error!("{}", e)
                        }
                    }
                });
            }

            // println!("后台处理线程成功退出...");
        });
    }

    async fn send_transaction(
        http_client: HttpClient,
        rpc_client: Arc<RpcClient>,
        jito_sdk: &JitoJsonRpcSDK,
        jito_tx: Sender<String>,
        data: SwapData,
        user_pubkey: Pubkey,
        payer: &Keypair,
        lastest_blockhash: Arc<LatestBlockhash>,
    ) -> Result<()> {
        let txs = match Engine::build_tx(
            &http_client,
            data,
            user_pubkey,
            &payer,
            Arc::clone(&rpc_client),
            lastest_blockhash,
        )
        .await
        {
            Ok(txs) => txs,
            Err(e) => {
                return Err(e);
            }
        };

        let config = config::get_config();

        // 交易大小检查
        if config.simulate_transaction {
            if txs.is_empty() {
                return Err(anyhow!("未发现任何交易"));
            }
            if txs.len() > 1 {
                return Err(anyhow!(
                    "👾 当前交易数据大小 超出Solana允许的单笔交易大小 1232 字节",
                ));
            }

            debug!("本次模拟交易共有 {} 笔\ntxs_info = {:?}", txs.len(), txs);
            for tx in &txs {
                match rpc_client.simulate_transaction(tx).await {
                    Ok(ret) => {
                        if let Some(e) = &ret.value.err {
                            println!("❌ simulate_transaction Failed, Err: {}", e);
                            return Err(anyhow!("❌ 交易模拟失败\n {:#?}", ret));
                        } else {
                            println!("✅ simulate_transaction SUCCESS = {:?}\n\n", ret);
                        }
                    }
                    Err(e) => {
                        return Err(anyhow!("🥶 simulate_transaction FAILED: {}\n\n", e));
                    }
                }
            }
            return Ok(());
        } else {
            // JITO bundle
            if config.jito.bundle_submit {
                // 模拟交易DEBUG
                // for tx in txs.iter() {
                //     let sim = rpc_client.simulate_transaction(tx).await?;
                //     println!("{:#?}", sim);
                // }

                // =========================== 打包交易
                let encoded_txs: Vec<String> = txs
                    .iter()
                    .map(|tx| {
                        let encoded_tx = base64::engine::general_purpose::STANDARD
                            .encode(bincode::serialize(&tx).unwrap());
                        encoded_tx
                    })
                    .collect();

                let transactions = json!(encoded_txs);
                let params = json!([transactions, {"encoding": "base64"}]);

                match jito_sdk.send_bundle(Some(params), None).await {
                    Ok(res) => {
                        if res.get("result").is_some() {
                            println!("✅ Bundle sent to JITO with UUID: {}", res["result"]);
                            for (idx, tx) in txs.iter().enumerate() {
                                println!("✅ 交易{}: {:?}", idx + 1, tx.signatures[0]);
                            }

                            // 启用打包状态检测功能
                            if config.jito.bundle_statuses_checking {
                                _ = jito_tx.send(res["result"].to_string()).await;
                            }

                            return Ok(());
                        } else {
                            return Err(anyhow!(
                                "❌ Failed to get bundle UUID from response, {}",
                                res
                            ));
                        }
                    }
                    Err(e) => {
                        return Err(anyhow!("❌ Failed to get bundle UUID from response, {}", e));
                    }
                }
            } else {
                // 普通提交
                let size = bincode::serialize(&txs[0])?.len();
                if size > constants::TX_SIZE {
                    return Err(anyhow!(
                        "❌ 当前交易数据大小 {} 超出Solana允许的单笔交易大小 1232 字节",
                        size
                    ));
                }
                // debug!("tx_info = {:?}", &txs[0]);

                // let sim = rpc_client.simulate_transaction(&txs[0]).await?;
                // if let Some(e) = &sim.value.err {
                //     println!("simulate_transaction Failed, Err: {}", e);
                //     return Err(anyhow!("❌ 交易模拟失败\n {:#?}", sim));
                // }

                let skip_preflight = config.skip_preflight;
                match send_transaction_with_options(&rpc_client, &txs[0], skip_preflight).await {
                    Ok(signature) => {
                        println!("✅ 成功发送交易: https://solscan.io/tx/{}\n", signature);
                        return Ok(());
                    }
                    Err(e) => match extract_program_error(&e) {
                        Some((ix, code)) => {
                            if let Some(e) = SwapError::from_code(code) {
                                anyhow::bail!(
                                    "❗ 指令 #{} 失败，错误码: {} (0x{:x})，{}",
                                    ix,
                                    code,
                                    code,
                                    e
                                );
                            }

                            anyhow::bail!("指令 #{} 失败，错误码: {} (0x{:x})", ix, code, code);
                        }
                        None => {
                            anyhow::bail!("❌ 交易失败: {}", e);
                        }
                    },
                }
            }
        }
    }

    async fn build_tx(
        http_client: &HttpClient,
        data: SwapData,
        user_pubkey: Pubkey,
        payer: &Keypair,
        rpc_client: Arc<RpcClient>,
        lastest_blockhash: Arc<LatestBlockhash>,
    ) -> Result<Vec<VersionedTransaction>> {
        let mut txs: Vec<VersionedTransaction> = vec![];
        let recent_blockhash = lastest_blockhash.get_blockhash().await;

        let start_time = Instant::now();
        let client_1 = http_client.clone().get_client().await;
        let client_2 = http_client.clone().get_client().await;
        let (swap_response, swap_response_2) = tokio::try_join!(
            Engine::fetch_swap_instructions(
                &client_1,
                data.data1.clone(),
                user_pubkey,
                payer.pubkey()
            ),
            Engine::fetch_swap_instructions(&client_2, data.data2, user_pubkey, payer.pubkey())
        )
        .map_err(|e| {
            error!("fetch_swap_instructions error: {:?}", e);
            anyhow!(e)
        })?;
        let elapsed_time = start_time.elapsed();
        debug!("fetch_swap_instructions elapsed_time: {:.4?}", elapsed_time);
        if elapsed_time > config::get_config().http_request_timeout_to_duration() {
            return Err(anyhow!("fetch swap_instructions HTTP request timeout"));
        }

        debug!(
            "swap_response1 = {:?}\n swap_response_2 = {:?}",
            swap_response, swap_response_2
        );

        // let alts =
        //     load_alt_accounts(&rpc_client, &swap_response.address_lookup_table_addresses).await;
        // let alts_2 =
        //     load_alt_accounts(&rpc_client, &swap_response_2.address_lookup_table_addresses).await;

        let start_time = Instant::now();
        let (alts, alts_2) = tokio::join!(
            load_alt_accounts(&rpc_client, &swap_response.address_lookup_table_addresses),
            load_alt_accounts(&rpc_client, &swap_response_2.address_lookup_table_addresses)
        );
        debug!(
            "load_alt_accounts elapsed_time: {:.4?}",
            start_time.elapsed()
        );

        let mut all_instructions: Vec<Instruction> = Vec::with_capacity(
            swap_response.compute_budget_instructions.len()
                + swap_response.setup_instructions.len()
                + swap_response.other_instructions.len()
                + 2,
        );

        // 小费
        if config::get_config().jito.bundle_submit {
            // TODO 百分比计算小费方式
            let fee_amount = config::get_config().jito.fixed_tip_amount;
            let tip_account = util::get_jito_tip_fee_account().await?;
            debug!("Tips account: {}, amount: {}", tip_account, fee_amount);
            let tip_tx =
                solana_sdk::system_instruction::transfer(&payer.pubkey(), &tip_account, fee_amount);
            all_instructions.push(tip_tx);
        }

        // input tx
        {
            // JITO Tip 与 Priority Fee 只设置一个，否则浪费CU
            if config::get_config().jito.bundle_submit {
                // 排除 priority_fee 指令
                if swap_response.compute_budget_instructions.len() > 0 {
                    // https://github.com/solana-labs/solana/blob/master/sdk/src/compute_budget.rs#L25
                    // ix.data.first() == Some(&0x03)
                    let ixs = util::exclude_set_compute_unit_price_ixs(
                        &swap_response.compute_budget_instructions,
                    );
                    all_instructions.extend(ixs);
                }
            } else {
                all_instructions.extend(
                    swap_response
                        .compute_budget_instructions
                        .into_iter()
                        .map(Instruction::from),
                );
            }

            all_instructions.extend(
                swap_response
                    .setup_instructions
                    .into_iter()
                    .map(Instruction::from),
            );

            all_instructions.push(Instruction::from(swap_response.swap_instruction));
            if let Some(cleanup_instruction) = swap_response.cleanup_instruction {
                all_instructions.push(Instruction::from(cleanup_instruction));
            }
        }

        // 只处理 setup_instructions、swap_instruction 和 cleanup_instruction
        // let swap_response_2 =
        //     match Engine::fetch_swap_instructions(&http_client, data.data2, user_pubkey).await {
        //         Ok(res) => res,
        //         Err(e) => {
        //             error!("fetch_swap_instructions error: {:?}", e);
        //             return Err(e);
        //         }
        //     };

        // 试图合并成一笔交易
        {
            let swap_response_clone = swap_response_2.clone();
            let mut all_instruction_clone = all_instructions;
            all_instruction_clone.extend(
                swap_response_clone
                    .setup_instructions
                    .into_iter()
                    .map(Instruction::from),
            );
            all_instruction_clone.push(Instruction::from(swap_response_clone.swap_instruction));
            if swap_response_clone.cleanup_instruction.is_some() {
                all_instruction_clone.push(Instruction::from(
                    swap_response_clone.cleanup_instruction.unwrap(),
                ));
            }
            // all_instruction_clone.push(Instruction::from(swap_response_clone.cleanup_instruction));
            //
            // 备注指令
            {
                let memo_string = format!("Memo-{}", timestamp());
                let memo = memo_string.as_bytes();
                let memo_instruction = build_memo(memo, &[&payer.pubkey()]);
                all_instruction_clone.push(memo_instruction);
            }

            // 添加 check_profit 利润检查指令
            {
                let current_balance = rpc_client.get_balance(&user_pubkey).await?;
                let min_profit_amount = config::get_config().min_profit_amount;

                let check_profit_ix =
                    Engine::get_check_profit_ix(&payer, current_balance, min_profit_amount).await;
                all_instruction_clone.push(check_profit_ix);
            }

            // println!("ixs = {:?}", all_instruction_clone);

            // TODO
            let mut alts_clone = alts.clone();
            alts_clone.extend(alts_2.clone());
            // 这里合并指令时，只使用了第一笔交易的 ALTS, 并没有使用第二笔交易的ALTS
            let tx_simple = Engine::convert_versioned_transaction(
                &user_pubkey,
                &payer,
                &all_instruction_clone,
                &alts_clone,
                recent_blockhash,
            )
            .await?;

            // 超出单笔交易大小
            let size = bincode::serialize(&tx_simple)?.len();
            if size > constants::TX_SIZE {
                return Err(anyhow!("交易过大，超出 1232 字节"));
            }
            txs.push(tx_simple);
        }

        Ok(txs)
    }

    async fn convert_versioned_transaction(
        user_pubkey: &Pubkey,
        payer: &Keypair,
        instructions: &Vec<Instruction>,
        alt_addresses: &Vec<AddressLookupTableAccount>,
        recent_blockhash: solana_hash::Hash,
    ) -> Result<VersionedTransaction> {
        let message =
            V0Message::try_compile(user_pubkey, instructions, alt_addresses, recent_blockhash)?;

        let tx = VersionedTransaction::try_new(VersionedMessage::V0(message), &[payer])?;
        Ok(tx)
    }

    // #[instrument(skip(self), fields(request_id))]
    pub async fn run(&mut self) -> Result<()> {
        let config = config::get_config();
        let SwapConfig {
            input_mint,
            output_mint,
            input_amount: quote_in_amount,
            slippage_bps,
            ..
        } = config.swap.clone();

        let start = Instant::now();

        let initial_interval = Duration::from_secs(5);
        let max_interval = Duration::from_secs(60);
        let multiplier = 1.5;
        let d = ExponentialBackoff {
            initial_interval,                                 // 初始延迟 5 秒
            max_interval,                                     // 最大延迟 60 秒（避免无限增长）
            multiplier,                                       // 每次延迟时间翻倍
            max_elapsed_time: Some(Duration::from_secs(300)), // 最多重试 5 分钟
            ..ExponentialBackoff::default()
        };

        let retry_count = std::cell::RefCell::new(0);
        let quote1 = retry(d, || async {
            let quote_response = match self
                .get_quote(&input_mint, &output_mint, quote_in_amount, slippage_bps)
                .await
            {
                Ok(res) => {
                    *retry_count.borrow_mut() = 0;
                    res
                }
                Err(e) => {
                    error!("request quote error: {:?}", e);
                    if let Some(reqwest_err) = e.downcast_ref::<reqwest::Error>() {
                        if reqwest_err.is_status() {
                            // too many requests
                            if let Some(status) = reqwest_err.status() {
                                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                                    // 获取下次延迟时间（基于当前退避状态）
                                    let current_retry = *retry_count.borrow();
                                    *retry_count.borrow_mut() += 1;

                                    // 根据重试次数计算下次等待时间
                                    let next_delay = if current_retry == 0 {
                                        initial_interval
                                    } else {
                                        // 使用指数退避策略: initial_interval * (multiplier ^ (retry_count - 1))
                                        let factor = multiplier.powi(current_retry as i32);
                                        let calculated = Duration::from_secs_f64(
                                            initial_interval.as_secs_f64() * factor,
                                        );
                                        // 确保不超过最大间隔
                                        std::cmp::min(calculated, max_interval)
                                    };

                                    error!(
                                        "TooManyRequests, retry count: {}, it will be retry... {:?}",
                                        current_retry, next_delay
                                    );
                                }
                                // let _ = sleep(Duration::from_secs(5)).await;
                            }
                            // 指数退避和重试策略
                            return Err(backoff::Error::transient(anyhow!(
                                "Http StatusCode: {}",
                                reqwest::StatusCode::TOO_MANY_REQUESTS
                            )));
                        } else {
                            debug!("Http reqwest error: {:?}", reqwest_err);
                        }
                    } else {
                        debug!("Http request error: {:?}", e);
                        return Err(backoff::Error::transient(e));
                    }

                    return Err(backoff::Error::transient(anyhow!(
                        "Http request error: {:?}",
                        e
                    )));
                }
            };

            Ok(quote_response)
        })
        .await?;

        // USDT => SOL
        let quote2_in_amount = quote1.out_amount.parse::<u64>()?;
        let quote2 = match self
            .get_quote(&output_mint, &input_mint, quote2_in_amount, slippage_bps)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                error!("request quote error: {:?}", e);

                // too many requests
                if let Some(reqwest_err) = e.downcast_ref::<reqwest::Error>() {
                    if reqwest_err.is_status() {
                        if let Some(status) = reqwest_err.status() {
                            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                                info!("Too many reuqest ...");
                            }
                        }
                    } else {
                        debug!("Other reqwest error: {:?}", reqwest_err);
                    }
                } else {
                    debug!("Other error: {:?}", e);
                }

                return Ok(());
            }
        };

        // 利润计算
        let quote2_out_amount = quote2.out_amount.parse::<u64>()?;

        let elapsed = start.elapsed();
        debug!("🕦 获取两次报价quote, 共计耗时: {:.4?}", elapsed);

        // HTTP请求时间太长，本次检测直接视为无效
        if util::latency_too_high(elapsed) {
            debug!(
                "👁️ Request latency too long ({} ms > {:?}), Ignore!",
                elapsed.as_millis(),
                config.max_latency_ms_to_duration()
            );
            return Ok(());
        }

        let diff = quote2_out_amount as i64 - quote_in_amount as i64;
        let snipe = diff > 0 && diff as u64 > config.min_profit_threshold_amount;
        debug!(
            " {quote2_out_amount} - {quote_in_amount} = {diff}, 存在利润(>{} Lamports) ：{snipe}",
            config.min_profit_threshold_amount
        );

        if snipe {
            println!(
                "🔥 发现利润：{quote2_out_amount} - {quote_in_amount} = {diff} Lamports ({} SOL) 🔥",
                diff as f64 / 10f64.powi(9)
            );

            if config.jito.bundle_submit {
                let tip_amount = util::calculation_jito_tip_amount(diff);
                println!(
                    "💵 启用 Jito Bundle Submit，本次支付小费 {}，可获得利润 {}",
                    tip_amount,
                    diff - tip_amount
                );

                // 净利润不足
                if diff <= tip_amount {
                    println!("净利润过低（{}）, 放弃...", diff - tip_amount);
                    return Ok(());
                }
                // util::sol_to_usd((diff - tip_amount) as f64 / 10f64.powi(9));
            } else {
                let prioritization_fee = config.prioritization_fee_lamports as i64;
                // ::PRIORITIZATION_FEE_LAMPORTS
                if diff <= prioritization_fee {
                    println!("净利润过低（{}）, 放弃...", diff - prioritization_fee);
                    return Ok(());
                }
                // util::sol_to_usd((diff - prioritization_fee) as f64 / 10f64.powi(9));
            }

            // 获取swap指令
            if let Err(e) = self
                .swap_channel_tx
                .send(SwapData {
                    data1: quote1,
                    data2: quote2,
                })
                .await
            {
                eprintln!("发送 SwapData 到 channel 失败：{}", e);
            }
        }

        //
        Ok(())
    }

    async fn get_quote(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: u64,
        slippage_bps: u64,
    ) -> Result<QuoteResponse> {
        // SOL => USDT
        // ?inputMint={input}&outputMint={output}&amount={amount}&slippageBps=50
        let url = format!("{}/quote", config::get_config().jup_v6_api_base_url);
        let quote_request = QuoteReuqest {
            input_mint: input_mint.to_string(),
            output_mint: output_mint.to_string(),
            amount,
            slippage_bps,
            dexes: config::get_config().swap.dexes.clone(),
            exclude_dexes: config::get_config().swap.exclude_dexes.clone(),
            ..Default::default()
        };

        let start = Instant::now();
        let resp = self
            .http_client
            .get_client()
            .await
            .get(&url)
            .query(&quote_request)
            .timeout(config::get_config().http_request_timeout_to_duration())
            .send()
            .await?
            .error_for_status()?;
        let quote = resp.json::<QuoteResponse>().await?;

        debug!(
            "URL: {}, Duration: {:.4?} contextSlot: {}",
            url,
            start.elapsed(),
            quote.context_slot
        );
        Ok(quote)
    }

    async fn fetch_swap_instructions(
        http_client: &reqwest::Client,
        quote: QuoteResponse,
        user_pubkey: Pubkey,
        payer: Pubkey,
    ) -> Result<SwapResponse> {
        let config = config::get_config();

        let mut swap_request = SwapRequest {
            quote_response: quote,
            user_public_key: user_pubkey.to_string(),
            payer: payer.to_string(),
            wrap_and_unwrap_sol: Some(config.swap.wrap_and_unwrap_sol),
            fee_account: None,
            as_legacy_transaction: None,
            ..Default::default()
        };

        // let jsona = serde_json::to_string_pretty(&swap_request).unwrap();
        // println!("{}", jsona);

        // 普通交易
        if !(config.jito.bundle_submit) {
            let prioritization_fee = PrioritizationFeeLamports {
                priority_level_with_max_lamports: Some(PriorityLevelWithMaxLamports {
                    priority_level: Some("high".to_string()),
                    max_lamports: config.prioritization_fee_lamports,
                }),
                jito_tip_lamports: None,
            };
            swap_request.prioritization_fee_lamports = Some(prioritization_fee);
        }

        debug!("swap_request = {:?}", swap_request);

        let start = Instant::now();

        // let url = "https://lite-api.jup.ag/swap/v1/swap-instructions";
        let url = format!("{}/swap-instructions", config.jup_v6_api_base_url);
        let resp = http_client
            .post(&url)
            .json(&swap_request)
            .timeout(config.http_request_timeout_to_duration())
            .send()
            .await?
            .error_for_status()? // 如果 HTTP 非 200，会报错
            .json::<SwapResponse>()
            .await?;

        debug!("URL: {}, Duration: {:.4?}", url, start.elapsed());
        Ok(resp)
    }

    async fn get_check_profit_ix(
        payer: &Keypair,
        current_balance: u64,
        min_profit: u64,
    ) -> Instruction {
        // instruction_data = (min_profit:u64, before_amount: u64)
        // accounts = {
        //     payer: payer.pubkey(),
        //     fee_recipient_pubkey: constants::FEE_RECIPIENT_PUBKEY,
        //     system_program: system_program::id(),
        // }
        //
        // let min_profit = config::get_config().min_profit_amount;
        // let current_balance = self.rpc_client.get_balance(&payer.pubkey()).await.unwrap();

        // 手续费
        let fee_recipient_pubkey = constants::FEE_RECIPIENT_PUBKEY;

        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&min_profit.to_le_bytes());
        instruction_data.extend_from_slice(&current_balance.to_le_bytes());

        Instruction {
            program_id: constants::CHECK_PROFIT_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true), // Payer
                AccountMeta::new(fee_recipient_pubkey, false),
                AccountMeta::new_readonly(system_program::ID, false), // Check Profit Account
            ],
            data: instruction_data,
        }
    }
}

async fn send_transaction_with_options(
    rpc_client: &RpcClient,
    tx: &VersionedTransaction,
    skip_preflight: bool,
) -> Result<Signature> {
    if skip_preflight {
        // 跳过预检查的模式
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: None,
            encoding: None,
            max_retries: None,
            min_context_slot: None,
        };

        let signature = rpc_client.send_transaction_with_config(tx, config).await?;

        // 可以选择是否等待确认
        let _confirmation = rpc_client
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::confirmed())
            .await?;

        Ok(signature)
    } else {
        // 使用默认的带预检查的方法
        rpc_client
            .send_and_confirm_transaction(tx)
            .await
            .map_err(|e| anyhow!("{}", e))
    }
}

/// 构建一个Memo指令
pub fn build_memo(memo: &[u8], signer_pubkeys: &[&Pubkey]) -> Instruction {
    Instruction {
        program_id: constants::MEMO_PROGRAM_ID,
        accounts: signer_pubkeys
            .iter()
            .map(|&pubkey| AccountMeta::new_readonly(*pubkey, true))
            .collect(),
        data: memo.to_vec(),
    }
}

async fn check_bundle_id_status(jito_sdk: &JitoJsonRpcSDK, bundle_uuid: &str) -> Result<()> {
    // Confirm bundle status
    let max_retries = 30;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        debug!(
            "[{}] Checking bundle status (attempt {}/{})",
            bundle_uuid, attempt, max_retries
        );

        let status_response = jito_sdk
            .get_in_flight_bundle_statuses(vec![bundle_uuid.to_string()])
            .await?;

        if let Some(result) = status_response.get("result") {
            if let Some(value) = result.get("value") {
                if let Some(statuses) = value.as_array() {
                    if let Some(bundle_status) = statuses.get(0) {
                        if let Some(status) = bundle_status.get("status") {
                            match status.as_str() {
                                Some("Landed") => {
                                    debug!("Bundle landed on-chain. Checking final status...");
                                    return check_final_bundle_status(&jito_sdk, bundle_uuid).await;
                                }
                                Some("Pending") => {
                                    debug!("Bundle is pending. Waiting...");
                                }
                                Some(status) => {
                                    debug!("Unexpected bundle status: {}. Waiting...", status);
                                }
                                None => {
                                    debug!("Unable to parse bundle status. Waiting...");
                                }
                            }
                        } else {
                            debug!("Status field not found in bundle status. Waiting...");
                        }
                    } else {
                        debug!("Bundle status not found. Waiting...");
                    }
                } else {
                    debug!("Unexpected value format. Waiting...");
                }
            } else {
                debug!("Value field not found in result. Waiting...");
            }
        } else if let Some(error) = status_response.get("error") {
            debug!("Error checking bundle status: {:?}", error);
        } else {
            debug!("Unexpected response format. Waiting...");
        }

        if attempt < max_retries {
            sleep(retry_delay).await;
        }
    }

    Err(anyhow!(
        "Failed to confirm bundle status after {} attempts",
        max_retries
    ))
}

async fn check_final_bundle_status(jito_sdk: &JitoJsonRpcSDK, bundle_uuid: &str) -> Result<()> {
    let max_retries = 30;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        debug!(
            "Checking final bundle status (attempt {}/{})",
            attempt, max_retries
        );

        let status_response = jito_sdk
            .get_bundle_statuses(vec![bundle_uuid.to_string()])
            .await?;
        let bundle_status = get_bundle_status(&status_response)?;

        match bundle_status.confirmation_status.as_deref() {
            Some("confirmed") => {
                debug!(
                    "✅ [{}] Bundle confirmed on-chain. Waiting for finalization...",
                    bundle_uuid
                );
                check_transaction_error(&bundle_status)?;
            }
            Some("finalized") => {
                debug!(
                    "✅ [{}] Bundle finalized on-chain successfully!",
                    bundle_uuid
                );
                check_transaction_error(&bundle_status)?;
                print_transaction_url(&bundle_status);
                return Ok(());
            }
            Some(status) => {
                debug!(
                    "Unexpected final bundle status: {}. Continuing to poll...",
                    status
                );
            }
            None => {
                debug!("Unable to parse final bundle status. Continuing to poll...");
            }
        }

        if attempt < max_retries {
            sleep(retry_delay).await;
        }
    }

    Err(anyhow!(
        "Failed to get finalized status after {} attempts",
        max_retries
    ))
}

#[derive(Debug)]
struct BundleStatus {
    confirmation_status: Option<String>,
    err: Option<serde_json::Value>,
    transactions: Option<Vec<String>>,
}

fn get_bundle_status(status_response: &serde_json::Value) -> Result<BundleStatus> {
    status_response
        .get("result")
        .and_then(|result| result.get("value"))
        .and_then(|value| value.as_array())
        .and_then(|statuses| statuses.get(0))
        .ok_or_else(|| anyhow!("Failed to parse bundle status"))
        .map(|bundle_status| BundleStatus {
            confirmation_status: bundle_status
                .get("confirmation_status")
                .and_then(|s| s.as_str())
                .map(String::from),
            err: bundle_status.get("err").cloned(),
            transactions: bundle_status
                .get("transactions")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                }),
        })
}

fn check_transaction_error(bundle_status: &BundleStatus) -> Result<()> {
    if let Some(err) = &bundle_status.err {
        if err["Ok"].is_null() {
            debug!("Transaction executed without errors.");
            Ok(())
        } else {
            debug!("Transaction encountered an error: {:?}", err);
            Err(anyhow!("Transaction encountered an error"))
        }
    } else {
        Ok(())
    }
}

fn print_transaction_url(bundle_status: &BundleStatus) {
    if let Some(transactions) = &bundle_status.transactions {
        if let Some(tx_id) = transactions.first() {
            println!("Transaction URL: https://solscan.io/tx/{}", tx_id);
        } else {
            println!("Unable to extract transaction ID.");
        }
    } else {
        println!("No transactions found in the bundle status.");
    }
}

use anyhow::Error;
use regex::Regex;

fn extract_program_error(e: &Error) -> Option<(usize, u64)> {
    let err_str = e.to_string();

    // 匹配格式："Instruction 7: Custom program error: 0x1788"
    let re = Regex::new(r"Instruction (\d+): Custom program error: 0x([0-9a-fA-F]+)").unwrap();

    if let Some(caps) = re.captures(&err_str) {
        let instruction_index = caps.get(1)?.as_str().parse::<usize>().ok()?;
        let error_code_hex = caps.get(2)?.as_str();
        let error_code = u64::from_str_radix(error_code_hex, 16).ok()?;
        return Some((instruction_index, error_code));
    }

    None
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[tokio::test]
//     async fn test_get_quote() {
//         let engine = Engine::new();

//         // SOL =》 USDT
//         let input_mint = "So11111111111111111111111111111111111111112";
//         let out_mint = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
//         let in_amount = 10_000_000;
//         let slippage_bps = 100;

//         let result = engine
//             .get_quote(input_mint, out_mint, in_amount, slippage_bps)
//             .await;
//         println!("{:#?}", result);
//         assert!(result.is_ok(), "Should return a quote route");

//         let quote = result.unwrap();
//         println!(
//             "Quote received: in={} out={}",
//             quote.in_amount, quote.out_amount
//         );
//         assert!(!quote.in_amount.is_empty());
//         assert!(!quote.out_amount.is_empty());
//     }

//     // #[tokio::test]
//     // async fn test_swap_instruction() {
//     //     let engine = Engine::new();

//     //     // SOL =》 USDT
//     //     let input_mint = "So11111111111111111111111111111111111111112";
//     //     let out_mint = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
//     //     let in_amount = 10_000_000;
//     //     let slippage_bps = 100;

//     //     let result = engine
//     //         .get_quote(input_mint, out_mint, in_amount, slippage_bps)
//     //         .await;
//     //     assert!(result.is_ok(), "Should return a quote route");

//     //     // quoteResponse
//     //     let quote_response = result.unwrap();
//     //     // println!("{:?}", quote_response.clone());

//     //     let user_pubkey = Pubkey::new_unique();
//     //     let swap_result =
//     //         Engine::fetch_swap_instructions(&engine.http_client, quote_response, user_pubkey).await;
//     //     println!("{:#?}", swap_result);
//     //     assert!(swap_result.is_ok(), "Should return swapResponse");
//     //     let swap_response = swap_result.unwrap();

//     //     assert!(
//     //         swap_response.setup_instructions.len() > 0,
//     //         "SwapInstruction Length Must Greater than 0"
//     //     )
//     // }
// }
