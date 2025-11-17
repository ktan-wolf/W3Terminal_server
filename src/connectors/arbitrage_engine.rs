use crate::connectors::state::PriceUpdate;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::broadcast::Sender;

#[derive(Debug, Clone, Serialize)]
pub struct ArbitrageOpportunity {
    pub pair: String,
    pub best_buy_source: String,
    pub best_buy_price: f64,
    pub best_sell_source: String,
    pub best_sell_price: f64,
    pub spread_percent: f64,
}

/// This struct wraps both the arbitrage opportunity and all latest prices
#[derive(Debug, Clone, Serialize)]
pub struct ArbitrageFeed {
    pub prices: Vec<PriceUpdate>,          // All prices
    pub opportunity: ArbitrageOpportunity, // Calculated arbitrage
}

pub struct ArbitrageEngine {
    market_state: HashMap<String, PriceUpdate>,
    tx: Sender<ArbitrageFeed>,
}

impl ArbitrageEngine {
    pub fn new(tx: Sender<ArbitrageFeed>) -> Self {
        Self {
            market_state: HashMap::new(),
            tx,
        }
    }

    pub fn process_price(&mut self, update: PriceUpdate) {
        self.market_state.insert(update.source.clone(), update);

        if self.market_state.len() >= 2 {
            let best_buy = self
                .market_state
                .values()
                .min_by(|a, b| a.price.partial_cmp(&b.price).unwrap())
                .unwrap();
            let best_sell = self
                .market_state
                .values()
                .max_by(|a, b| a.price.partial_cmp(&b.price).unwrap())
                .unwrap();
            let spread_percent = ((best_sell.price - best_buy.price) / best_buy.price) * 100.0;

            let arb = ArbitrageOpportunity {
                pair: best_buy.pair.clone(),
                best_buy_source: best_buy.source.clone(),
                best_buy_price: best_buy.price,
                best_sell_source: best_sell.source.clone(),
                best_sell_price: best_sell.price,
                spread_percent,
            };

            let feed = ArbitrageFeed {
                prices: self.market_state.values().cloned().collect(),
                opportunity: arb,
            };

            let _ = self.tx.send(feed);
        }
    }
}
