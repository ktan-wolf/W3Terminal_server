use super::state::PriceUpdate;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast::Sender};
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct BitgetEnvelope {
    event: Option<String>,
    arg: Option<BitgetArg>,
    data: Option<Vec<BitgetTicker>>,
}

#[derive(Debug, Deserialize)]
struct BitgetArg {
    instType: String,
    channel: String,
    instId: String,
}

#[derive(Debug, Deserialize)]
struct BitgetTicker {
    lastPr: String,
}

/// Run the Bitget WebSocket connector for a given pair
pub async fn run_bitget_connector(tx: Sender<PriceUpdate>, pair: String) {
    // Bitget uses "SOLUSDT" format, so remove "/" and uppercase
    let symbol = pair.replace("/", "").to_uppercase();
    let canonical_pair = pair.clone(); // e.g., "SOL/USDT"

    loop {
        println!("🔌 Connecting to Bitget for {}", symbol);
        let ws_url = "wss://ws.bitget.com/v2/ws/public";

        // 1️⃣ Connect to WebSocket
        let (ws_stream, _) = match connect_async(ws_url).await {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("❌ Bitget WS connection failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        println!("⚡ Bitget WS connected");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // 2️⃣ Subscribe to ticker
        let sub_msg = serde_json::json!({
            "op": "subscribe",
            "args": [{
                "instType": "SPOT",
                "channel": "ticker",
                "instId": symbol
            }]
        });

        {
            let mut w = write.lock().await;
            if let Err(e) = w.send(Message::Text(sub_msg.to_string().into())).await {
                eprintln!("❌ Failed to send subscription: {:?}", e);
            }
        }
        println!("📡 Subscribed to Bitget ticker {}", canonical_pair);

        // 3️⃣ Ping task (keep connection alive)
        let ping_write = Arc::clone(&write);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(15)).await;
                let mut w = ping_write.lock().await;
                if w.send(Message::Ping(Bytes::new())).await.is_err() {
                    break; // WS closed
                }
            }
        });

        // 4️⃣ Read loop
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<BitgetEnvelope>(&text) {
                        if let Some(ticks) = parsed.data {
                            for tick in ticks {
                                if let Ok(price) = tick.lastPr.parse::<f64>() {
                                    let _ = tx.send(PriceUpdate {
                                        source: "Bitget".to_string(),
                                        pair: canonical_pair.clone(),
                                        price,
                                    });
                                }
                            }
                        }
                    }
                }

                Ok(Message::Ping(payload)) => {
                    let mut w = write.lock().await;
                    let _ = w.send(Message::Pong(payload)).await;
                }

                Ok(Message::Close(_)) => break,

                Err(e) => {
                    eprintln!("❌ Bitget WS error: {:?}", e);
                    break;
                }

                _ => {}
            }
        }

        eprintln!(
            "⚠️ Bitget connector for {} disconnected. Reconnecting in 5s...",
            canonical_pair
        );
        sleep(Duration::from_secs(5)).await;
    }
}

