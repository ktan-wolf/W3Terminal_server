use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
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
    lastPr: String, // IMPORTANT: Bitget uses lastPr, not last
}

pub async fn run_bitget_connector(tx: Sender<PriceUpdate>) {
    let symbol = "SOLUSDT";

    println!("🔌 Starting Bitget connector for {}", symbol);

    let ws_url = "wss://ws.bitget.com/v2/ws/public";

    let (ws_stream, _) = match connect_async(ws_url).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Bitget WS connect failed: {:?}", e);
            return;
        }
    };

    println!("⚡ Bitget WebSocket connected");

    let (mut write, mut read) = ws_stream.split();

    // Correct subscription format
    let sub = serde_json::json!({
        "op": "subscribe",
        "args": [{
            "instType": "SPOT",
            "channel": "ticker",
            "instId": symbol
        }]
    });

    let _ = write.send(Message::Text(sub.to_string().into())).await;

    println!("📡 Subscribed to Bitget ticker {}", symbol);

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<BitgetEnvelope>(&text) {
                    if let Some(ticks) = parsed.data {
                        for tick in ticks {
                            if let Ok(price) = tick.lastPr.parse::<f64>() {
                                let _ = tx.send(PriceUpdate {
                                    source: "Bitget".to_string(),
                                    pair: "SOL/USDT".to_string(),
                                    price,
                                });
                            }
                        }
                    }
                }
            }

            Ok(Message::Ping(payload)) => {
                let _ = write.send(Message::Pong(payload)).await;
            }

            Err(e) => {
                eprintln!("❌ Bitget WS error: {:?}", e);
                break;
            }

            _ => {}
        }
    }

    eprintln!("⚠️ Bitget connector stopped.");
}
