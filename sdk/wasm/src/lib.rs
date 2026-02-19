//! # Nendi SDK — WebAssembly bindings
//!
//! Exposes the Nendi CDC SDK to JavaScript/TypeScript via `wasm-bindgen`.
//! Use `wasm-pack build sdk/wasm --target web` to produce an npm-ready package.
//!
//! ## Usage (TypeScript / ESM)
//! ```ts
//! import init, { NendiClient, Transpiler } from "./pkg/nendi_sdk_wasm.js";
//!
//! await init();
//! const client = new NendiClient("ws://localhost:50052/stream", "my-consumer");
//! const stream  = await client.subscribe(["public.orders"]);
//! while (true) {
//!   const ev = await stream.next();
//!   if (!ev) break;
//!   console.log(ev.op(), ev.schema(), ev.table(), ev.payload_json());
//!   await stream.commit(ev.lsn());
//! }
//! ```

use wasm_bindgen::prelude::*;

pub mod client;
pub mod decode;
pub mod event;
pub mod transpile;

/// Initialise the wasm module (sets a panic hook for better error messages in browser console).
#[wasm_bindgen(start)]
pub fn start() {
  // console_error_panic_hook can be enabled later if added as a dependency
}

/// Returns the SDK version string.
#[wasm_bindgen]
pub fn version() -> String {
  env!("CARGO_PKG_VERSION").to_owned()
}
