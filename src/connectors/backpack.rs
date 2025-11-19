use crate::connectors::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;

#[derive(Debug, Deserialize)]
struct TradeMessage {
    #[serde(rename = "e")]
    event: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "t")]
    trade_id: u64,
    #[serde(rename = "T")]
    ts: u64,
}

pub async fn run_backpack_connector(tx: Sender<PriceUpdate>, pair: String) {
    const BACKPACK_WS_URL: &str = "wss://ws.backpack.exchange";
    let symbol = pair.replace("/", "_").to_uppercase();

    println!("[Backpack] Connecting: {}", BACKPACK_WS_URL);

    let (ws_stream, _) = match connect_async(BACKPACK_WS_URL).await {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("[Backpack] ❌ Connection error: {:?}", e);
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to trade stream
    let subscribe_msg = json!({
        "method": "SUBSCRIBE",
        "params": [ format!("trade.{}", symbol) ]
    });

    if let Err(e) = write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .await
    {
        eprintln!("[Backpack] ❌ Failed to subscribe: {:?}", e);
        return;
    }

    println!("[Backpack] Subscribed to trade.{}", symbol);

    while let Some(msg) = read.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[Backpack] ❌ Error reading message: {:?}", e);
                break;
            }
        };

        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
            let v: serde_json::Value = match serde_json::from_str(&txt) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if v.get("stream").is_some() {
                let data = &v["data"];

                let trade: TradeMessage = match serde_json::from_value(data.clone()) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                let price: f64 = trade.price.parse().unwrap_or(0.0);

                let update = PriceUpdate {
                    source: "Backpack".into(),
                    pair: pair.clone(),
                    price,
                };

                let _ = tx.send(update);
            }
        }
    }

    println!("[Backpack] ❌ Disconnected");
}

