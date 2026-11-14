use std::time::Instant;

use crate::{Enricher, EnricherFn, context};

/// An [`Enricher`] that appends [`SystemTime::now()`] to the context.
///
/// The captured time is the moment `try_enable` runs on the handler chain.
/// Placed before an offloader, it reflects the caller's time.
/// Placed after an offloader, it reflects the worker's time.
///
/// [`SystemTime::now()`]: std::time::SystemTime::now
pub type SystemTimeEnricher<H> = Enricher<H, SystemTimeEnricherFn>;

/// Factory for [`SystemTimeEnricher`].
///
/// Stateless: every call returns a fresh [`SystemTime::now()`].
///
/// [`SystemTime::now()`]: std::time::SystemTime::now
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Debug)]
pub struct SystemTimeEnricherFn;

impl EnricherFn for SystemTimeEnricherFn {
    type Output = std::time::SystemTime;

    #[inline]
    fn call(&self) -> Self::Output {
        std::time::SystemTime::now()
    }
}

/// An [`Enricher`] that appends [`MonotonicTime`] measured since the
/// factory's reference instant.
///
/// The value is a [`Duration`] from the reference instant to the moment
/// `try_enable` runs.
/// It is monotone with respect to the factory, but not comparable with values
/// produced by a different [`MonotonicEnricherFn`].
///
/// [`Duration`]: core::time::Duration
/// [`MonotonicTime`]: crate::context::MonotonicTime
pub type MonotonicEnricher<H> = Enricher<H, MonotonicEnricherFn>;

/// Factory for [`MonotonicEnricher`].
///
/// The reference instant is captured by [`MonotonicEnricherFn::new_since_now`]
/// and is fixed for the lifetime of the factory.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct MonotonicEnricherFn {
    start: Instant,
}

impl MonotonicEnricherFn {
    /// Creates a factory whose reference instant is the moment of the call.
    ///
    /// The produced [`MonotonicTime`] values are therefore elapsed durations
    /// since this call.
    ///
    /// [`MonotonicTime`]: crate::context::MonotonicTime
    #[inline]
    #[must_use]
    pub fn new_since_now() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl EnricherFn for MonotonicEnricherFn {
    type Output = context::MonotonicTime;

    #[inline]
    fn call(&self) -> Self::Output {
        context::MonotonicTime(Instant::now().saturating_duration_since(self.start))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(miri))]
    use crate::SystemTimeEnricherFn;
    use crate::{EnricherFn, MonotonicEnricherFn};

    #[cfg(not(miri))]
    #[test]
    fn test_system_time_enricher_fn() {
        let before = std::time::SystemTime::now();
        let time = SystemTimeEnricherFn.call();
        let after = std::time::SystemTime::now();
        assert!(before <= time && time <= after);
    }

    #[test]
    fn test_monotonic_enricher_fn() {
        let func = MonotonicEnricherFn::new_since_now();
        let first = func.call();
        let second = func.call();

        assert!(first.0 <= second.0);
    }
}
