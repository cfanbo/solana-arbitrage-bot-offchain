use anyhow::{Error, Result};
use rand::Rng;
use rand::prelude::IndexedRandom;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum HttpClientError {
    #[error("Failed to bind IP {0}: {1}")]
    BindFailed(Ipv4Addr, Error),
}

#[derive(Debug, Clone, Copy)]
pub enum IpSelectAlgorithm {
    RoundRobin,
    Random,
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    clients: Arc<Vec<Client>>,
    algorithm: IpSelectAlgorithm,
    round_robin_index: Arc<Mutex<usize>>,
    last_random_ip: Arc<Mutex<Option<usize>>>,
}

impl HttpClient {
    /// 初始化：空IP列表时创建默认Client，否则预绑定所有IP
    pub fn initialize(
        ips: Vec<Ipv4Addr>,
        algorithm: IpSelectAlgorithm,
    ) -> Result<Self, HttpClientError> {
        let clients = if ips.is_empty() {
            // 情况1：用户未指定IP，使用默认Client
            vec![Client::new()]
        } else {
            // 情况2：预创建所有绑定IP的Client
            ips.iter()
                .map(|&ip| {
                    Client::builder()
                        .local_address(Some(IpAddr::V4(ip)))
                        .build()
                        .map_err(|e| HttpClientError::BindFailed(ip, e.into()))
                })
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(Self {
            clients: Arc::new(clients),
            algorithm,
            round_robin_index: Arc::new(Mutex::new(0)),
            last_random_ip: Arc::new(Mutex::new(None)),
        })
    }

    /// 获取客户端：单IP直接返回，多IP走算法
    pub async fn get_client(&self) -> Client {
        match self.clients.len() {
            0 => unreachable!(),             // 初始化保证clients非空
            1 => self.clients[0].clone(),    // 单IP快速路径
            _ => self.select_client().await, // 多IP算法选择
        }
    }

    /// 多IP选择算法
    async fn select_client(&self) -> Client {
        let index = match self.algorithm {
            IpSelectAlgorithm::RoundRobin => {
                let mut idx = self.round_robin_index.lock().await;
                let selected = *idx;
                *idx = (*idx + 1) % self.clients.len();
                selected
            }
            IpSelectAlgorithm::Random => {
                let mut last_idx = self.last_random_ip.lock().await;
                let candidates: Vec<usize> = (0..self.clients.len())
                    .filter(|&i| Some(i) != *last_idx)
                    .collect();

                let selected = if candidates.is_empty() {
                    rand::rng().random_range(0..self.clients.len())
                } else {
                    *candidates.choose(&mut rand::rng()).unwrap()
                };

                *last_idx = Some(selected);
                selected
            }
        };

        self.clients[index].clone()
    }
}
