//! An order-keeping parallel map: every core natively (rayon), one item
//! after another on wasm. Callers make each item's result independent of
//! the others, so the output is the same either way.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync + Send) -> Vec<R> {
    use rayon::prelude::*;
    items.into_par_iter().map(f).collect()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync + Send) -> Vec<R> {
    items.into_iter().map(f).collect()
}
