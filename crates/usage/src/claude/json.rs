//! Reads a JSON number, quoted or not, as long as it's finite.

use serde_json::Value;

pub(super) fn number(value: &Value) -> Option<f64> {
    let n = match value {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn numbers_may_be_quoted_if_finite() {
        assert_eq!(number(&json!(42)), Some(42.0));
        assert_eq!(number(&json!(" 7.5 ")), Some(7.5));
        assert_eq!(number(&json!("NaN")), None);
        assert_eq!(number(&json!("inf")), None);
        assert_eq!(number(&json!(null)), None);
        assert_eq!(number(&json!([1])), None);
    }
}
