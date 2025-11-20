use std::str::FromStr;

use crate::connectors::state::PriceUpdate;
use anyhow::{Result, anyhow};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::time::{SystemTime, UNIX_EPOCH}; // Added for timestamp generation
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};

const USDC_DECIMALS: u32 = 6;
const SOL_DECIMALS: u32 = 9;
const ETH_DECIMALS: u32 = 7; // Assuming 8 decimals for ETH as per previous context
const BTC_DECIMALS: u32 = 8; // Assuming 8 decimals for BTC as per previous context

struct VaultConfig {
    token_a_vault: &'static str,
    token_b_vault: &'static str, // Always USDC vault in this context
    token_a_decimals: u32,
    token_b_decimals: u32, // Always USDC_DECIMALS
}

fn get_vault_config(pair: &str) -> Result<VaultConfig> {
    match pair {
        "SOL/USDC" => Ok(VaultConfig {
            token_a_vault: "DQyrAcCrDXQ7NeoqGgDCZwBvWDcYmFCjSb9JtteuvPpz", // SOL_VAULT
            token_b_vault: "HLmqeL62xR1QoZ1HKKbXRrdN1p3phKpxRMb2VVopvBBz", // USDC_VAULT (for SOL)
            token_a_decimals: SOL_DECIMALS,
            token_b_decimals: USDC_DECIMALS,
        }),
        "BTC/USDC" => Ok(VaultConfig {
            token_a_vault: "HWTaEDR6BpWjmyeUyfGZjeppLnH7s8o225Saar7FYDt5", // BTC vault
            token_b_vault: "7iGcnvoLAxthsXY3AFSgkTDoqnLiuti5fyPNm2VwZ3Wz", // USDC vault (for BTC)
            token_a_decimals: BTC_DECIMALS,
            token_b_decimals: USDC_DECIMALS,
        }),
        "ETH/USDC" => Ok(VaultConfig {
            token_a_vault: "EHT99uYfAnVxWHPLUMJRTyhD4AyQZDDknKMEssHDtor5", // ETH vault
            token_b_vault: "58tgdkogRoMsrXZJubnFPsFmNp5mpByEmE1fF6FTNvDL", // USDC vault (for ETH)
            token_a_decimals: ETH_DECIMALS,
            token_b_decimals: USDC_DECIMALS,
        }),
        _ => Err(anyhow!("Unsupported pair for Raydium connector: {}", pair)),
    }
}

async fn fetch_raydium_price(rpc: &RpcClient, config: &VaultConfig) -> Result<f64> {
    // Note: Token A is the base (e.g., SOL, BTC, ETH), Token B is the quote (USDC).
    let token_a_vault = Pubkey::from_str(config.token_a_vault)?;
    let token_b_vault = Pubkey::from_str(config.token_b_vault)?;

    let token_a_acc = rpc.get_token_account_balance(&token_a_vault).await?;
    let token_b_acc = rpc.get_token_account_balance(&token_b_vault).await?;

    let token_a_raw = token_a_acc.amount.parse::<f64>()?;
    let token_b_raw = token_b_acc.amount.parse::<f64>()?;

    // Apply decimal correction
    let token_a = token_a_raw / 10f64.powi(config.token_a_decimals as i32);
    let token_b = token_b_raw / 10f64.powi(config.token_b_decimals as i32); // USDC decimals

    if token_a == 0.0 {
        anyhow::bail!("Base token reserve ({}) is zero", token_a_vault.to_string());
    }

    // Price = Quote Reserve / Base Reserve (USDC / SOL or USDC / BTC)
    let price = token_b / token_a;
    Ok(price)
}

pub async fn run_raydium_connector(tx: Sender<PriceUpdate>, pair: String) {
    let canonical_pair = pair.clone();

    let config = match get_vault_config(&canonical_pair) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("RAYDIUM Configuration Error: {:?}", e);
            return;
        }
    };

    println!(
        "RAYDIUM Starting Raydium price feed for {}...",
        canonical_pair
    );

    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".into());

    loop {
        // Pass the dynamic configuration to the fetch function
        match fetch_raydium_price(&rpc, &config).await {
            Ok(price) => {
                // Generate timestamp for RPC poll
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let update = PriceUpdate {
                    source: "Raydium".into(),
                    pair: canonical_pair.clone(), // Use the original canonical pair
                    price,
                    timestamp, // Added timestamp field
                };
                let _ = tx.send(update);
            }
            Err(err) => {
                println!("RAYDIUM Error fetching {}: {:?}", canonical_pair, err);
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}
