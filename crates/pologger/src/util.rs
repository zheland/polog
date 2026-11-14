//! Utilities module.

use core::fmt::{Debug, Display};

/// Blanket alias for `Display + Debug`.
///
/// Used by [`ExternalArg::Display`] to name the bound for values that
/// implement both traits.
/// Implementing it is automatic: this trait has no methods and no required
/// impls.
///
/// [`ExternalArg::Display`]: crate::ExternalArg::Display
pub trait DisplayAndDebug: Display + Debug {}
impl<T> DisplayAndDebug for T where T: ?Sized + Display + Debug {}
