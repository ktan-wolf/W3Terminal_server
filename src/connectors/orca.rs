use crate::connectors::state::PriceUpdate;
use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::convert::TryInto;
use std::str::FromStr;
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};

/// Orca SOL/USDC Whirlpool Address
const ORCA_WHIRLPOOL_ADDRESS: &str = "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE";

const SOL_DECIMALS: i32 = 9;
const USDC_DECIMALS: i32 = 6;
const SQRT_PRICE_OFFSET: usize = 65;

pub async fn run_orca_connector(tx: Sender<PriceUpdate>) -> Result<()> {
    let rpc_url = "https://api.mainnet-beta.solana.com";
    println!("[ORCA] Connecting to Solana RPC: {rpc_url}");

    let rpc_client = RpcClient::new(rpc_url.to_string());
    let pool_pubkey = Pubkey::from_str(ORCA_WHIRLPOOL_ADDRESS)?;

    loop {
        match rpc_client.get_account_data(&pool_pubkey).await {
            Ok(data_bytes) => {
                if data_bytes.len() < SQRT_PRICE_OFFSET + 16 {
                    println!("[ORCA] Account data too short");
                    sleep(Duration::from_millis(500)).await;
                    continue;
                }

                let sqrt_price_bytes: &[u8] =
                    &data_bytes[SQRT_PRICE_OFFSET..SQRT_PRICE_OFFSET + 16];
                let sqrt_price_u128 = u128::from_le_bytes(sqrt_price_bytes.try_into()?);

                let price_x64 = sqrt_price_u128 as f64;
                let two_pow_64 = (1u128 << 64) as f64;
                let price_native = (price_x64 / two_pow_64).powi(2);
                let decimal_adjustment = 10f64.powi(SOL_DECIMALS - USDC_DECIMALS);
                let final_price = price_native * decimal_adjustment;

                let update = PriceUpdate {
                    source: "Orca".into(),
                    pair: "SOL/USDC".into(),
                    price: final_price,
                };

                let _ = tx.send(update);
            }
            Err(err) => {
                println!("[ORCA] Error fetching account data: {:?}", err);
            }
        }

        // Poll every 500ms
        sleep(Duration::from_millis(500)).await;
    }
}
