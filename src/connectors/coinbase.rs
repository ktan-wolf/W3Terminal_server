use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Deserialize)]
struct CoinbaseMatch {
    #[serde(rename = "type")]
    msg_type: String,

    price: Option<String>,
    size: Option<String>,
    time: Option<String>,
}

pub async fn run_coinbase_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://ws-feed.exchange.coinbase.com";
    println!("[Coinbase] connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("[Coinbase] ✅ Connected");

            // Subscribe to SOL-USD trade matches
            let subscribe_msg = serde_json::json!({
                "type": "subscribe",
                "product_ids": ["SOL-USD"],
                "channels": ["matches"]
            });

            ws_stream
                .send(Message::text(subscribe_msg.to_string()))
                .await
                .unwrap();

            println!("[Coinbase] 📡 Subscribed to SOL-USD trades");

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        if let Ok(parsed) =
                            serde_json::from_str::<CoinbaseMatch>(msg.to_text().unwrap())
                        {
                            if parsed.msg_type == "match" {
                                if let Some(price_str) = parsed.price {
                                    if let Ok(price) = price_str.parse::<f64>() {
                                        let _ = tx.send(PriceUpdate {
                                            source: "Coinbase".to_string(),
                                            pair: "SOL/USD".to_string(),
                                            price,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[Coinbase] ❌ Connection error: {:?}", e);
        }
    }
}
