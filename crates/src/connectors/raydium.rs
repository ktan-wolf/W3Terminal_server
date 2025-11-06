use super::state::PriceUpdate;
use futures_util::{SinkExt, StreamExt};
use rand;
use serde_json::json;
use tokio::sync::broadcast::Sender;
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const RAYDIUM_AMM_PROGRAM: &str = "AMM55ShkzCBXz9QWzLmfG7Vqvvuv9Wz7M8EYvFyyDpMx";
const HELIUS_WS_URL: &str =
    "wss://mainnet.helius-rpc.com/?api-key=7a025834-fd0a-4f62-9b34-6866d0e98eac";

pub async fn run_raydium_connector(tx: Sender<PriceUpdate>) {
    println!("[Helius] connecting to Solana WS...");

    let (ws_stream, _) = match connect_async(HELIUS_WS_URL).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[Helius] ❌ Failed to connect: {e:?}");
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to Raydium AMM program updates
    let sub_msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "programSubscribe",
        "params": [
            RAYDIUM_AMM_PROGRAM,
            { "encoding": "jsonParsed" }
        ]
    });

    if let Err(e) = write.send(Message::Text(sub_msg.to_string().into())).await {
        eprintln!("[Helius] ❌ Failed to send subscription: {e:?}");
        return;
    }

    println!("[Helius] ✅ Subscribed to Raydium AMM program");

    let tx_clone = tx.clone();

    // 🔁 Send fake prices periodically for testing
    tokio::spawn(async move {
        loop {
            let fake_price = 120.0 + (rand::random::<f64>() * 10.0);
            let _ = tx_clone.send(PriceUpdate {
                source: "Raydium".into(),
                pair: "SOL/USDC".into(),
                price: fake_price,
            });
            //println!("[Helius] 🔔 Sent periodic update, price {:.2}", fake_price);
            sleep(Duration::from_millis(300)).await;
        }
    });

    // ✅ Still listen to real messages from Helius if any arrive
    while let Some(msg) = read.next().await {
        if let Ok(Message::Text(txt)) = msg {
            if txt.contains("result") || txt.contains("notification") {
                let fake_price = 120.0 + (rand::random::<f64>() * 10.0);
                let _ = tx.send(PriceUpdate {
                    source: "Raydium".into(),
                    pair: "SOL/USDC".into(),
                    price: fake_price,
                });
                //println!("[Helius] 🔔 Received WS update, price {:.2}", fake_price);
            }
        }
    }
}
