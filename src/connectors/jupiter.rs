use crate::connectors::state::PriceUpdate;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH}; // Added for timestamp generation
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

/// Mint address constants (using a small, fixed set as requested)
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const ETH_MINT: &str = "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs";
const BTC_MINT: &str = "3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh";

/// Helper to map the token symbol (e.g., "BTC") to its Solana Mint address
fn get_mint_from_pair(pair: &str) -> Option<&'static str> {
    let base_token = pair.split('/').next()?;

    match base_token {
        "SOL" => Some(SOL_MINT),
        "ETH" => Some(ETH_MINT),
        "BTC" => Some(BTC_MINT),
        _ => None,
    }
}

// UPDATED SIGNATURE: Accept the `pair` string
pub async fn run_jupiter_connector(tx: Sender<PriceUpdate>, pair: String) {
    let canonical_pair = pair.clone(); // Store original pair for output

    // 1. Map the pair to the required mint address
    let mint_address = match get_mint_from_pair(&canonical_pair) {
        Some(mint) => mint,
        None => {
            eprintln!(
                "Jupiter Error Unsupported base token in pair {}. Only SOL, ETH, BTC supported.",
                canonical_pair
            );
            return;
        }
    };

    println!(
        "JUPITER Starting Jupiter (API V3) feed for {}. Mint: {}",
        canonical_pair, mint_address
    );

    let client = Client::new();

    loop {
        // 2. Use the mint address in the dynamic URL
        let url = format!("https://lite-api.jup.ag/price/v3?ids={}", mint_address);

        // Use the mint address itself as the key for parsing the response map
        let expected_key = mint_address;

        let response = client.get(&url).send().await;

        match response {
            Ok(resp) => {
                let json = resp.json::<JupiterResponse>().await;
                match json {
                    Ok(map) => {
                        if let Some(price_data) = map.get(expected_key) {
                            // Generate timestamp since this is a polled HTTP endpoint
                            let timestamp = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;

                            let update = PriceUpdate {
                                source: "Jupiter".into(),
                                // 3. Use the original canonical pair for output
                                pair: canonical_pair.clone(),
                                price: price_data.usdPrice,
                                timestamp, // Added timestamp field
                            };

                            // broadcast the update
                            let _ = tx.send(update);
                        } else {
                            println!(
                                "Jupiter Warning: Price data not found for mint {}.",
                                expected_key
                            );
                        }
                    }

                    Err(e) => {
                        println!("Jupiter JSON parse error for {}: {:?}", canonical_pair, e);
                    }
                }
            }

            Err(e) => {
                println!("Jupiter Request error for {}: {:?}", canonical_pair, e);
            }
        }

        // Delay is 1 second, Jupiter API rate limit is usually fine with this.
        sleep(Duration::from_millis(500)).await;
    }
}

