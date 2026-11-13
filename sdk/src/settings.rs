//! A tool's saved settings, read by key. Missing or mistyped values read as empty.

use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::protocol::{Value, Values};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings(Values);

impl Settings {
    pub fn new(values: Values) -> Self {
        Self(values)
    }

    pub(crate) fn over(defaults: Values, values: Values) -> Self {
        let mut merged = defaults;
        merged.extend(values);
        Self(merged)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or_default()
    }

    pub fn number(&self, key: &str) -> f64 {
        self.get(key).and_then(Value::as_f64).unwrap_or_default()
    }

    pub fn text(&self, key: &str) -> &str {
        self.get(key).and_then(Value::as_str).unwrap_or_default()
    }

    pub fn string(&self, key: &str) -> Option<String> {
        Some(self.text(key).trim()).filter(|text| !text.is_empty()).map(str::to_owned)
    }

    pub fn path(&self, key: &str) -> Option<PathBuf> {
        self.string(key).map(PathBuf::from)
    }

    pub fn parse<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_value(self.get(key)?.clone()).ok()
    }

    pub fn values(&self) -> &Values {
        &self.0
    }

    pub(crate) fn set(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_owned(), value);
    }
}
