//! Filter inspection (dry-run).

/// Result of a filter dry-run / inspection.
///
/// Returned by `SubscriptionBuilder::inspect()` to validate filters
/// before starting a live stream.
#[derive(Debug, Clone)]
pub struct FilterInspection {
  /// Estimated percentage of events that would pass the filter (0.0–1.0).
  pub pass_rate: f64,

  /// Any warnings about the filter configuration.
  pub warnings: Vec<String>,

  /// Whether the filter is valid (no errors).
  pub valid: bool,
}
