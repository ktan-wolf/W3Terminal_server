use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct KrakenTradeEntry(
    String,                     // price
    #[allow(dead_code)] String, // volume
    String,                     // timestamp (seconds.decimals)
    #[allow(dead_code)] String, // buy/sell
    #[allow(dead_code)] String, // market/limit
    #[allow(dead_code)] String, // misc
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
    let kraken_quote = match quote {
        "USD" => "USD".to_string(), // SOL/USD or BTC/USD are common
        "USDT" => "USDT".to_string(),
        "USDC" => "USDC".to_string(),
        _ => quote.to_string(),
    };

    // Combine them
    format!("{}/{}", kraken_base, kraken_quote)
}

pub async fn run_kraken_connector(tx: Sender<PriceUpdate>, pair: String) {
    let kraken_subscription_symbol = to_kraken_symbol(&pair);

    let url = "wss://ws.kraken.com";
    println!("Kraken connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("Kraken Connected");

            // Subscribe to trades
            let subscribe_msg = serde_json::json!({
                "event": "subscribe",
                "pair": [kraken_subscription_symbol.clone()],
                "subscription": { "name": "trade" }
            });

            ws_stream
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .unwrap();

            println!("Kraken Subscribed to {} trades", kraken_subscription_symbol);

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();

                        // Skip non-trade events
                        if text.contains("heartbeat") || text.contains("\"event\"") {
                            continue;
                        }

                        // Kraken sends trade arrays: [channelID, [[price, vol, time, ...], ...], channelName, pair]
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(text) {
                            if let Some(arr) = json_val.as_array() {
                                // Ensure we have the data array at index 1
                                if arr.len() >= 2 && arr[1].is_array() {
                                    if let Ok(trades) =
                                        serde_json::from_value::<Vec<KrakenTradeEntry>>(
                                            arr[1].clone(),
                                        )
                                    {
                                        for trade in trades {
                                            if let Ok(price) = trade.0.parse::<f64>() {
                                                // Parse timestamp: Kraken sends seconds as string "1616661666.1234"
                                                let ts_seconds =
                                                    trade.2.parse::<f64>().unwrap_or(0.0);
                                                let timestamp = (ts_seconds * 1000.0) as u64; // Convert to ms

                                                let _ = tx.send(PriceUpdate {
                                                    source: "Kraken".to_string(),
                                                    pair: pair.clone(),
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
            }
        }
        Err(e) => eprintln!("Kraken Connection error: {:?}", e),
    }
}
