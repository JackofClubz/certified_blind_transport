use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Secret wrapper: zeroised on drop; `Debug` prints `<secret>`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Secret<T: Zeroize>(pub T);

impl<T: Zeroize> Secret<T> {
    pub fn new(v: T) -> Self {
        Self(v)
    }
    pub fn expose(&self) -> &T {
        &self.0
    }
    pub fn expose_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<secret>")
    }
}
