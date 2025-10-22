mod engine;
use anyhow::{Result, anyhow};
use arbitrage_bot::*;
use clap::{Parser, Subcommand};
use self_update::Status as UpdateStatus;
use std::str::FromStr;
use std::sync::Arc;
use tokio::{
    signal,
    sync::Notify,
    time::{Duration, sleep},
};
use tracing::{Level, error};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// CLI 工具入口
#[derive(Parser)]
#[command(
    name = "arbitrage-bot",
    version=option_env!("VERGEN_GIT_DESCRIBE").unwrap_or("unknown"),
    about = "一款基于 Jupiter Aggregator 实现的套利工具"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 打印版本信息
    Version,

    /// 检查并更新到最新版本
    Update,

    /// 运行套利主程序
    Run,

    /// 初始化配置文件
    Init,
}

#[tokio::main]
async fn main() -> Result<()> {
    // tokio-console
    // #[cfg(debug_assertions)]
    // {
    //     console_subscriber::init();
    // }

    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Run) {
        Commands::Version => {
            println!("当前版本: {}", env!("VERGEN_GIT_DESCRIBE"));
        }

        Commands::Update => {
            let current_version = env!("VERGEN_GIT_DESCRIBE");
            let status = tokio::task::spawn_blocking(move || {
                self_update::backends::github::Update::configure()
                    .repo_owner("cfanbo")
                    .repo_name("solana-arbitrage-bot-offchain")
                    .bin_name("arbitrage-bot")
                    .show_download_progress(true)
                    .current_version(&current_version.trim_start_matches('v'))
                    .build()
                    .and_then(|u| u.update())
            })
            .await??;

            match status {
                UpdateStatus::UpToDate(version) => {
                    println!("✅ 已是最新版本: v{}", version);
                }
                UpdateStatus::Updated(version) => {
                    println!("✅ 成功更新到版本: v{}", version);
                }
            }
        }

        Commands::Run => {
            run().await?;
        }

        Commands::Init => {
            init_config()?;
        }
    }
    Ok(())
}

async fn run() -> Result<()> {
    println!(
        "🚀 启动主程序[{}]...",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let mut bot = engine::Engine::new().await;

    let config = config::get_config();

    let env_filter = EnvFilter::new(format!("{}={}", env!("CARGO_PKG_NAME"), config.log_level));
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_max_level(Level::from_str(&config.log_level).unwrap_or(Level::INFO))
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| anyhow!("Failed to set global default subscriber: {}", e))?;

    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = shutdown.clone();

    // 开一个任务监听 Ctrl+C
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("failed to listen for event");
        println!("\n🛑 Ctrl+C captured, notifying shutdown...");
        shutdown_clone.notify_waiters();
    });

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                println!("🔌 收到停止服务信号，正在退出...");
                break;
            }
            _ = async {
                if let Err(e) = bot.run().await {
                    error!("bot.run() = {:?}\n", e);
                }

                sleep(Duration::from_millis(config.frequency)).await;
            } => {}
        }
    }
    println!("✅ 服务已退出");
    Ok(())
}

fn init_config() -> Result<()> {
    const CONFIG_TEMPLATE: &str = include_str!("../config.example.toml");
    let config_path = "config.toml";

    if std::path::Path::new(config_path).exists() {
        println!("⚠️  配置文件 {} 已存在，是否覆盖？(y/N)", config_path);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            println!("❌ 取消初始化");
            return Ok(());
        }
    }

    std::fs::write(config_path, CONFIG_TEMPLATE)?;
    println!("✅ 配置文件 {} 已生成，请根据需要修改配置参数", config_path);
    Ok(())
}
