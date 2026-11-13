//! The one-line message the shelf shows after an operation, good or bad.

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub text: String,
    pub ok: bool,
}

impl Notice {
    pub(super) fn ok(text: impl Into<String>) -> Self {
        Self { text: text.into(), ok: true }
    }

    pub(super) fn failed(text: impl Into<String>) -> Self {
        Self { text: text.into(), ok: false }
    }
}
