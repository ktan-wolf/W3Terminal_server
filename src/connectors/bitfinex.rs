use crate::state::PriceUpdate;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH}; // Added for timestamp generation
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// Helper function to map canonical pair (e.g., BTC/USDC) to Bitfinex symbol (e.g., tBTCUSD)
fn to_bitfinex_symbol(pair: &str) -> String {
    let parts: Vec<&str> = pair.split('/').collect();
    if parts.len() != 2 {
        return format!("t{}", pair.replace("/", ""));
    }

    let base = parts[0];
    let quote = parts[1];

    // Convert to Bitfinex's tSYMBOL format
    let symbol = match (base, quote) {
        (b, "USD") => format!("t{b}USD"),
        (b, "USDT") => format!("t{b}:USDT"),
        (b, "USDC") => format!("t{b}:USDC"),
        (b, q) => format!("t{b}{q}"),
    };

    symbol.to_uppercase()
}

pub async fn run_bitfinex_connector(tx: Sender<PriceUpdate>, pair: String) {
    let url = "wss://api-pub.bitfinex.com/ws/2";

    // 1. Derive Bitfinex symbol from the canonical pair
    let bitfinex_symbol = to_bitfinex_symbol(&pair);
    let canonical_pair = pair.clone();

    println!(
        "Bitfinex connecting to pair: {} (Symbol: {})",
        canonical_pair, bitfinex_symbol
    );

    let (mut ws, _) = match connect_async(url).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Bitfinex Connection error for {}: {:?}", canonical_pair, e);
            return;
        }
    };

    println!("Bitfinex Connected to {}", canonical_pair);

    // 2. Subscribe using the dynamic Bitfinex symbol
    let sub = json!({
        "event": "subscribe",
        "channel": "ticker",
        "symbol": bitfinex_symbol
    });

    if let Err(e) = ws.send(Message::Text(sub.to_string().into())).await {
        eprintln!(
            "Bitfinex Subscription failed for {}: {:?}",
            canonical_pair, e
        );
        return;
    }

    println!("Bitfinex Subscribed to TICKER {}", canonical_pair);

    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    // Skip event messages
                    if parsed.get("event").is_some() {
                        continue;
                    }

                    if let Some(arr) = parsed.as_array() {
                        // Ignore heartbeat ["hb"]
                        if arr.len() == 2 && arr[1] == "hb" {
                            continue;
                        }

                        if arr.len() >= 2 {
                            if let Some(data) = arr[1].as_array() {
                                // LAST_PRICE is at index 6
                                if data.len() > 6 {
                                    if let Some(price) = data[6].as_f64() {
                                        // Generate current timestamp (Bitfinex ticker doesn't provide one)
                                        let timestamp = SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                            as u64;

                                        let _ = tx.send(PriceUpdate {
                                            source: "Bitfinex".to_string(),
                                            pair: canonical_pair.clone(),
                                            price,
                                            timestamp, // Added timestamp
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p)).await;
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                eprintln!("Bitfinex WS error for {}: {:?}", canonical_pair, e);
                break;
            }
            _ => {}
        }
    }

    eprintln!("Bitfinex Connector for {} stopped.", canonical_pair);
}
