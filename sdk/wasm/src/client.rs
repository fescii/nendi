//! `NendiClient` — WebSocket-based CDC stream client for the browser.
//!
//! In the browser/wasm context, tonic gRPC over TCP is not available.
//! Instead, `NendiClient` connects to the Nendi HTTP gateway's WebSocket
//! endpoint, which proxies the same binary event frames over the wire.
//!
//! ## Wire protocol
//! - Client → Server: JSON subscribe request (`{"consumer":"…","tables":[…],"resume":0}`)
//! - Server → Client: binary frames in the Nendi TLV format (see `decode.rs`)
//! - Client → Server: JSON commit ACK (`{"type":"commit","lsn":12345}`)
//!
//! ## Usage (JavaScript)
//! ```js
//! const client = new NendiClient("ws://nendi:8080/stream", "my-consumer");
//! const stream  = await client.subscribe(["public.orders", "public.shipments"]);
//! while (true) {
//!   const ev = await stream.next();
//!   if (!ev) break;
//!   console.log(ev.op(), ev.table(), ev.payload_json());
//!   await stream.commit(ev.lsn());
//! }
//! ```

use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{BinaryType, MessageEvent, WebSocket};

use crate::decode::decode_frame;
use crate::event::WasmChangeEvent;

// ─── NendiClient ─────────────────────────────────────────────────────────────

/// A connected client to the Nendi gateway WebSocket endpoint.
#[wasm_bindgen]
pub struct NendiClient {
  url: String,
  consumer: String,
}

#[wasm_bindgen]
impl NendiClient {
  /// Create a new `NendiClient`.
  ///
  /// - `url` — WebSocket URL to the Nendi gateway, e.g. `"ws://nendi:8080/stream"`.
  /// - `consumer` — unique consumer name for offset tracking.
  #[wasm_bindgen(constructor)]
  pub fn new(url: &str, consumer: &str) -> Self {
    Self {
      url: url.to_owned(),
      consumer: consumer.to_owned(),
    }
  }

  /// Subscribe to a set of tables and return a `WasmStream`.
  ///
  /// `tables` is a JS array of fully-qualified table names,
  /// e.g. `["public.orders", "public.shipments"]`.
  ///
  /// `resume_lsn` is the LSN to resume from (pass `0` to start from the
  /// current head). Matches the value returned by a previous `stream.commit()`.
  pub fn subscribe(&self, tables: Array, resume_lsn: f64) -> Promise {
    let url = self.url.clone();
    let consumer = self.consumer.clone();
    let tables_vec: Vec<String> = (0..tables.length())
      .filter_map(|i| tables.get(i).as_string())
      .collect();

    wasm_bindgen_futures::future_to_promise(async move {
      let stream = WasmStream::connect(&url, &consumer, &tables_vec, resume_lsn as u64).await?;
      Ok(JsValue::from(stream))
    })
  }
}

// ─── WasmStream ──────────────────────────────────────────────────────────────

/// An active CDC event stream driven by a WebSocket connection.
///
/// Call `.next()` in a loop to receive events; call `.commit(lsn)` to
/// acknowledge processed events.
#[wasm_bindgen]
pub struct WasmStream {
  socket: WebSocket,
  /// Buffered events decoded from WebSocket messages.
  /// We use a JS Array as a simple FIFO queue visible from both the
  /// message handler closure and `.next()`, sharing via Rc<RefCell<…>>
  /// would require unsafe in wasm. Instead we buffer in a Vec on the
  /// Rust side and pull from it on .next() calls.
  ///
  /// For wasm single-thread, a `Rc<RefCell<VecDeque>>` is fine.
  buffer: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<WasmChangeEvent>>>,
  /// Track whether the WebSocket is still open.
  open: std::rc::Rc<std::cell::RefCell<bool>>,
  /// Pending resolve for the next `.next()` Promise, if any.
  pending: std::rc::Rc<std::cell::RefCell<Option<js_sys::Function>>>,
}

impl WasmStream {
  async fn connect(
    url: &str,
    consumer: &str,
    tables: &[String],
    resume: u64,
  ) -> Result<WasmStream, JsValue> {
    let ws = WebSocket::new(url)?;
    ws.set_binary_type(BinaryType::Arraybuffer);

    let buffer: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<WasmChangeEvent>>> =
      std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new()));
    let open: std::rc::Rc<std::cell::RefCell<bool>> =
      std::rc::Rc::new(std::cell::RefCell::new(false));
    let pending: std::rc::Rc<std::cell::RefCell<Option<js_sys::Function>>> =
      std::rc::Rc::new(std::cell::RefCell::new(None));

    // ── onopen: send subscribe request ───────────────────────────────────
    {
      let ws_clone = ws.clone();
      let consumer_clone = consumer.to_owned();
      let tables_clone = tables.to_vec();
      let open_clone = open.clone();
      let on_open = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
        *open_clone.borrow_mut() = true;
        let req = serde_json::json!({
            "type":     "subscribe",
            "consumer": consumer_clone,
            "tables":   tables_clone,
            "resume":   resume,
        });
        let _ = ws_clone.send_with_str(&req.to_string());
      });
      ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
      on_open.forget();
    }

    // ── onmessage: decode binary frame and push to buffer ────────────────
    {
      let buffer_clone = buffer.clone();
      let pending_clone = pending.clone();
      let on_message = Closure::<dyn FnMut(_)>::new(move |ev: MessageEvent| {
        let data = ev.data();
        if let Some(ab) = data.dyn_ref::<js_sys::ArrayBuffer>() {
          let bytes = Uint8Array::new(ab).to_vec();
          match decode_frame(&bytes) {
            Ok(event) => {
              // If someone is awaiting .next(), resolve immediately.
              let resolver = pending_clone.borrow_mut().take();
              if let Some(resolve) = resolver {
                let _ = resolve.call1(&JsValue::undefined(), &JsValue::from(event));
              } else {
                buffer_clone.borrow_mut().push_back(event);
              }
            }
            Err(e) => {
              web_sys::console::warn_1(&format!("nendi-wasm: frame decode error: {e}").into());
            }
          }
        }
      });
      ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
      on_message.forget();
    }

    // ── onclose: mark stream closed ───────────────────────────────────────
    {
      let open_clone = open.clone();
      let on_close = Closure::<dyn FnMut(_)>::new(move |_: web_sys::CloseEvent| {
        *open_clone.borrow_mut() = false;
      });
      ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
      on_close.forget();
    }

    // Wait for the connection to open (poll open flag).
    let open_poll = open.clone();
    let wait = Promise::new(&mut |resolve, _reject| {
      // Poll via requestAnimationFrame / setTimeout — simplified:
      // for now we resolve immediately after setting handlers.
      // In production, onopen fires the resolve.
      let _ = resolve.call0(&JsValue::undefined());
      let _ = open_poll; // capture
    });
    JsFuture::from(wait).await?;

    Ok(WasmStream {
      socket: ws,
      buffer,
      open,
      pending,
    })
  }
}

#[wasm_bindgen]
impl WasmStream {
  /// Receive the next change event.
  ///
  /// Returns a `Promise<WasmChangeEvent | null>`. `null` means the stream
  /// is closed. Awaiting this in a loop gives an async event iterator.
  pub fn next(&self) -> Promise {
    // If there's a buffered event return it immediately.
    if let Some(ev) = self.buffer.borrow_mut().pop_front() {
      return Promise::resolve(&JsValue::from(ev));
    }
    // If the stream is closed, return null.
    if !*self.open.borrow() {
      return Promise::resolve(&JsValue::NULL);
    }
    // Otherwise, park a resolver that will be called when the next
    // frame arrives in the onmessage handler.
    let pending = self.pending.clone();
    Promise::new(&mut move |resolve, _reject| {
      *pending.borrow_mut() = Some(resolve);
    })
  }

  /// Acknowledge that events up to `lsn` have been processed.
  ///
  /// Sends a JSON commit message to the gateway which forwards it to
  /// the Nendi server to advance the stored offset.
  pub fn commit(&self, lsn: f64) -> Result<(), JsValue> {
    let msg = serde_json::json!({
        "type": "commit",
        "lsn":  lsn as u64,
    });
    self.socket.send_with_str(&msg.to_string())?;
    Ok(())
  }

  /// Close the WebSocket connection.
  pub fn close(&self) -> Result<(), JsValue> {
    self.socket.close()?;
    Ok(())
  }

  /// Returns `true` if the WebSocket connection is open.
  pub fn is_open(&self) -> bool {
    *self.open.borrow()
  }
}
