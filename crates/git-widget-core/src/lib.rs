//! Core Git operations and repository scanner for git-widget.

/// Placeholder health check for Phase 1 workspace verification.
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::core_version;

    #[test]
    fn returns_crate_version() {
        assert_eq!(core_version(), "0.1.0");
    }
}
