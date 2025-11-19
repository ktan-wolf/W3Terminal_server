use super::state::PriceUpdate;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::connect_async;

#[derive(Debug, Deserialize)]
struct BinanceTrade {
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    timestamp: u64,
}

pub async fn run_binance_connector(tx: Sender<PriceUpdate>, pair: String) {
    let symbol = pair.to_lowercase().replace("/", "");

    let url = format!("wss://stream.binance.com:9443/ws/{}@trade", symbol);
    println!("Binance connecting : {url}");

    match connect_async(url).await {
        Ok((mut ws_stream, _)) => {
            print!("Binance Connected");
            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        if let Ok(parsed) =
                            serde_json::from_str::<BinanceTrade>(msg.to_text().unwrap())
                        {
                            if let Ok(price) = parsed.price.parse::<f64>() {
                                let _ = tx.send(PriceUpdate {
                                    source: "Binance".to_string(),
                                    pair: pair.clone(),
                                    price,
                                    timestamp: parsed.timestamp,
                                });
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("Binance Connection error: {:?}", e),
    }
}
