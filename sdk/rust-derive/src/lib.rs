//! Proc macros for the Nendi CDC SDK.
//!
//! Provides `#[derive(NendiDeserialize)]` and `#[derive(NendiEvent)]`
//! for automatic deserialization of change events into user-defined types.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

// ─── NendiDeserialize ─────────────────────────────────────────────────────────

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

  if table_name.is_empty() {
    return syn::Error::new_spanned(
      &input.ident,
      "NendiDeserialize requires #[nendi(table = \"schema.table\")]",
    )
    .to_compile_error()
    .into();
  }

  let table_lit = &table_name;

  let expanded = quote! {
      impl nendi_sdk::NendiDeserialize for #name {
          fn table_name() -> &'static str {
              #table_lit
          }
      }
  };

  TokenStream::from(expanded)
}

// ─── NendiEvent ───────────────────────────────────────────────────────────────

/// Derive macro for multi-table event enums.
///
/// Use on an enum whose variants each contain a `NendiDeserialize` type.
/// The SDK matches on `table_name()` to dispatch to the correct variant.
/// Generated `from_event()` uses a `match` on a static string key — O(1)
/// branch prediction for ≤20 variants.
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
  let name = &input.ident;

  // Collect variants with their table attributes.
  let variants = match &input.data {
    Data::Enum(e) => &e.variants,
    _ => {
      return syn::Error::new_spanned(name, "NendiEvent can only be derived for enums")
        .to_compile_error()
        .into();
    }
  };

  let mut match_arms = Vec::new();

  for variant in variants {
    let variant_name = &variant.ident;

    // Extract table from variant's #[nendi(table = "...")] attribute
    let mut table_name = String::new();
    for attr in &variant.attrs {
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

    if table_name.is_empty() {
      return syn::Error::new_spanned(
        variant_name,
        "each NendiEvent variant requires #[nendi(table = \"schema.table\")]",
      )
      .to_compile_error()
      .into();
    }

    // Derive the inner type name
    let inner_ty = match &variant.fields {
      Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed.first().unwrap().ty,
      _ => {
        return syn::Error::new_spanned(
          variant_name,
          "NendiEvent variants must have exactly one unnamed field",
        )
        .to_compile_error()
        .into();
      }
    };

    let table_lit = &table_name;
    match_arms.push(quote! {
        #table_lit => {
            let payload: #inner_ty = event.payload()?;
            Ok(#name::#variant_name(payload))
        }
    });
  }

  let expanded = quote! {
      impl #name {
          /// Dispatch a raw `ChangeEvent` to the correct enum variant
          /// based on the source table name.
          ///
          /// Uses a static string `match` — O(1) branch prediction for
          /// the common case of ≤20 variants.
          pub fn from_event(
              event: &nendi_sdk::ChangeEvent,
          ) -> Result<Self, nendi_sdk::NendiError> {
              let key = format!("{}.{}", event.schema(), event.table());
              match key.as_str() {
                  #(#match_arms)*
                  other => Err(nendi_sdk::NendiError::Daemon {
                      code: 0,
                      message: format!("unknown table: {other}"),
                  }),
              }
          }
      }
  };

  TokenStream::from(expanded)
}
