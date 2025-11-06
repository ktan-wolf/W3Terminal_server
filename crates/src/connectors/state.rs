use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub source: String,
    pub pair: String,
    pub price: f64,
}
