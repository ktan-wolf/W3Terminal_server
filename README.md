# 🧠 Web3 Terminal — High-Performance Arbitrage Dashboard

**Web3 Terminal** is a high-performance, real-time cryptocurrency arbitrage dashboard built with **Rust**, **Axum**, **Solana**, and **Next.js**.  
It provides a unified interface to monitor fragmented liquidity across **Centralized Exchanges (CEXs)** and **Decentralized Exchanges (DEXs)** in real-time.

> ⚡ Built for speed, concurrency, and reliability — powered by Rust on the backend and Next.js on the frontend.

---

## 🚀 Current Status

🟢 **In Active Development**

The terminal now:
- Streams and normalizes **live price data** from multiple exchanges:
  - **Binance (CEX)** — `SOL/USDT`
  - **Jupiter (DEX)** — `SOL/USDC` via API
  - **Raydium (DEX)** — `SOL/USDC` on-chain liquidity pool
- Calculates and displays **arbitrage opportunities** across all sources in real-time.
- Sends **both individual prices and arbitrage info** to the frontend over WebSocket.

---

## ✨ Features

### 🦀 High-Performance Rust Backend
- Built with **[Axum](https://github.com/tokio-rs/axum)** and **Tokio** for maximum concurrency.
- Designed for low-latency, high-throughput real-time data handling.

### 📡 Real-Time WebSocket Broadcasting
- A concurrent-safe **WebSocket server** broadcasts:
  - **Normalized price updates** from all connected exchanges (Binance, Jupiter, Raydium)
  - **Arbitrage opportunities** with best buy/sell source and spread percentage.

### 💱 Multi-Source Data Pipelines
- **CEX Data Pipeline:** Streams live trades from **Binance** WebSocket (`SOL/USDT`).
- **DEX Data Pipeline:** Streams prices from **Raydium** on-chain (`SOL/USDC`) and **Jupiter API V3** (`SOL/USDC`).
- Normalizes all prices into a single `PriceUpdate` structure for unified processing.

### 📊 Arbitrage Engine
- Continuously compares prices across **Binance**, **Jupiter**, and **Raydium**.
- Computes:
  - `best_buy_source` and `best_buy_price`
  - `best_sell_source` and `best_sell_price`
  - `spread_percent`  
- Broadcasts **ArbitrageFeed** to all connected clients in real-time.

### 🖥 Frontend Dashboard
- Built with **Next.js** (App Router) and **React Hooks**.
- Displays:
  - Live prices from all exchanges
  - Real-time arbitrage opportunities
- Automatically updates when any price changes or arbitrage arises.

---

## 🔧 Tech Stack

| Layer           | Technology                                      |
|-----------------|------------------------------------------------|
| Backend         | Rust, Axum, Tokio, tokio-tungstenite          |
| Solana DEX Data | solana-client, solana-pubsub-client           |
| Frontend        | Next.js, React, TypeScript                     |
| Real-Time Comms | WebSocket (broadcast channels in Rust)        |
| APIs            | Jupiter V3 API for DEX price feeds            |

---

## ⚡ Next Steps
- Sprint 4: Store historical price data and visualize charts (TimescaleDB + TradingView lightweight-charts).  
- Sprint 5: Implement one-click Solana trade execution using Jupiter API + wallet adapter.  
- Sprint 6: UI/UX polish, responsive design, multiple exchange integrations, and deployment.

