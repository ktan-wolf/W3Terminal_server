use crate::connectors::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use solana_sdk::pubkey::Pubkey;
use std::convert::TryInto;
use std::str::FromStr;
use tokio::sync::broadcast::Sender;

/// Orca SOL/USDC Whirlpool Address
const ORCA_WHIRLPOOL_ADDRESS: &str = "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE";

const SOL_DECIMALS: i32 = 9;
const USDC_DECIMALS: i32 = 6;
const SQRT_PRICE_OFFSET: usize = 65;

pub async fn run_orca_connector(tx: Sender<PriceUpdate>) -> anyhow::Result<()> {
    let ws_url = "wss://api.mainnet-beta.solana.com/";
    println!("[ORCA] Connecting to Solana WebSocket: {ws_url}");

    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut write, mut read) = ws_stream.split();

    let pool_pubkey = Pubkey::from_str(ORCA_WHIRLPOOL_ADDRESS)?;

    // Subscribe to account changes
    let subscribe_msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "accountSubscribe",
        "params": [
            pool_pubkey.to_string(),
            {
                "encoding": "base64"
            }
        ]
    });

    write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .await?;

    println!("[ORCA] Subscribed to Orca SOL/USDC Whirlpool account");

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
            let v: serde_json::Value = serde_json::from_str(&txt)?;

            // Check if this is a notification from the subscription
            if v.get("method") == Some(&serde_json::Value::String("accountNotification".into())) {
                if let Some(encoded) = v["params"]["result"]["value"]["data"][0].as_str() {
                    let data_bytes = base64::decode(encoded)?;
                    if data_bytes.len() < SQRT_PRICE_OFFSET + 16 {
                        println!("[ORCA] Account data too short");
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
            }
        }
    }

    Ok(())
}

