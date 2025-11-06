use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::broadcast};

mod connectors;
use connectors::{
    binance::run_binance_connector, raydium::run_raydium_connector, state::PriceUpdate,
};

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel::<PriceUpdate>(100);

    // spawn Binance connector
    let tx_binance = tx.clone();
    tokio::spawn(async move {
        run_binance_connector(tx_binance).await;
    });

    // spawn raydium connector
    let tx_raydium = tx.clone();
    tokio::spawn(async move {
        run_raydium_connector(tx_raydium).await;
    });

    // WebSocket route
    let app = Router::new().route(
        "/ws",
        get({
            let tx = tx.clone();
            move |ws: WebSocketUpgrade| ws_handler(ws, tx.clone())
        }),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    println!("✅ Web3 Terminal backend running at ws://{addr}/ws");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, tx: broadcast::Sender<PriceUpdate>) -> impl IntoResponse {
    let rx = tx.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, rx))
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<PriceUpdate>) {
    println!("⚡ Client connected");

    // Split the socket into separate send and receive halves
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task that forwards price updates to the WebSocket client
    tokio::spawn(async move {
        while let Ok(update) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&update) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!("❌ Client disconnected (send failed)");
                    break;
                }
            }
        }
    });

    // Optionally handle messages from the client
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            println!("💬 Client says: {txt}");
        }
    }

    println!("❌ Client disconnected");
}
