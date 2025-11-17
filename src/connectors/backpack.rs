use crate::connectors::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;
use url::Url;

const BACKPACK_WS_URL: &str = "wss://ws.backpack.exchange";
const MARKET_SYMBOL: &str = "SOL_USDC"; // change to whatever market you want

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

pub async fn run_backpack_connector(tx: Sender<PriceUpdate>) -> anyhow::Result<()> {
    //let ws_url = Url::parse(BACKPACK_WS_URL)?;
    //println!("[Backpack] Connecting to WebSocket at {}", BACKPACK_WS_URL);

    let (ws_stream, _) = tokio_tungstenite::connect_async("wss://ws.backpack.exchange").await?;
    let (mut write, mut read) = ws_stream.split();

    // Subscribe to trade stream for the symbol
    let subscribe_msg = json!({
        "method": "SUBSCRIBE",
        "params": [ format!("trade.{}", MARKET_SYMBOL) ]
    });

    write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .await?;

    println!("[Backpack] Subscribed to trade.{}", MARKET_SYMBOL);

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
            // Parse the JSON
            let v: serde_json::Value = serde_json::from_str(&txt)?;
            if v.get("stream").is_some() {
                // This is a message from a stream
                let data = &v["data"];
                // Deserialize into our TradeMessage
                let trade: TradeMessage = serde_json::from_value(data.clone())?;
                // Parse price
                let price: f64 = trade.price.parse().unwrap_or(0.0);

                // Send price update
                let update = PriceUpdate {
                    source: "Backpack".into(),
                    pair: MARKET_SYMBOL.into(),
                    price,
                };
                let _ = tx.send(update);
            }
        }
    }

    Ok(())
}

