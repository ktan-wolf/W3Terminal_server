use super::state::PriceUpdate;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast::Sender};
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct BulletResponse {
    code: String,
    data: BulletData,
}

#[derive(Debug, Deserialize)]
struct BulletData {
    token: String,
    instanceServers: Vec<InstanceServer>,
}

#[derive(Debug, Deserialize)]
struct InstanceServer {
    endpoint: String,
    pingInterval: u64,
}

#[derive(Debug, Deserialize)]
struct KucoinMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    data: Option<TickerData>,
}

#[derive(Debug, Deserialize)]
struct TickerData {
    price: String,
}

/// Run the KuCoin WebSocket connector
/// - `tx` : broadcast sender for PriceUpdate
/// - `pair`: trading pair, e.g., "BTC/USDT"
pub async fn run_kucoin_connector(tx: Sender<PriceUpdate>, pair: String) {
    // Convert to KuCoin symbol format: "BTC/USDT" -> "BTC-USDT"
    let kucoin_symbol = pair.replace("/", "-").to_uppercase();
    let canonical_pair = pair.clone(); // keep original for broadcast

    loop {
        println!("[KuCoin] Attempting connection for {}", canonical_pair);

        // 1️⃣ Fetch Bullet token & server endpoint
        let bullet_resp = match reqwest::Client::new()
            .post("https://api.kucoin.com/api/v1/bullet-public")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("❌ KuCoin bullet request failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let bullet: BulletResponse = match bullet_resp.json().await {
            Ok(json) => json,
            Err(e) => {
                eprintln!("❌ Failed to parse Bullet JSON: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let server = &bullet.data.instanceServers[0];
        let ws_url = format!("{}?token={}", server.endpoint, bullet.data.token);

        // 2️⃣ Connect to WebSocket
        let (ws_stream, _) = match connect_async(&ws_url).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ KuCoin WS connection failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        println!(
            "⚡ KuCoin WS connected for {} at {}",
            canonical_pair, server.endpoint
        );

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // 3️⃣ Subscribe to ticker
        let sub_msg = serde_json::json!({
            "id": 1,
            "type": "subscribe",
            "topic": format!("/market/ticker:{}", kucoin_symbol),
            "privateChannel": false,
            "response": true
        });

        {
            let mut w = write.lock().await;
            if w.send(Message::Text(sub_msg.to_string().into()))
                .await
                .is_err()
            {
                eprintln!("❌ Failed to send subscription for {}", canonical_pair);
                continue;
            }
        }
        println!("📡 Subscribed to KuCoin ticker {}", canonical_pair);

        // 4️⃣ Ping task to keep WS alive
        let ping_write = Arc::clone(&write);
        let ping_interval = Duration::from_millis(server.pingInterval);
        tokio::spawn(async move {
            loop {
                sleep(ping_interval).await;
                let mut w = ping_write.lock().await;
                if w.send(Message::Ping(Bytes::new())).await.is_err() {
                    break; // WS closed
                }
            }
        });

        // 5️⃣ Read loop
        let read_tx = tx.clone();
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<KucoinMessage>(&text) {
                        if parsed.msg_type == "message" {
                            if let Some(tick) = parsed.data {
                                if let Ok(price) = tick.price.parse::<f64>() {
                                    let _ = read_tx.send(PriceUpdate {
                                        source: "KuCoin".to_string(),
                                        pair: canonical_pair.clone(),
                                        price,
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(Message::Ping(_)) => {
                    let mut w = write.lock().await;
                    let _ = w.send(Message::Pong(Bytes::new())).await;
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    eprintln!("❌ KuCoin WS error: {:?}", e);
                    break;
                }
                _ => {}
            }
        }

        eprintln!(
            "⚠️ KuCoin connector for {} disconnected. Reconnecting...",
            canonical_pair
        );
        sleep(Duration::from_secs(5)).await;
    }
}
