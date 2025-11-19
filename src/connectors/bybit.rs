use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message; // Import Message type explicitly

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

// UPDATED SIGNATURE: Accept the `pair` string
pub async fn run_bybit_connector(tx: Sender<PriceUpdate>, pair: String) {
    let url = "wss://stream.bybit.com/v5/public/spot";

    // Store the original requested pair for output
    let canonical_pair = pair.clone();

    // 1. Derive Bybit Symbol: Canonical "BTC/USDT" -> Bybit "BTCUSDT" (Uppercase, no separator)
    let bybit_symbol = pair.to_uppercase().replace("/", "");

    println!(
        "🔗 Connecting to Bybit WebSocket for pair {}...",
        canonical_pair
    );

    let (ws_stream, _) = match connect_async(url).await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!(
                "❌ Bybit WS Connect Error for {}: {:?}",
                canonical_pair, err
            );
            return;
        }
    };

    println!("✅ Connected to Bybit for {}!", canonical_pair);

    let (mut write, mut read) = ws_stream.split();

    // 2. USE THE PAIR: Subscribe to the dynamically determined ticker
    let subscribe_msg = serde_json::json!({
        "op": "subscribe",
        "args": [format!("tickers.{}", bybit_symbol)] // Dynamically insert the symbol
    });

    if let Err(e) = write
        .send(Message::Text(subscribe_msg.to_string().into()))
        .await
    {
        eprintln!(
            "❌ Bybit Subscription failed for {}: {:?}",
            canonical_pair, e
        );
        return;
    }

    println!("📡 Subscribed to Bybit {} ticker.", canonical_pair);

    // Read incoming messages
    while let Some(msg) = read.next().await {
        if let Ok(text) = msg.and_then(|m| m.into_text()) {
            // NOTE: Add a check for ping messages or acknowledgment messages here if needed,
            // but assuming the current message parsing logic handles it.

            if let Ok(parsed) = serde_json::from_str::<BybitMessage>(&text) {
                if let (Some(_topic), Some(data)) = (parsed.topic, parsed.data) {
                    if let Ok(price) = data.last_price.parse::<f64>() {
                        let _ = tx.send(PriceUpdate {
                            source: "Bybit".to_string(),
                            // 3. Use the original canonical pair for output
                            pair: canonical_pair.clone(),
                            price,
                        });
                    }
                }
            }
        }
    }

    println!("⚠️ Bybit WebSocket for {} closed.", canonical_pair);
}

