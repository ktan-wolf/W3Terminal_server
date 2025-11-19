use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;

use crate::connectors::state::PriceUpdate;

#[derive(Debug, Deserialize)]
struct BybitTickerData {
    #[serde(rename = "lastPrice")]
    last_price: String,
}

#[derive(Debug, Deserialize)]
struct BybitMessage {
    topic: Option<String>,
    data: Option<BybitTickerData>,
}

pub async fn run_bybit_connector(tx: Sender<PriceUpdate>, pair: String) {
    let symbol = pair.to_uppercase().replace("/", "");
    let url = "wss://stream.bybit.com/v5/public/spot";

    println!("Connecting to Bybit WebSocket…");

    let (ws_stream, _) = match connect_async(url).await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("Bybit WS Connect Error: {:?}", err);
            return;
        }
    };

    println!("Connected to Bybit!");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to SOLUSDT ticker
    let subscribe_msg = serde_json::json!({
        "op": "subscribe",
        "args": [format!("tickers.{}", symbol)] // Dynamically insert the symbol
    });

    let _ = write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .await;

    println!("Subscribed to Bybit {} ticker.", symbol);

    // Read incoming messages
    while let Some(msg) = read.next().await {
        if let Ok(text) = msg.and_then(|m| m.into_text()) {
            if let Ok(parsed) = serde_json::from_str::<BybitMessage>(&text) {
                if let (Some(_topic), Some(data)) = (parsed.topic, parsed.data) {
                    if let Ok(price) = data.last_price.parse::<f64>() {
                        let _ = tx.send(PriceUpdate {
                            source: "Bybit".to_string(),
                            pair: pair.clone(),
                            price,
                        });
                    }
                }
            }
        }
    }

    println!("Bybit WebSocket closed.");
}
