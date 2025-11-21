use crate::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH}; // Added for timestamp generation
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Deserialize)]
struct CoinbaseMatch {
    #[serde(rename = "type")]
    msg_type: String,

    price: Option<String>,
    _size: Option<String>,
    _time: Option<String>,
}

pub async fn run_coinbase_connector(tx: Sender<PriceUpdate>, pair: String) {
    let url = "wss://ws-feed.exchange.coinbase.com";

    // Store the original requested pair for output (e.g., "BTC/USDC")
    let canonical_pair = pair.clone();

    // 1. DERIVE PRODUCT ID: "BTC/USDC" -> "BTC-USDC"
    let mut coinbase_product_id = pair.replace("/", "-");

    // 2. FALLBACK LOGIC: If requesting USDC, attempt to subscribe to the more common USD product.
    if coinbase_product_id.ends_with("-USDC") {
        let base_currency = coinbase_product_id.trim_end_matches("-USDC");
        coinbase_product_id = format!("{}-USD", base_currency);
        println!(
            "Coinbase Falling back to product ID: {} (Requested: {})",
            coinbase_product_id, canonical_pair
        );
    }

    println!(
        "Coinbase connecting to pair: {} (Product ID: {})",
        canonical_pair, coinbase_product_id
    );

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("Coinbase Connected");

            // Subscribe using the potentially modified product ID
            let subscribe_msg = serde_json::json!({
                "type": "subscribe",
                "product_ids": [coinbase_product_id],
                "channels": ["matches"]
            });

            if ws_stream
                .send(Message::text(subscribe_msg.to_string()))
                .await
                .is_ok()
            {
                println!("Coinbase Subscribed to {} trades", canonical_pair);
            }

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        if let Ok(parsed) =
                            serde_json::from_str::<CoinbaseMatch>(msg.to_text().unwrap())
                        {
                            if parsed.msg_type == "match" {
                                if let Some(price_str) = parsed.price {
                                    if let Ok(price) = price_str.parse::<f64>() {
                                        // Generate timestamp
                                        let timestamp = SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                            as u64;

                                        let _ = tx.send(PriceUpdate {
                                            source: "Coinbase".to_string(),
                                            pair: canonical_pair.clone(),
                                            price,
                                            timestamp, // Added timestamp field
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Coinbase Connection error for {}: {:?}", canonical_pair, e);
        }
    }
}
