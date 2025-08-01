use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::{sync::RwLock, time::Duration};

const INTERVAL: Duration = Duration::from_secs(2);
pub struct LatestBlockhash {
    blockhash: RwLock<solana_sdk::hash::Hash>,
    slot: AtomicU64,
}

impl LatestBlockhash {
    pub async fn start(rpc_client: Arc<RpcClient>) -> Arc<Self> {
        let latest_blockhash = Arc::new(LatestBlockhash {
            blockhash: RwLock::new(solana_sdk::hash::Hash::default()),
            slot: AtomicU64::new(0),
        });

        let latest_blockhash_clone = Arc::clone(&latest_blockhash);
        tokio::spawn(async move {
            loop {
                if let Ok((blockhash, slot)) = rpc_client
                    .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
                    .await
                {
                    let mut blockhash_write = latest_blockhash_clone.blockhash.write().await;
                    *blockhash_write = blockhash;
                    latest_blockhash_clone.slot.store(slot, Ordering::Relaxed);
                }
                tokio::time::sleep(INTERVAL).await;
            }
        });

        latest_blockhash
    }

    pub async fn get_blockhash(&self) -> solana_sdk::hash::Hash {
        self.blockhash.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_cached_blockhash_concurrent() {
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let rpc_client = Arc::new(rpc);

        let latest_blockhash = LatestBlockhash::start(rpc_client.clone()).await;

        // 等待一下让它拉到 blockhash
        tokio::time::sleep(Duration::from_secs(4)).await;

        let hash = latest_blockhash.get_blockhash().await;

        println!("Cached blockhash: {:?}", hash);

        assert_ne!(hash, solana_sdk::hash::Hash::default());
        let hash2 = latest_blockhash.get_blockhash().await;
        assert_eq!(hash, hash2);
    }
}
