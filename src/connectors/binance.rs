use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Deserialize)]
struct BinanceTrade {
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    _quantity: String,
    #[serde(rename = "T")]
    timestamp: u64,
}

pub async fn run_binance_connector(tx: Sender<PriceUpdate>, pair: String) {
    let symbol = pair.to_lowercase().replace("/", "");
    let url = format!("wss://stream.binance.us:443/ws/{}@trade", symbol);

    println!("Connecting to Binance: {}", url);

    match connect_async(&url).await {
        Ok((mut ws_stream, _)) => {
            println!("Binance connected!");

            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(msg) => {
                        if msg.is_ping() {
                            ws_stream
                                .send(Message::Pong(bytes::Bytes::new()))
                                .await
                                .ok();
                            continue;
                        }

                        if msg.is_text() {
                            let txt = msg.to_text().unwrap();

                            match serde_json::from_str::<BinanceTrade>(txt) {
                                Ok(parsed) => {
                                    if let Ok(price) = parsed.price.parse::<f64>() {
                                        if let Err(err) = tx.send(PriceUpdate {
                                            source: "Binance".to_string(),
                                            pair: pair.clone(),
                                            price,
                                            timestamp: parsed.timestamp,
                                        }) {
                                            eprintln!("Binance TX send error: {:?}", err);
                                            break;
                                        }
                                    }
                                }
                                Err(err) => {
                                    eprintln!("Binance parse error: {:?}", err);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("Binance WS error: {:?}", err);
                        break;
                    }
                }
            }
        }
        Err(e) => eprintln!("Binance connection error: {:?}", e),
    }
}
