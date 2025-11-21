use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub source: String,
    pub pair: String,
    pub price: f64,
    pub timestamp: u64, // Unix timestamp (ms)
}

// --- In-Memory Cache System ---

pub struct MarketCache {
    // Stores history per pair: "SOL/USDT" -> [Price1, Price2, ...]
    pub vaults: HashMap<String, VecDeque<PriceUpdate>>,
    pub max_per_pair: usize,
}

impl MarketCache {
    pub fn new(max_per_pair: usize) -> Self {
        Self {
            vaults: HashMap::new(),
            max_per_pair,
        }
    }

    pub fn add(&mut self, update: PriceUpdate) {
        // Get or create the deque for this specific pair
        let history = self
            .vaults
            .entry(update.pair.clone())
            .or_insert_with(|| VecDeque::with_capacity(self.max_per_pair));

        // Maintain size limit
        if history.len() >= self.max_per_pair {
            history.pop_front(); // Remove oldest
        }
        history.push_back(update); // Add newest
    }

    pub fn get_history(&self, pair: &str) -> Vec<PriceUpdate> {
        match self.vaults.get(pair) {
            Some(deque) => deque.iter().cloned().collect(),
            None => Vec::new(),
        }
    }
}

