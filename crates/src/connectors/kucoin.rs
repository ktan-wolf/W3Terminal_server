use super::state::PriceUpdate;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
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

    println!("🔌 Starting KuCoin connector for {}", symbol);

    // 1) Fetch working endpoint/token
    let resp = match reqwest::Client::new()
        .post("https://api.kucoin.com/api/v1/bullet-public")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("❌ KuCoin bullet error: {:?}", e);
            return;
        }
    };

    let bullet: BulletResponse = match resp.json().await {
        Ok(json) => json,
        Err(e) => {
            eprintln!("❌ KuCoin bullet JSON error: {:?}", e);
            return;
        }
    };

    let server = &bullet.data.instanceServers[0];
    let ws_url = format!("{}?token={}", server.endpoint, bullet.data.token);

    // 2) Connect to correct WebSocket server
    let (ws_stream, _) = match connect_async(&ws_url).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ KuCoin WS connect failed: {:?}", e);
            return;
        }
    };

    println!("⚡ KuCoin WebSocket connected to {}", server.endpoint);

    let (mut write, mut read) = ws_stream.split();

    // 3) Subscribe to ticker
    let sub = serde_json::json!({
        "id": 1,
        "type": "subscribe",
        "topic": format!("/market/ticker:{}", symbol),
        "privateChannel": false,
        "response": true
    });

    let _ = write.send(Message::Text(sub.to_string().into())).await;

    println!("📡 Subscribed to KuCoin ticker {}", symbol);

    // 4) Listen to messages
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<KucoinMessage>(&text) {
                    if parsed.msg_type == "message" {
                        if let Some(tick) = parsed.data {
                            if let Ok(price) = tick.price.parse::<f64>() {
                                let _ = tx.send(PriceUpdate {
                                    source: "Kucoin".to_string(),
                                    pair: symbol.to_string(),
                                    price,
                                });
                            }
                        }
                    }
                }
            }

            Ok(Message::Ping(_)) => {
                // Required per KuCoin WS protocol
                let _ = write.send(Message::Pong(Bytes::new())).await;
            }

            Err(e) => {
                eprintln!("❌ KuCoin WS error: {:?}", e);
                break;
            }

            _ => {}
        }
    }

    eprintln!("⚠️ KuCoin connector stopped.");
}
