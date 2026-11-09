//! Minimal no-C replacement for `clear_on_drop` so wasm builds do not need clang.

pub fn clear_stack(_n: usize) {}

pub fn clear_stack_on_return<F, R>(_n: usize, f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}

pub fn clear_stack_on_return_fnonce<F, R>(_n: usize, f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}

pub mod clear {
    pub trait Clear {
        fn clear(&mut self);
    }

    impl<T: Default> Clear for T {
        fn clear(&mut self) {
            *self = T::default();
        }
    }
}
