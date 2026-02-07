use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use wasm_bindgen::prelude::*;
use web_sys::Request;

/// Simple hash function to generate unique subscription IDs from SQL
pub(crate) fn md5_hash(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Helper to create a Promise with resolve/reject functions
pub(crate) fn create_promise() -> (js_sys::Promise, js_sys::Function, js_sys::Function) {
    let mut resolve_fn: Option<js_sys::Function> = None;
    let mut reject_fn: Option<js_sys::Function> = None;

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        resolve_fn = Some(resolve);
        reject_fn = Some(reject);
    });

    (promise, resolve_fn.unwrap(), reject_fn.unwrap())
}

// ── Cross-platform fetch ────────────────────────────────────────────────────
//
// `web_sys::window()` returns `None` in Node.js because the polyfilled
// `window` object fails the `instanceof Window` check in the wasm-bindgen
// glue code.  To support both browser and Node.js WASM targets we bind
// directly to the global `fetch()` function which is available in both
// environments (browsers natively, Node.js 18+ via `globalThis.fetch`).

#[wasm_bindgen]
extern "C" {
    /// Call the global `fetch(request)` — works in browsers AND Node.js 18+.
    #[wasm_bindgen(js_name = "fetch")]
    fn global_fetch_with_request(request: &Request) -> js_sys::Promise;

    /// Call `setTimeout(callback, delay)` from the global scope.
    #[wasm_bindgen(js_name = "setTimeout")]
    pub(crate) fn global_set_timeout(closure: &js_sys::Function, delay: i32) -> i32;
}

/// Portable replacement for `window.fetch_with_request(req)`.
///
/// Returns a `js_sys::Promise` that resolves to a `Response`.
pub(crate) fn fetch_request(request: &Request) -> js_sys::Promise {
    global_fetch_with_request(request)
}
