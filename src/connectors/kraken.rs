use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize}; // Added Serialize for subscribe_msg
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct KrakenTradeEntry(
    String, // price
    String, // volume
    String, // timestamp
    String, // buy/sell
    String, // market/limit
    String, // misc
);

// Helper function to map canonical pair (e.g., BTC/USDC) to Kraken V1 symbol (e.g., XBT/USDC)
fn to_kraken_symbol(pair: &str) -> String {
    let parts: Vec<&str> = pair.split('/').collect();
    if parts.len() != 2 {
        return pair.to_string(); // Return original if format is wrong
    }

    let base = parts[0];
    let quote = parts[1];

    // 1. Map Base Currency: BTC is usually XBT on V1
    let kraken_base = match base {
        "BTC" => "XBT".to_string(),
        // SOL is sometimes XSOL, but often just SOL
        _ => base.to_string(),
    };

    // 2. Map Quote Currency: Fiat/Stablecoin conversion is messy on Kraken V1
    // The safest approach is usually XBT/USD -> XBTZUSD. For cryptos/stablecoins,
    // it's often simpler (SOL/USDC -> SOLUSDC), but we'll try the common ones.
    let kraken_quote = match quote {
        // ZUSD for USD on older feeds, but simple USDT/USDC sometimes works for pairs like SOL/USDC
        "USD" => "USD".to_string(), // SOL/USD or BTC/USD are common
        "USDT" => "USDT".to_string(),
        "USDC" => "USDC".to_string(),
        _ => quote.to_string(),
    };

    // Combine them, Kraken V1 API uses no separator (XBTUSDC) or sometimes a prefixed quote (XBTZUSD)
    // We'll use the canonical "BASE/QUOTE" format in the subscription message's 'pair' field,
    // as Kraken often accepts common formats if listed on the exchange.
    format!("{}/{}", kraken_base, kraken_quote)
}

pub async fn run_kraken_connector(tx: Sender<PriceUpdate>, pair: String) {
    // Convert canonical pair (e.g., BTC/USDC) to the format Kraken expects (e.g., XBT/USDC)
    let kraken_subscription_symbol = to_kraken_symbol(&pair);

    let url = "wss://ws.kraken.com";
    println!("[Kraken] connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("[Kraken] ✅ Connected");

            // Subscribe to SOL/USD trades
            let subscribe_msg = serde_json::json!({
                "event": "subscribe",
                "pair": [kraken_subscription_symbol.clone()],
                "subscription": { "name": "trade" }
            });

            ws_stream
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .unwrap();

            println!(
                "[Kraken] 📡 Subscribed to {} trades",
                kraken_subscription_symbol
            );

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();

                        // Skip non-trade events
                        if text.contains("heartbeat") || text.contains("\"event\"") {
                            continue;
                        }

                        // Kraken sends trade arrays
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(text) {
                            if let Some(arr) = json_val.as_array() {
                                if arr.len() >= 4 && arr[1].is_array() {
                                    if let Ok(trades) =
                                        serde_json::from_value::<Vec<KrakenTradeEntry>>(
                                            arr[1].clone(),
                                        )
                                    {
                                        for trade in trades {
                                            if let Ok(price) = trade.0.parse::<f64>() {
                                                let _ = tx.send(PriceUpdate {
                                                    source: "Kraken".to_string(),
                                                    pair: pair.clone(),
                                                    price,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("[Kraken] ❌ Connection error: {:?}", e),
    }
}
