use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct OkxTrade {
    px: String, // price
}

#[derive(Debug, Deserialize)]
struct OkxData {
    data: Vec<OkxTrade>,
}

#[derive(Debug, Deserialize)]
struct OkxMsg {
    arg: serde_json::Value,
    data: Option<Vec<OkxTrade>>,
}

pub async fn run_okx_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    println!("[OKX] connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("[OKX] ✅ Connected");

            let subscribe_msg = serde_json::json!({
                "op": "subscribe",
                "args": [{
                    "channel": "trades",
                    "instId": "SOL-USDT"
                }]
            });

            ws_stream
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .unwrap();

            println!("[OKX] 📡 Subscribed to SOL-USDT trades");

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if !msg.is_text() {
                        continue;
                    }

                    if let Ok(parsed) = serde_json::from_str::<OkxMsg>(msg.to_text().unwrap()) {
                        if let Some(data) = parsed.data {
                            for t in data {
                                if let Ok(price) = t.px.parse::<f64>() {
                                    let _ = tx.send(PriceUpdate {
                                        source: "OKX".to_string(),
                                        pair: "SOL/USDT".to_string(),
                                        price,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("[OKX] ❌ Connection error: {:?}", e),
    }
}
