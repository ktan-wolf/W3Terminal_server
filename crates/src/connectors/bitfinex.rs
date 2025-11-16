use super::state::PriceUpdate;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_bitfinex_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://api-pub.bitfinex.com/ws/2";
    println!("[Bitfinex] connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws, _)) => {
            println!("[Bitfinex] ✅ Connected");

            let sub = serde_json::json!({
                "event": "subscribe",
                "channel": "trades",
                "symbol": "tSOLUSD"
            });

            ws.send(Message::Text(sub.to_string().into()))
                .await
                .unwrap();

            println!("[Bitfinex] 📡 Subscribed to SOLUSD");

            while let Some(msg) = ws.next().await {
                if let Ok(msg) = msg {
                    if !msg.is_text() {
                        continue;
                    }

                    let text = msg.to_text().unwrap();
                    let parsed: Value = match serde_json::from_str(text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if parsed.is_array() {
                        let arr = parsed.as_array().unwrap();
                        // arr = [ channelId, [ "tu", PRICE, ... ] ]
                        if arr.len() >= 2 && arr[1].is_array() {
                            let inner = arr[1].as_array().unwrap();
                            if inner.len() >= 3 {
                                let price = inner[2].as_f64().unwrap_or(0.0);

                                let _ = tx.send(PriceUpdate {
                                    source: "Bitfinex".to_string(),
                                    pair: "SOL/USD".to_string(),
                                    price,
                                });
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("[Bitfinex] ❌ Connection error: {:?}", e),
    }
}
