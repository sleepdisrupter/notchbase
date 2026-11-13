//! Setting values are plain JSON.

pub use serde_json::{Map, Value};

pub type Values = Map<String, Value>;
