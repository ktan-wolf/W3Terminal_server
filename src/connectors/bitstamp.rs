use crate::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;

// Bitstamp trade data is nested inside a "data" object
#[derive(Debug, Deserialize)]
struct BitstampTradeData {
    id: u64,
    timestamp: String, // Bitstamp sends this as a string of seconds
    amount_str: String,
    price_str: String, // We use the string version for safety
}

#[derive(Debug, Deserialize)]
struct BitstampMessage {
    event: String,
    channel: String,
    data: serde_json::Value, // We delay parsing this until we confirm it's a trade
}

pub async fn run_bitstamp_connector(tx: Sender<PriceUpdate>, pair: String) {
    const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

    // Bitstamp format: lowercase, no separator (e.g., "solusdc")
    let symbol = pair.replace("/", "").to_lowercase();
    let channel_name = format!("live_trades_{}", symbol);

    println!("Bitstamp Connecting: {}", BITSTAMP_WS_URL);

    let (ws_stream, _) = match connect_async(BITSTAMP_WS_URL).await {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("Bitstamp Connection error: {:?}", e);
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to trade stream
    let subscribe_msg = json!({
        "event": "bts:subscribe",
        "data": {
            "channel": channel_name
        }
    });

    if let Err(e) = write
        .send(tokio_tungstenite::tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .await
    {
        eprintln!("Bitstamp Failed to subscribe: {:?}", e);
        return;
    }

    println!("Bitstamp Subscribed to {}", channel_name);

    while let Some(msg) = read.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[Bitstamp] Error reading message: {:?}", e);
                break;
            }
        };

        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
            // First parse the outer structure to check the event type
            let v: BitstampMessage = match serde_json::from_str(&txt) {
                Ok(v) => v,
                Err(_) => continue, // Skip heartbeats or malformed json
            };

            // Only process if the event is strictly a "trade"
            if v.event == "trade" {
                let trade: BitstampTradeData = match serde_json::from_value(v.data) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Parse price (string -> f64)
                let price: f64 = trade.price_str.parse().unwrap_or(0.0);

                // Parse timestamp (string -> u64)
                // Bitstamp sends Unix seconds (e.g. "1630000000")
                let ts: u64 = trade.timestamp.parse().unwrap_or(0);

                let update = PriceUpdate {
                    source: "Bitstamp".into(),
                    pair: pair.clone(),
                    price,
                    timestamp: ts,
                };

                let _ = tx.send(update);
            }
        }
    }

    println!("Bitstamp Disconnected");
}
