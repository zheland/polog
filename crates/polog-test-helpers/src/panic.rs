use core::any::Any;
use std::sync::Arc;

/// # Errors
///
/// Returns an error if the thread panicked.
#[expect(clippy::unwrap_used, reason = "okay in tests")]
#[inline]
pub fn catch_unwind_on_thread<L, F, T>(logger: &L, func: F) -> Result<T, String>
where
    L: 'static + Send + polog::Logger + Clone,
    F: 'static + Send + FnOnce(L) -> T,
    T: 'static + Send,
{
    let logger = logger.clone();
    let handle = std::thread::spawn(move || func(logger));
    handle
        .join()
        .map_err(|err| try_any_as_str(&*err).unwrap().to_owned())
}

#[expect(clippy::option_if_let_else, reason = "better readability")]
#[inline]
pub fn try_any_as_str(any: &(dyn Any + Send)) -> Option<&str> {
    if let Some(any) = any.downcast_ref::<&str>() {
        Some(*any)
    } else if let Some(any) = any.downcast_ref::<Box<str>>() {
        Some(any)
    } else if let Some(any) = any.downcast_ref::<Arc<str>>() {
        Some(any)
    } else if let Some(any) = any.downcast_ref::<String>() {
        Some(any.as_str())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::panic::try_any_as_str;

    #[test]
    fn test_try_any_as_str() {
        assert_eq!(try_any_as_str(&"abc"), Some("abc"));
        assert_eq!(try_any_as_str(&Box::<str>::from("abc")), Some("abc"));
        assert_eq!(try_any_as_str(&Arc::<str>::from("abc")), Some("abc"));
        assert_eq!(try_any_as_str(&String::from("abc")), Some("abc"));
        assert_eq!(try_any_as_str(&Box::new(String::from("abc"))), None);
        assert_eq!(try_any_as_str(&123), None);
    }
}
