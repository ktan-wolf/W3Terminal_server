use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

pub async fn run_kraken_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://ws.kraken.com";
    println!("[Kraken] connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("[Kraken] ✅ Connected");

            // Subscribe to SOL/USD trades
            let subscribe_msg = serde_json::json!({
                "event": "subscribe",
                "pair": ["SOL/USD"],
                "subscription": { "name": "trade" }
            });

            ws_stream
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .unwrap();

            println!("[Kraken] 📡 Subscribed to SOL/USD trades");

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
                                                    pair: "SOL/USD".to_string(),
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
