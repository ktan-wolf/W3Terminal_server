use super::state::PriceUpdate;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

pub async fn run_kucoin_connector(tx: Sender<PriceUpdate>) {
    let symbol = "SOL-USDT";

    loop {
        println!("HTX: connecting KuCoin...");

        // 1) Fetch bullet token & endpoint
        let bullet_resp = match reqwest::Client::new()
            .post("https://api.kucoin.com/api/v1/bullet-public")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("❌ KuCoin bullet request failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let bullet: BulletResponse = match bullet_resp.json().await {
            Ok(json) => json,
            Err(e) => {
                eprintln!("❌ KuCoin bullet JSON parse failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let server = &bullet.data.instanceServers[0];
        let ws_url = format!("{}?token={}", server.endpoint, bullet.data.token);

        // 2) Connect to WebSocket
        let (ws_stream, _) = match connect_async(&ws_url).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ KuCoin WS connect failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        println!("⚡ KuCoin WS connected: {}", server.endpoint);

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // 3) Subscribe to ticker
        let sub = serde_json::json!({
            "id": 1,
            "type": "subscribe",
            "topic": format!("/market/ticker:{}", symbol),
            "privateChannel": false,
            "response": true
        });

        {
            let mut w = write.lock().await;
            let _ = w.send(Message::Text(sub.to_string().into())).await;
        }

        println!("📡 Subscribed to KuCoin ticker {}", symbol);

        // 4) Spawn ping task
        let ping_write = Arc::clone(&write);
        let ping_interval = Duration::from_millis(server.pingInterval);
        tokio::spawn(async move {
            loop {
                sleep(ping_interval).await;
                let mut w = ping_write.lock().await;
                if w.send(Message::Ping(Bytes::new())).await.is_err() {
                    break; // connection closed
                }
            }
        });

        // 5) Read loop
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
                                        pair: symbol.to_string(),
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

        eprintln!("⚠️ KuCoin connector disconnected. Reconnecting...");
        sleep(Duration::from_secs(5)).await;
    }
}

