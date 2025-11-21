use crate::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH}; // Added for fallback timestamp
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct OkxTrade {
    px: String, // price
    ts: String, // timestamp
}

#[derive(Debug, Deserialize)]
struct _OkxData {
    data: Vec<OkxTrade>,
}

#[derive(Debug, Deserialize)]
struct OkxMsg {
    _arg: serde_json::Value,
    data: Option<Vec<OkxTrade>>,
}

pub async fn run_okx_connector(tx: Sender<PriceUpdate>, pair: String) {
    let url = "wss://ws.okx.com:8443/ws/v5/public";

    // Store the original requested pair for output
    let canonical_pair = pair.clone();

    // 1. Derive OKX Instrument ID (e.g., "BTC/USDC" -> "BTC-USDC")
    // OKX uses BASE-QUOTE format
    let inst_id = pair.replace("/", "-");

    println!(
        "OKX connecting to pair: {} (InstID: {})",
        canonical_pair, inst_id
    );

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            println!("OKX Connected");

            // 2. USE THE PAIR: Subscribe using the dynamic InstID
            let subscribe_msg = serde_json::json!({
                "op": "subscribe",
                "args": [{
                    "channel": "trades",
                    "instId": inst_id
                }]
            });

            // Send subscription message
            if ws_stream
                .send(Message::Text(subscribe_msg.to_string().into()))
                .await
                .is_err()
            {
                eprintln!(
                    "OKX Failed to send subscribe message for {}",
                    canonical_pair
                );
                return;
            }

            println!("OKX Subscribed to {} trades", canonical_pair);

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if !msg.is_text() {
                        continue;
                    }

                    if let Ok(parsed) = serde_json::from_str::<OkxMsg>(msg.to_text().unwrap()) {
                        if let Some(data) = parsed.data {
                            for t in data {
                                if let Ok(price) = t.px.parse::<f64>() {
                                    // Parse OKX timestamp (string ms) or fallback to system time
                                    let timestamp = t.ts.parse::<u64>().unwrap_or_else(|_| {
                                        SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                            as u64
                                    });

                                    let _ = tx.send(PriceUpdate {
                                        source: "OKX".to_string(),
                                        // 3. Use the original requested pair for output
                                        pair: canonical_pair.clone(),
                                        price,
                                        timestamp, // Added timestamp field
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("OKX Connection error for {}: {:?}", canonical_pair, e),
    }
}
