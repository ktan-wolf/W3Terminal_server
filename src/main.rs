pub mod arbitrage_engine;
mod connectors;
pub mod state;
#[allow(unused_imports)]
pub use state::*;

use arbitrage_engine::{ArbitrageEngine, ArbitrageFeed};
use axum::{
    Router,
    extract::State, // Use Axum State instead of Extension for cleaner architecture
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use connectors::{
    backpack::run_backpack_connector, binance::run_binance_connector,
    bitfinex::run_bitfinex_connector, bitget::run_bitget_connector, bybit::run_bybit_connector,
    coinbase::run_coinbase_connector, htx::run_htx_connector, jupiter::run_jupiter_connector,
    kraken::run_kraken_connector, kucoin::run_kucoin_connector, okx::run_okx_connector,
    orca::run_orca_connector, raydium::run_raydium_connector,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use state::{MarketCache, PriceUpdate};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

// --- Types ---

#[derive(Debug, Deserialize)]
struct PairRequest {
    token_a: String,
    token_b: String,
}

// Shared State Container
struct AppState {
    cache: Arc<Mutex<MarketCache>>,
}

#[tokio::main]
async fn main() {
    // 1. Initialize In-Memory Cache (No DB)
    // Holds 500 prices per pair in RAM
    let market_cache = Arc::new(Mutex::new(MarketCache::new(500)));
    let app_state = Arc::new(AppState {
        cache: market_cache,
    });

    // 2. Build Router
    let app = Router::new()
        .route("/ws/subscribe", get(ws_handler_subscribe))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    println!(
        "Engine running on ws://{}/ws/subscribe (In-Memory Mode)",
        addr
    );

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- WebSocket Handlers ---

async fn ws_handler_subscribe(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket_subscribe(socket, state))
}

async fn handle_socket_subscribe(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Wait for Client to request a pair (e.g., SOL/USDT)
    let pair = match receiver.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<PairRequest>(&text) {
            Ok(req) => format!(
                "{}/{}",
                req.token_a.to_uppercase(),
                req.token_b.to_uppercase()
            ),
            _ => return, // Invalid JSON
        },
        _ => return, // Disconnected or error
    };

    println!("Client subscribed to: {}", pair);

    // 2. INSTANTLY send Cached History (Get data, Drop lock, THEN Send)
    let history_json = {
        // Open a new scope just for locking
        let lock = state.cache.lock().unwrap();
        let history = lock.get_history(&pair);

        if !history.is_empty() {
            serde_json::to_string(&history).ok()
        } else {
            None
        }
    };

    if let Some(json) = history_json {
        if let Err(e) = sender.send(Message::Text(json.into())).await {
            println!("Failed to send history: {}", e);
            return;
        }
    }

    // 3. Setup Broadcast Channels for Live Data
    let (tx_price_raw, mut rx_price_raw) = broadcast::channel::<PriceUpdate>(5000);
    let (tx_arb_feed, mut rx_arb_feed) = broadcast::channel::<ArbitrageFeed>(5000);

    // 4. Spawn Connectors
    tokio::spawn(run_binance_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_backpack_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_bitfinex_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_bitget_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_bybit_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_coinbase_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_htx_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_jupiter_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_kraken_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_kucoin_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_okx_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_raydium_connector(tx_price_raw.clone(), pair.clone()));
    tokio::spawn(run_orca_connector(tx_price_raw.clone(), pair.clone()));
    // ... spawn others ...

    // 5. Spawn Arbitrage Engine
    tokio::spawn(async move {
        let mut engine = ArbitrageEngine::new(tx_arb_feed);
        while let Ok(update) = rx_price_raw.recv().await {
            // A. Update the Global Cache
            {
                let mut lock = state.cache.lock().unwrap();
                lock.add(update.clone());
            }

            // B. Calculate Arbitrage
            engine.process_price(update);
        }
    });

    // 6. Stream Final Results to Client
    while let Ok(feed) = rx_arb_feed.recv().await {
        if let Ok(json) = serde_json::to_string(&feed) {
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    }
}
