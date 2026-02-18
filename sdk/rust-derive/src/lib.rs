//! Proc macros for the Nendi CDC SDK.
//!
//! Provides `#[derive(NendiDeserialize)]` and `#[derive(NendiEvent)]`
//! for automatic deserialization of change events into user-defined types.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive macro that implements `NendiDeserialize` for a struct.
///
/// The struct must also derive `serde::Deserialize`. Nendi uses serde
/// internally to map column values to struct fields.
///
/// # Attributes
///
/// - `#[nendi(table = "schema.table")]` — binds this struct to a specific
///   PostgreSQL table. Required for typed stream subscriptions.
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
#[proc_macro_derive(NendiDeserialize, attributes(nendi))]
pub fn derive_nendi_deserialize(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let name = &input.ident;

  // Extract the table name from #[nendi(table = "...")]
  let mut table_name = String::new();
  for attr in &input.attrs {
    if attr.path().is_ident("nendi") {
      let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("table") {
          let value = meta.value()?;
          let lit: syn::LitStr = value.parse()?;
          table_name = lit.value();
        }
        Ok(())
      });
    }
  }

  let table_lit = &table_name;

  let expanded = quote! {
      impl crate::deserialize::traits::NendiDeserialize for #name {
          fn table_name() -> &'static str {
              #table_lit
          }
      }
  };

  TokenStream::from(expanded)
}

/// Derive macro for multi-table event enums.
///
/// Use on an enum whose variants each contain a `NendiDeserialize` type.
/// The SDK uses the table name to dispatch to the correct variant.
///
/// # Example
///
/// ```ignore
/// #[derive(NendiEvent)]
/// pub enum AppEvent {
///     #[nendi(table = "public.orders")]
///     Order(Order),
///     #[nendi(table = "public.payments")]
///     Payment(Payment),
/// }
/// ```
#[proc_macro_derive(NendiEvent, attributes(nendi))]
pub fn derive_nendi_event(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let _name = &input.ident;

  // Stub: full implementation will generate a match on table name
  // and deserialize into the correct variant
  let expanded = quote! {};

  TokenStream::from(expanded)
}
