use std::sync::OnceLock;

/// Function pointer type for offloading heavy deallocations to a background thread pool.
pub type BackgroundDropDispatcher = fn(Box<dyn FnOnce() + Send>);

static BACKGROUND_DISPATCHER: OnceLock<BackgroundDropDispatcher> = OnceLock::new();

/// Registers the background deallocation dispatcher used by [`discard_in_background`].
/// Subsequent calls after initial registration are ignored.
pub fn register_background_dispatcher(dispatcher: BackgroundDropDispatcher) {
    let _ = BACKGROUND_DISPATCHER.set(dispatcher);
}

/// Discards a heavy value in the background using the registered dispatcher if available,
/// or directly drops it synchronously if no dispatcher is registered (e.g. in deterministic unit tests).
pub fn discard_in_background<T: Send + 'static>(value: T) {
    if let Some(dispatcher) = BACKGROUND_DISPATCHER.get() {
        dispatcher(Box::new(move || drop(value)));
    } else {
        drop(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discard_in_background_without_dispatcher() {
        let val = vec![1, 2, 3, 4, 5];
        discard_in_background(val);
    }
}
