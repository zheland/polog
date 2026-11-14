use core::fmt::{self, Debug, Formatter, Pointer};
use core::ptr::NonNull;

use crate::private;

/// A wrapper around a pointer that can be sent and shared across thread
/// boundaries.
///
/// The wrapped pointer type `T` must implement [`Ptr`], which is sealed and
/// implemented for raw pointers and non-null pointers.
///
/// This type implements [`Send`] and [`Sync`] even while the wrapped pointer
/// type does not.
/// Unwrapping the pointer with [`into_inner`] is unsafe because the caller must
/// ensure that the pointed-to memory is safe to access from the current thread.
///
/// [`into_inner`]: Self::into_inner
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct UnsafeSendablePtr<T>(T);

/// A sealed trait for pointer types that can be wrapped in
/// [`UnsafeSendablePtr`].
///
/// This trait is implemented for raw pointers and `NonNull` pointers.
/// It is sealed and cannot be implemented outside this crate.
pub trait Ptr: private::Sealed {}

impl<T> UnsafeSendablePtr<T> {
    /// Creates a new [`UnsafeSendablePtr`] wrapping `ptr`.
    ///
    /// Construction is safe, but the resulting wrapper implements [`Send`] and
    /// [`Sync`] unconditionally (see [`UnsafeSendablePtr`] for the safety
    /// contract).
    /// Unwrapping or dereferencing the pointer requires unsafe code.
    #[inline]
    pub const fn new(ptr: T) -> Self
    where
        T: Ptr,
    {
        Self(ptr)
    }

    /// Unwraps the pointer, returning the inner pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointed-to memory is safe to access from
    /// the current thread.
    /// In particular, if this wrapper was sent from another thread, the caller
    /// must uphold the thread-safety requirements of the wrapped pointer type.
    #[inline]
    pub unsafe fn into_inner(self) -> T {
        self.0
    }
}

impl<T> private::Sealed for *const T where T: ?Sized {}
impl<T> private::Sealed for *mut T where T: ?Sized {}
impl<T> private::Sealed for NonNull<T> where T: ?Sized {}

impl<T> Ptr for *const T where T: ?Sized {}
impl<T> Ptr for *mut T where T: ?Sized {}
impl<T> Ptr for NonNull<T> where T: ?Sized {}

impl<T> Debug for UnsafeSendablePtr<T>
where
    T: Debug,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<T> Pointer for UnsafeSendablePtr<T>
where
    T: Pointer,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Pointer::fmt(&self.0, f)
    }
}

// SAFETY: `UnsafeSendablePtr` only wraps a pointer type `T: Ptr`.
// It provides no safe access to the pointed-to memory, so sending the wrapper
// across threads cannot introduce data races by itself.
unsafe impl<T> Send for UnsafeSendablePtr<T> {}

// SAFETY: `UnsafeSendablePtr` only wraps a pointer type `T: Ptr`.
// It provides no safe access to the pointed-to memory, so sharing the wrapper
// across threads cannot introduce data races by itself.
unsafe impl<T> Sync for UnsafeSendablePtr<T> {}

#[cfg(test)]
mod tests {
    use alloc::format;
    use core::ptr::NonNull;

    use crate::UnsafeSendablePtr;

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_unsafe_ptr() {
        let mut value = 123;

        let ptr1 = &raw const value;
        let ptr2 = UnsafeSendablePtr::new(ptr1);
        assert_eq!(format!("{ptr1:?}"), format!("{ptr2:?}"));
        assert_eq!(format!("{ptr1:p}"), format!("{ptr2:p}"));

        // SAFETY: Safe to access from current thread.
        let ptr3 = unsafe { ptr2.into_inner() };
        assert_eq!(ptr1, ptr3);

        let ptr1 = &raw mut value;
        let ptr2 = UnsafeSendablePtr::new(ptr1);
        assert_eq!(format!("{ptr1:?}"), format!("{ptr2:?}"));
        assert_eq!(format!("{ptr1:p}"), format!("{ptr2:p}"));

        // SAFETY: Safe to access from current thread.
        let ptr3 = unsafe { ptr2.into_inner() };
        assert_eq!(ptr1, ptr3);

        let ptr1 = NonNull::new(&raw mut value).unwrap();
        let ptr2 = UnsafeSendablePtr::new(ptr1);
        assert_eq!(format!("{ptr1:?}"), format!("{ptr2:?}"));
        assert_eq!(format!("{ptr1:p}"), format!("{ptr2:p}"));

        // SAFETY: Safe to access from current thread.
        let ptr3 = unsafe { ptr2.into_inner() };
        assert_eq!(ptr1, ptr3);
    }
}
