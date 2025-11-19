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

// UPDATED SIGNATURE: It now accepts the pair string
pub async fn run_binance_connector(tx: Sender<PriceUpdate>, pair: String) {
    
    // Convert canonical pair "BTC/USDT" to binance format "btcusdt"
    let binance_symbol = pair.to_lowercase().replace("/", "");
    
    // Use the pair to construct the dynamic WebSocket URL
    let url = format!("wss://stream.binance.com:9443/ws/{}@trade", binance_symbol);
    
    println!("[Binance] connecting to pair: {}: {url}", pair);

    match connect_async(&url).await {
        Ok((mut ws_stream, _)) => {
            println!("[Binance] ✅ Connected to {}", pair);
            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        if let Ok(parsed) =
                            serde_json::from_str::<BinanceTrade>(msg.to_text().unwrap())
                        {
                            if let Ok(price) = parsed.price.parse::<f64>() {
                                // Use the requested pair for the PriceUpdate
                                let _ = tx.send(PriceUpdate {
                                    source: "Binance".to_string(),
                                    pair: pair.clone(), // Use the requested pair
                                    price,
                                });
                            }
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("[Binance] ❌ Connection error for {}: {:?}", pair, e),
    }
}
