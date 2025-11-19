use crate::connectors::state::PriceUpdate;
use anyhow::{Result, anyhow}; // Used anyhow! macro for dynamic error
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::convert::TryInto;
use std::str::FromStr;
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};

// --- Constant Definitions (Standardized) ---
const SOL_DECIMALS: i32 = 9;
const USDC_DECIMALS: i32 = 6;
const SQRT_PRICE_OFFSET: usize = 65;

// --- Mapping Structure ---
struct PairConfig {
    whirlpool_address: &'static str,
    base_decimals: i32,
}

/// Helper function to map the canonical pair to its Orca configuration.
fn get_orca_config(pair: &str) -> Option<PairConfig> {
    match pair {
        "SOL/USDC" => Some(PairConfig {
            whirlpool_address: "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE",
            base_decimals: SOL_DECIMALS, // 9
        }),
        "ETH/USDC" => Some(PairConfig {
            whirlpool_address: "AU971DrPyhhrpRnmEBp5pDTWL2ny7nofb5vYBjDJkR2E",
            base_decimals: 8, // Given 8 decimals for ETH
        }),
        "BTC/USDC" => Some(PairConfig {
            whirlpool_address: "55BrDTCLWayM16GwrMEQU57o4PTm6ceF9wavSdNZcEiy",
            base_decimals: 8, // Given 8 decimals for BTC
        }),
        _ => None,
    }
}

// UPDATED SIGNATURE: Accept the `pair` string

pub async fn run_orca_connector(tx: Sender<PriceUpdate>, pair: String) {
    if let Err(e) = async {
        let config = get_orca_config(&pair)
            .ok_or_else(|| anyhow::anyhow!("Unsupported pair: {} for Orca connector", pair))?;

        let rpc_url = "https://api.mainnet-beta.solana.com";
        println!("ORCA Connecting to Solana RPC for {}: {rpc_url}", pair);

        let rpc_client =
            solana_client::nonblocking::rpc_client::RpcClient::new(rpc_url.to_string());

        let pool_pubkey = solana_sdk::pubkey::Pubkey::from_str(config.whirlpool_address)?;

        let canonical_pair = pair.clone();

        loop {
            match rpc_client.get_account_data(&pool_pubkey).await {
                Ok(data_bytes) => {
                    if data_bytes.len() < SQRT_PRICE_OFFSET + 16 {
                        println!("ORCA Account data too short for {}", canonical_pair);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        continue;
                    }

                    let sqrt_price_bytes: &[u8] =
                        &data_bytes[SQRT_PRICE_OFFSET..SQRT_PRICE_OFFSET + 16];
                    let sqrt_price_u128 = u128::from_le_bytes(sqrt_price_bytes.try_into()?);

                    let price_x64 = sqrt_price_u128 as f64;
                    let two_pow_64 = (1u128 << 64) as f64;

                    let price_native = (price_x64 / two_pow_64).powi(2);

                    let decimal_adjustment = 10f64.powi(config.base_decimals - USDC_DECIMALS);
                    let final_price = price_native * decimal_adjustment;

                    let update = PriceUpdate {
                        source: "Orca".into(),
                        pair: canonical_pair.clone(),
                        price: final_price,
                    };

                    let _ = tx.send(update);
                }
                Err(err) => {
                    println!(
                        "ORCA Error fetching account data for {}: {:?}",
                        canonical_pair, err
                    );
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        Ok::<(), anyhow::Error>(())
    }
    .await
    {
        eprintln!("Orca connector error for {}: {:?}", pair, e);
    }
}
