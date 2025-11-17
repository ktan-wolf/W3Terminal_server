use super::state::PriceUpdate;
use bytes::Bytes;
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::io::Read;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast::Sender};
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Deserialize)]
struct HtxEnvelope {
    #[serde(default)]
    ch: String,
    #[serde(default)]
    tick: Option<HtxTick>,
}

#[derive(Debug, Deserialize)]
struct HtxTick {
    data: Vec<HtxTrade>,
}

#[derive(Debug, Deserialize)]
struct HtxTrade {
    price: f64,
}

pub async fn run_htx_connector(tx: Sender<PriceUpdate>) {
    let symbol = "solusdt";
    let channel = format!("market.{}.trade.detail", symbol);

    loop {
        println!("HTX: connecting...");

        let ws_url = "wss://api-aws.huobi.pro/ws";
        let (ws_stream, _) = match connect_async(ws_url).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ HTX WS connect failed: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        println!("HTX: connected");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // Subscribe
        let sub = serde_json::json!({
            "sub": channel,
            "id": "htx_sub_1"
        });
        if write
            .lock()
            .await
            .send(Message::Text(sub.to_string().into()))
            .await
            .is_err()
        {
            eprintln!("❌ HTX subscribe failed");
            sleep(Duration::from_secs(5)).await;
            continue;
        }
        println!("HTX: subscribed.");

        // Ping task
        let ping_write = Arc::clone(&write);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(5)).await;
                let mut w = ping_write.lock().await;
                if w.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
        });

        // Read messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Binary(bin)) => {
                    let mut d = GzDecoder::new(&bin[..]);
                    let mut decoded = String::new();
                    if d.read_to_string(&mut decoded).is_err() {
                        continue;
                    }

                    if let Ok(parsed) = serde_json::from_str::<HtxEnvelope>(&decoded) {
                        if let Some(tick) = parsed.tick {
                            for trade in tick.data {
                                let _ = tx.send(PriceUpdate {
                                    source: "HTX".to_string(),
                                    pair: "SOL/USDT".to_string(),
                                    price: trade.price,
                                });
                            }
                        }
                    }
                }

                Ok(Message::Ping(p)) => {
                    let mut w = write.lock().await;
                    let _ = w.send(Message::Pong(p)).await;
                }

                Ok(Message::Close(_)) => {
                    eprintln!("HTX: connection closed by server");
                    break;
                }

                Err(e) => {
                    eprintln!("❌ HTX WS error: {:?}", e);
                    break;
                }

                _ => {}
            }
        }

        eprintln!("HTX: disconnected. Reconnecting in 5s...");
        sleep(Duration::from_millis(500)).await;
    }
}
