mod connectors;

use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use connectors::{
    arbitrage_engine::{ArbitrageEngine, ArbitrageFeed},
    binance::run_binance_connector,
    db::{init_db, insert_price},
    jupiter::run_dex_connector,
    raydium::run_raydium_connector,
    state::PriceUpdate,
};
use futures_util::{SinkExt, StreamExt};
use sqlx::PgPool;
use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::broadcast};

#[tokio::main]
async fn main() {
    let database_url = "postgresql://ktan:whoami@localhost:5432/web3terminal";
    let db_pool = init_db(&database_url).await;

    // Channel for PriceUpdate from connectors
    let (tx_price, rx_price) = broadcast::channel::<PriceUpdate>(200);

    // Channel for ArbitrageFeed to WebSocket clients
    let (tx_arb, _rx_arb) = broadcast::channel::<ArbitrageFeed>(200);

    // Spawn connectors
    tokio::spawn(run_binance_connector(tx_price.clone()));
    tokio::spawn(run_dex_connector(tx_price.clone()));
    tokio::spawn(run_raydium_connector(tx_price.clone()));

    // Spawn arbitrage engine
    tokio::spawn({
        let tx_arb = tx_arb.clone();

        let db_pool = db_pool.clone();

        async move {
            let mut engine = ArbitrageEngine::new(tx_arb);
            let mut rx_price = rx_price;

            while let Ok(update) = rx_price.recv().await {
                let db_pool = db_pool.clone();
                let update_clone = update.clone();
                tokio::spawn(async move {
                    insert_price(&db_pool, &update_clone).await;
                });

                engine.process_price(update);
            }
        }
    });

    // WebSocket route for combined feed
    let app = Router::new().route(
        "/ws/arb",
        get({
            let tx_arb = tx_arb.clone();
            move |ws: WebSocketUpgrade| ws_handler(ws, tx_arb.clone())
        }),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    println!("✅ Web3 Terminal Arbitrage feed running at ws://{addr}/ws/arb");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    tx: broadcast::Sender<ArbitrageFeed>,
) -> impl IntoResponse {
    let rx = tx.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, rx))
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<ArbitrageFeed>) {
    println!("⚡ Client connected to combined feed");

    let (mut sender, mut receiver) = socket.split();

    // Send updates
    tokio::spawn(async move {
        while let Ok(feed) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&feed) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!("❌ Client disconnected (send failed)");
                    break;
                }
            }
        }
    });

    // Optional: receive messages from client
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            println!("💬 Client says: {txt}");
        }
    }

    println!("❌ Client disconnected");
}
