mod connectors;

use axum::extract::Extension;
use axum::{
    Json, Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use connectors::{
    // RE-ADDED ArbitrageEngine and ArbitrageFeed imports
    arbitrage_engine::{ArbitrageEngine, ArbitrageFeed},
    backpack::run_backpack_connector,
    binance::run_binance_connector,
    bitfinex::run_bitfinex_connector,
    bitget::run_bitget_connector,
    bybit::run_bybit_connector,
    coinbase::run_coinbase_connector,
    db::{HistoricalPrice, fetch_historical, init_db, insert_price},
    htx::run_htx_connector,
    jupiter::run_jupiter_connector,
    kraken::run_kraken_connector,
    kucoin::run_kucoin_connector,
    okx::run_okx_connector,
    orca::run_orca_connector,
    raydium::run_raydium_connector,
    state::PriceUpdate,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use tokio::{net::TcpListener, sync::broadcast};

// --- Struct for Client Request (Two Inputs) ---
#[derive(Debug, Deserialize)]
struct PairRequest {
    token_a: String,
    token_b: String,
}

#[tokio::main]
async fn main() {
    let database_url = "postgresql://ktan:whoami@localhost:5432/web3terminal";
    let db_pool = init_db(&database_url).await;

    // Build router with WebSocket and historical HTTP endpoint
    let app = Router::new()
        // --- NEW Dynamic Connector Spawning WebSocket Route ---
        .route(
            "/ws/subscribe",
            get({
                let db_pool = db_pool.clone();
                // We pass the db_pool to the new handler
                move |ws: WebSocketUpgrade| ws_handler_subscribe(ws, db_pool.clone())
            }),
        )
        // Existing Historical HTTP Route
        .route("/historical", get(historical_prices_handler))
        .layer(Extension(db_pool.clone())); // pass db_pool to handlers

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    println!("Dynamic Price Subscriber running at ws://{addr}/ws/subscribe (Arbitrage Enabled)");
    println!("Historical prices endpoint running at http://{addr}/historical?pair=SOL/USDT");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// --- Handler for Dynamic Connector Spawning (New) ---

async fn ws_handler_subscribe(ws: WebSocketUpgrade, db_pool: PgPool) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket_subscribe(socket, db_pool))
}

async fn handle_socket_subscribe(mut socket: WebSocket, db_pool: PgPool) {
    println!("Client connected to dynamic subscription feed. Awaiting pair request...");
    let (mut sender, mut receiver) = socket.split();

    // 1. Await the initial message with the token pair
    let pair = match receiver.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<PairRequest>(&text) {
            Ok(req) => {
                let pair_str = format!(
                    "{}/{}",
                    req.token_a.to_uppercase(),
                    req.token_b.to_uppercase()
                );
                println!("Client requested pair: {}", pair_str);
                pair_str
            }
            Err(e) => {
                println!("Invalid pair request format: {}. Disconnecting.", e);
                return;
            }
        },
        _ => {
            println!("Client disconnected before sending pair request.");
            return;
        }
    };

    // --- 2. Create channels for Price Updates and Arbitrage Feeds ---
    // This is the channel the connectors send raw prices into (for the requested pair)
    let (tx_price_raw, mut rx_price_raw) = broadcast::channel::<PriceUpdate>(1000);

    // This is the channel the ArbitrageEngine sends the final computed feed into
    let (tx_arb_feed, mut rx_arb_feed) = broadcast::channel::<ArbitrageFeed>(1000);

    let mut connector_handles: Vec<JoinHandle<()>> = Vec::new();

    println!("Spawning dedicated connectors for pair: {}", pair);

    // DYNAMIC SPAWNING: Pass raw price sender (tx_price_raw) to all connectors.
    connector_handles.push(tokio::spawn(run_binance_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_coinbase_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    // // REPLICATE THIS FOR ALL CONNECTORS
    connector_handles.push(tokio::spawn(run_jupiter_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_raydium_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_kraken_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_okx_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_bitfinex_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_bybit_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_kucoin_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_bitget_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_htx_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_orca_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));
    connector_handles.push(tokio::spawn(run_backpack_connector(
        tx_price_raw.clone(),
        pair.clone(),
    )));

    // 3. Spawn Arbitrage Engine for THIS specific pair stream
    tokio::spawn(async move {
        let mut engine = ArbitrageEngine::new(tx_arb_feed);

        // The engine consumes the raw price updates from the connectors
        while let Ok(update) = rx_price_raw.recv().await {
            // Insert into DB (using a separate task to avoid blocking the engine)
            let db_pool = db_pool.clone();
            let update_clone = update.clone();
            tokio::spawn(async move {
                insert_price(&db_pool, &update_clone).await;
            });

            // Process the price and calculate arbitrage for THIS pair
            engine.process_price(update);
        }
    });

    // 4. Stream the FINAL ArbitrageFeed result to the client
    let pair_clone_stream = pair.clone();
    tokio::spawn(async move {
        // This loop receives the calculated ArbitrageFeed structs
        while let Ok(feed) = rx_arb_feed.recv().await {
            if let Ok(json) = serde_json::to_string(&feed) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!(
                        "Client disconnected from {} feed (send failed)",
                        pair_clone_stream
                    );
                    break;
                }
            }
        }
    });

    // 5. Cleanup: Wait for client disconnection
    while let Some(Ok(_)) = receiver.next().await {}

    println!(
        "Client disconnected from dynamic subscription for {}. Shutting down dedicated connectors...",
        pair
    );

    // 6. Abort all spawned connector tasks when the client disconnects
    for handle in connector_handles {
        handle.abort();
    }
}

// --- Handler for Historical Prices (Original) ---

async fn historical_prices_handler(
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
    Extension(db_pool): Extension<PgPool>,
) -> Json<Vec<HistoricalPrice>> {
    let pair = match params.get("pair") {
        Some(p) => p.as_str(),
        None => "SOL/USDT",
    };
    let data = fetch_historical(&db_pool, pair).await;
    Json(data)
}
