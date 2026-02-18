//! NendiDeserialize trait.

/// Trait for types that can be deserialized from a Nendi change event.
///
/// Derive this trait with `#[derive(NendiDeserialize)]` from `nendi-derive`.
/// The struct must also implement `serde::Deserialize`.
///
/// # Example
///
/// ```ignore
/// #[derive(NendiDeserialize, Deserialize)]
/// #[nendi(table = "public.orders")]
/// pub struct Order {
///     pub id: i64,
///     pub status: String,
///     pub amount: i64,
/// }
/// ```
pub trait NendiDeserialize: serde::de::DeserializeOwned {
  /// Returns the fully qualified table name this type represents.
  fn table_name() -> &'static str;
}
