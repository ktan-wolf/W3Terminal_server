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
    if let Err(e) = async {
        // Original connector body goes here
        let backpack_symbol = pair.replace("/", "_").to_uppercase();
        let canonical_pair = pair.clone();

        let ws_url = "wss://ws.backpack.exchange";
        println!("[Backpack] Connecting to WebSocket at {}", ws_url);

        let (ws_stream, _) = connect_async(ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let subscribe_msg = serde_json::json!({
            "method": "SUBSCRIBE",
            "params": [ format!("trade.{}", backpack_symbol) ]
        });

        write
            .send(tokio_tungstenite::tungstenite::Message::Text(
                subscribe_msg.to_string().into(),
            ))
            .await?;

        println!("[Backpack] Subscribed to trade.{}", backpack_symbol);

        while let Some(msg) = read.next().await {
            let msg = msg?;
            if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
                let v: serde_json::Value = serde_json::from_str(&txt)?;
                if let Some(data) = v.get("data") {
                    if let Ok(trade) = serde_json::from_value::<TradeMessage>(data.clone()) {
                        if let Ok(price) = trade.price.parse::<f64>() {
                            let _ = tx.send(PriceUpdate {
                                source: "Backpack".into(),
                                pair: canonical_pair.clone(),
                                price,
                            });
                        }
                    }
                }
            }
        }

        Ok::<(), anyhow::Error>(())
    }
    .await
    {
        eprintln!("❌ Backpack connector error: {:?}", e);
    }
}

