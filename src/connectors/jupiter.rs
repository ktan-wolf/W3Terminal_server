use crate::connectors::state::PriceUpdate;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};

// Jupiter Price API V3 response format
#[derive(Debug, Deserialize)]
pub struct JupiterPrice {
    pub usdPrice: f64,
    pub blockId: u64,
    pub decimals: u8,
    pub priceChange24h: f64,
}

// Map<mint → JupiterPrice>
pub type JupiterResponse = HashMap<String, JupiterPrice>;

/// Mint address for SOL token
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Raydium price connector using Jupiter Price V3 API
pub async fn run_dex_connector(tx: Sender<PriceUpdate>) {
    println!("[DEX] Starting Jupiter (API V3) feed...");

    let client = Client::new();

    loop {
        let url = format!("https://lite-api.jup.ag/price/v3?ids={}", SOL_MINT);

        let response = client.get(&url).send().await;

        match response {
            Ok(resp) => {
                let json = resp.json::<JupiterResponse>().await;
                match json {
                    Ok(map) => {
                        if let Some(price_data) = map.get(SOL_MINT) {
                            let update = PriceUpdate {
                                source: "Jupiter".into(),
                                pair: "SOL/USDC".into(),
                                price: price_data.usdPrice,
                            };

                            // broadcast the update
                            let _ = tx.send(update);
                        }
                    }

                    Err(e) => {
                        println!("[DEX] JSON parse error: {:?}", e);
                    }
                }
            }

            Err(e) => {
                println!("[DEX] Request error: {:?}", e);
            }
        }

        sleep(Duration::from_secs(1)).await;
    }
}
