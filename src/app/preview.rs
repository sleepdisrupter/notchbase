//! NOTCHBASE_PREVIEW starts the app on a sample state instead of the real services.

const PREVIEW_VAR: &str = "NOTCHBASE_PREVIEW";

pub(super) fn preview_name() -> Option<String> {
    std::env::var(PREVIEW_VAR).ok()
}

pub(super) fn previewing() -> bool {
    std::env::var_os(PREVIEW_VAR).is_some()
}
