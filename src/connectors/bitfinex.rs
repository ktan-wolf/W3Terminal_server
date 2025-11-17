use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_bitfinex_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://api-pub.bitfinex.com/ws/2";
    println!("[Bitfinex] connecting: {}", url);

    let (mut ws, _) = match connect_async(url).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[Bitfinex] ❌ Connection error: {:?}", e);
            return;
        }
    };

    println!("[Bitfinex] ✅ Connected");

    // Subscribe to SOL/USD ticker
    let sub = serde_json::json!({
        "event": "subscribe",
        "channel": "ticker",
        "symbol": "tSOLUSD"
    });

    if let Err(e) = ws.send(Message::Text(sub.to_string().into())).await {
        eprintln!("[Bitfinex] ❌ Subscription failed: {:?}", e);
        return;
    }

    println!("[Bitfinex] 📡 Subscribed to TICKER SOLUSD");

    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    // Skip event messages
                    if parsed.get("event").is_some() {
                        continue;
                    }

                    // Should be an array
                    if let Some(arr) = parsed.as_array() {
                        // Ignore heartbeat ["hb"]
                        if arr.len() == 2 && arr[1] == "hb" {
                            continue;
                        }

                        // The second element contains ticker data
                        if arr.len() >= 2 {
                            if let Some(data) = arr[1].as_array() {
                                // LAST_PRICE is at index 6
                                if data.len() > 6 {
                                    if let Some(price) = data[6].as_f64() {
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
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p)).await;
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                eprintln!("[Bitfinex] ❌ WS error: {:?}", e);
                break;
            }
            _ => {}
        }
    }

    eprintln!("[Bitfinex] ⚠️ Connector stopped. Reconnecting...");
}
