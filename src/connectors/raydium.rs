use std::str::FromStr;

use crate::connectors::state::PriceUpdate;
use anyhow::Result;
use solana_account_decoder::UiAccountEncoding;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};

/// Pool state account (NOT LP mint)
const RAYDIUM_POOL_ID: &str = "58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2";

/// Raydium SOL vault
const SOL_VAULT: &str = "DQyrAcCrDXQ7NeoqGgDCZwBvWDcYmFCjSb9JtteuvPpz";

/// Raydium USDC vault
const USDC_VAULT: &str = "HLmqeL62xR1QoZ1HKKbXRrdN1p3phKpxRMb2VVopvBBz";

/// Reads Raydium vault balances and computes SOL/USDC price

async fn fetch_raydium_price(rpc: &RpcClient) -> Result<f64> {
    let sol_vault = Pubkey::from_str(SOL_VAULT)?;
    let usdc_vault = Pubkey::from_str(USDC_VAULT)?;

    let sol_acc = rpc.get_token_account_balance(&sol_vault).await?;
    let usdc_acc = rpc.get_token_account_balance(&usdc_vault).await?;

    let sol_raw = sol_acc.amount.parse::<f64>()?;
    let usdc_raw = usdc_acc.amount.parse::<f64>()?;

    let sol = sol_raw / 1e9; // SOL decimals
    let usdc = usdc_raw / 1e6; // USDC decimals

    if sol == 0.0 {
        anyhow::bail!("SOL reserve zero");
    }

    let price = usdc / sol;
    Ok(price)
}

/// Runs the Raydium price feed loop
pub async fn run_raydium_connector(tx: Sender<PriceUpdate>) {
    println!("[RAYDIUM] Starting Raydium price feed...");

    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".into());

    loop {
        match fetch_raydium_price(&rpc).await {
            Ok(price) => {
                let update = PriceUpdate {
                    source: "Raydium".into(),
                    pair: "SOL/USDC".into(),
                    price,
                };
                let _ = tx.send(update);
            }
            Err(err) => {
                println!("[RAYDIUM] Error: {:?}", err);
            }
        }

        //sleep(Duration::from_secs(2)).await;
    }
}

