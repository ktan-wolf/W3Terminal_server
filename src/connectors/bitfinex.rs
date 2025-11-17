use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_bitfinex_connector(tx: Sender<PriceUpdate>) {
    let url = "wss://api-pub.bitfinex.com/ws/2";
    println!("[Bitfinex] connecting : {url}");

    let (mut ws, _) = match connect_async(url).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[Bitfinex] ❌ Connection error: {:?}", e);
            return;
        }
    };

    println!("[Bitfinex] ✅ Connected");

    // Subscribe to ticker channel (better & more stable)
    let sub = serde_json::json!({
        "event": "subscribe",
        "channel": "ticker",
        "symbol": "tSOLUSD"
    });

    ws.send(Message::Text(sub.to_string().into()))
        .await
        .unwrap();

    println!("[Bitfinex] 📡 Subscribed to TICKER SOLUSD");

    while let Some(msg) = ws.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !msg.is_text() {
            continue;
        }

        let text = msg.to_text().unwrap();

        let parsed: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Ticker responses are arrays
        if !parsed.is_array() {
            continue;
        }

        let arr = parsed.as_array().unwrap();

        // Format: [ chanId, [ BID, BID_SIZE, ASK, ASK_SIZE, DAILY_CHANGE, DAILY_CHANGE_PERC,
        //          LAST_PRICE, VOLUME, HIGH, LOW ] ]
        if arr.len() < 2 {
            continue;
        }

        let data = &arr[1];

        if !data.is_array() {
            continue;
        }

        let inner = data.as_array().unwrap();

        // LAST PRICE = index 6
        if inner.len() > 6 {
            if let Some(price) = inner[6].as_f64() {
                // println!("[Bitfinex] price = {}", price);

                let _ = tx.send(PriceUpdate {
                    source: "Bitfinex".to_string(),
                    pair: "SOL/USD".to_string(),
                    price,
                });
            }
        }
    }
}
