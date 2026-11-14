//! Extension traits for heterogeneous argument tuples.
//!
//! This module provides type-filtered access to [`MixedArgs`] tuples and
//! `Any`-bounded marker traits for downcasting tuples to concrete types.

use core::any::Any;

use polog::{
    DebuggableMixedArgs, DebuggableMixedArgsBuilder, DisplayableMixedArgs,
    DisplayableMixedArgsBuilder, DynMixedArgs, MixedArgs,
};

/// Extension methods for heterogeneous argument tuples.
///
/// This trait is implemented for all [`MixedArgs`] types and adds type-aware
/// leaf access on top of the absolute-index access provided by [`MixedArgs`].
pub trait MixedArgsExt: MixedArgs {
    /// Returns whether the leaf at absolute `index` has concrete type `T`.
    ///
    /// Returns:
    /// - `Some(true)` if the leaf exists and its type is `T`,
    /// - `Some(false)` if the leaf exists but its type differs, and
    /// - `None` if `index` is out of bounds.
    #[inline]
    fn is_nth_arg_type<T>(&self, index: usize) -> Option<bool>
    where
        Self: 'static,
        T: 'static,
    {
        self.nth_arg_as_dyn_any(index)
            .map(<dyn Any + 'static>::is::<T>)
    }

    /// Attempts to downcast the leaf at absolute `index` to `&T`.
    ///
    /// Returns:
    /// - `Some(&T)` if the leaf at absolute `index` is of type `T`,
    /// - `None` if the leaf exists but its type differs, or if the `index` is
    ///   out of bounds.
    #[inline]
    fn try_nth_arg_as<T>(&self, index: usize) -> Option<&T>
    where
        Self: 'static,
        T: 'static,
    {
        self.nth_arg_as_dyn_any(index)
            .and_then(|value| value.downcast_ref())
    }

    /// Number of leaf arguments whose concrete type matches `T`, determined by
    /// [`core::any::TypeId`] comparison.
    #[inline]
    fn type_filtered_num_args<T>(&self) -> usize
    where
        Self: 'static,
        T: 'static,
    {
        (0..self.num_args())
            .filter(|&index| self.is_nth_arg_type::<T>(index).unwrap_or(false))
            .count()
    }

    /// Returns the `index`-th leaf whose concrete type matches `T`.
    ///
    /// The `index` counts only matching leaves.
    ///
    /// Returns:
    /// - `Some(&T)` if such a leaf exists.
    /// - `None` if there are fewer than `index + 1` matching leaves.
    #[inline]
    fn type_filtered_nth_arg<T>(&self, mut index: usize) -> Option<&T>
    where
        Self: 'static,
        T: 'static,
    {
        for raw_index in 0..self.num_args() {
            let Some(value) = self.try_nth_arg_as::<T>(raw_index) else {
                continue;
            };
            if let Some(new_index) = index.checked_sub(1) {
                index = new_index;
            } else {
                return Some(value);
            }
        }
        None
    }
}

impl<T> MixedArgsExt for T where T: ?Sized + MixedArgs {}

/// Combines [`Any`] and [`DynMixedArgs<T>`] so a heterogeneous tuple can be
/// downcast back to a concrete type when needed.
///
/// This is automatically implemented for all types that are `Any +
/// DynMixedArgs<T>`.
pub trait AnyDynMixedArgs<T>: Any + DynMixedArgs<T>
where
    T: ?Sized,
{
}

impl<T, U> AnyDynMixedArgs<U> for T
where
    T: Any + DynMixedArgs<U>,
    U: ?Sized,
{
}

/// Combines [`Any`] and [`DisplayableMixedArgs`] so a displayable
/// heterogeneous tuple can be downcast back to a concrete type when needed.
///
/// This is automatically implemented for all types that are
/// `Any + DisplayableMixedArgs<'a>`.
pub trait AnyDisplayableMixedArgs<'a>: Any + DisplayableMixedArgs<'a> {}

impl<'a, T> AnyDisplayableMixedArgs<'a> for T where T: ?Sized + Any + DisplayableMixedArgs<'a> {}

/// Combines [`Any`] and [`DisplayableMixedArgsBuilder`] so a displayable
/// argument tuple builder can be downcast back to a concrete type when needed.
///
/// This is automatically implemented for all types that are
/// `Any + DisplayableMixedArgsBuilder<'a>`.
pub trait AnyDisplayableMixedArgsBuilder<'a>: Any + DisplayableMixedArgsBuilder<'a> {}

impl<'a, T> AnyDisplayableMixedArgsBuilder<'a> for T where
    T: ?Sized + Any + DisplayableMixedArgsBuilder<'a>
{
}

/// Combines [`Any`] and [`DebuggableMixedArgs`] so a debuggable heterogeneous
/// tuple can be downcast back to a concrete type when needed.
///
/// This is automatically implemented for all types that are
/// `Any + DebuggableMixedArgs<'a>`.
pub trait AnyDebuggableMixedArgs<'a>: Any + DebuggableMixedArgs<'a> {}

impl<'a, T> AnyDebuggableMixedArgs<'a> for T where T: ?Sized + Any + DebuggableMixedArgs<'a> {}

/// Combines [`Any`] and [`DebuggableMixedArgsBuilder`] so a debuggable argument
/// tuple builder can be downcast back to a concrete type when needed.
///
/// This is automatically implemented for all types that are
/// `Any + DebuggableMixedArgsBuilder<'a>`.
pub trait AnyDebuggableMixedArgsBuilder<'a>: Any + DebuggableMixedArgsBuilder<'a> {}

impl<'a, T> AnyDebuggableMixedArgsBuilder<'a> for T where
    T: ?Sized + Any + DebuggableMixedArgsBuilder<'a>
{
}

#[cfg(feature = "alloc")]
#[cfg(test)]
mod tests {
    use core::fmt::Debug;

    use polog::Arg;

    use crate::args::MixedArgsExt;

    #[test]
    fn test_mixed_args_try_as() {
        #[track_caller]
        fn check<T, U>(data: &T, i: usize, expected: Option<&U>)
        where
            T: 'static + MixedArgsExt,
            U: 'static + Clone + PartialEq + Debug,
        {
            assert_eq!(data.try_nth_arg_as::<U>(i), expected);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check1<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.num_args(), 0);
            check::<_, u32>(&data, 0, None);
            check::<_, u32>(&data, 1, None);
            check::<_, u32>(&data, 2, None);
            check::<_, u32>(&data, usize::MAX, None);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check2<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.num_args(), 4);
            check::<_, u32>(&data, 0, Some(&12_u32));
            check::<_, i8>(&data, 0, None);
            check::<_, u32>(&data, 1, Some(&23_u32));
            check::<_, i8>(&data, 1, None);
            check::<_, u32>(&data, 2, Some(&34_u32));
            check::<_, i8>(&data, 2, None);
            check::<_, u32>(&data, 3, Some(&45_u32));
            check::<_, i8>(&data, 3, None);
            check::<_, u32>(&data, 4, None);
            check::<_, i8>(&data, 4, None);
            check::<_, u32>(&data, 5, None);
            check::<_, i8>(&data, 5, None);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check3<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.num_args(), 4);
            check::<_, u32>(&data, 1, None);
            check::<_, i64>(&data, 1, Some(&23_i64));
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check4<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.num_args(), 12);
            check::<_, u32>(&data, 0, Some(&12_u32));
            check::<_, bool>(&data, 0, None);
            check::<_, bool>(&data, 1, Some(&false));
            check::<_, u32>(&data, 1, None);
            check::<_, u32>(&data, 2, Some(&23_u32));
            check::<_, char>(&data, 2, None);
            check::<_, char>(&data, 3, Some(&'d'));
            check::<_, i32>(&data, 3, None);
            check::<_, i32>(&data, 4, Some(&34_i32));
        }

        let data = ();
        check1(data);

        let data = (Arg(12_u32), Arg(23_u32), Arg(34_u32), Arg(45_u32));
        check2(data);

        let data = (
            (Arg(12_i64), (), (((Arg(23_i64),), ()),)),
            ((Arg(34_i64), Arg(45_i64)),),
            ((),),
        );
        check3(data);

        let data = (
            (Arg(12_u32), Arg(false)),
            (Arg(23_u32), Arg('d')),
            (Arg(34_i32), Arg('c')),
            (Arg(45_i32), Arg(true)),
            ((Arg('b'), Arg('a')), (Arg(12.23_f32), Arg(23.34_f32))),
        );
        check4(data);
    }

    #[test]
    fn test_mixed_args_type_filtered() {
        #[track_caller]
        fn check<T, U>(data: &T, i: usize, expected: Option<&U>)
        where
            T: 'static + MixedArgsExt,
            U: 'static + Clone + PartialEq + Debug,
        {
            assert_eq!(data.type_filtered_nth_arg::<U>(i), expected);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check1<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.type_filtered_num_args::<u32>(), 0);
            check::<_, u32>(&data, 0, None);
            check::<_, u32>(&data, 1, None);
            check::<_, u32>(&data, 2, None);
            check::<_, u32>(&data, usize::MAX, None);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check2<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.type_filtered_num_args::<u32>(), 4);
            assert_eq!(data.type_filtered_num_args::<i8>(), 0);
            assert_eq!(data.type_filtered_num_args::<i128>(), 0);
            check::<_, u32>(&data, 0, Some(&12_u32));
            check::<_, u32>(&data, 1, Some(&23_u32));
            check::<_, u32>(&data, 2, Some(&34_u32));
            check::<_, u32>(&data, 3, Some(&45_u32));
            check::<_, u32>(&data, 4, None);
            check::<_, u32>(&data, 5, None);
            check::<_, u32>(&data, 6, None);
            check::<_, u32>(&data, usize::MAX, None);
            check::<_, i8>(&data, 0, None);
            check::<_, i128>(&data, 0, None);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check3<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.type_filtered_num_args::<i64>(), 4);
            assert_eq!(data.type_filtered_num_args::<i8>(), 0);
            assert_eq!(data.type_filtered_num_args::<i128>(), 0);
            check::<_, i64>(&data, 0, Some(&12_i64));
            check::<_, i64>(&data, 1, Some(&23_i64));
            check::<_, i64>(&data, 2, Some(&34_i64));
            check::<_, i64>(&data, 3, Some(&45_i64));
            check::<_, i64>(&data, 4, None);
            check::<_, i64>(&data, 5, None);
            check::<_, i64>(&data, 6, None);
            check::<_, i64>(&data, usize::MAX, None);
            check::<_, i8>(&data, 0, None);
            check::<_, i128>(&data, 0, None);
        }

        // A separate function with generic `T: MixedArgsBuilder` ensures the
        // test can only rely on the `MixedArgsBuilder` trait.
        fn check4<T>(data: T)
        where
            T: 'static + MixedArgsExt,
        {
            assert_eq!(data.type_filtered_num_args::<u32>(), 2);
            assert_eq!(data.type_filtered_num_args::<i32>(), 2);
            assert_eq!(data.type_filtered_num_args::<f32>(), 2);
            assert_eq!(data.type_filtered_num_args::<bool>(), 2);
            assert_eq!(data.type_filtered_num_args::<char>(), 4);
            check::<_, u32>(&data, 0, Some(&12_u32));
            check::<_, u32>(&data, 1, Some(&23_u32));
            check::<_, u32>(&data, 2, None);
            check::<_, i32>(&data, 0, Some(&34_i32));
            check::<_, i32>(&data, 1, Some(&45_i32));
            check::<_, i32>(&data, 2, None);
            check::<_, bool>(&data, 0, Some(&false));
            check::<_, bool>(&data, 1, Some(&true));
            check::<_, bool>(&data, 2, None);
            check::<_, char>(&data, 0, Some(&'d'));
            check::<_, char>(&data, 1, Some(&'c'));
            check::<_, char>(&data, 2, Some(&'b'));
            check::<_, char>(&data, 3, Some(&'a'));
            check::<_, char>(&data, 4, None);
            check::<_, f32>(&data, 0, Some(&12.23));
            check::<_, f32>(&data, 1, Some(&23.34));
            check::<_, f32>(&data, 2, None);
        }

        let data = ();
        check1(data);

        let data = (Arg(12_u32), Arg(23_u32), Arg(34_u32), Arg(45_u32));
        check2(data);

        let data = (
            (Arg(12_i64), (), (((Arg(23_i64),), ()),)),
            ((Arg(34_i64), Arg(45_i64)),),
            ((),),
        );
        check3(data);

        let data = (
            (Arg(12_u32), Arg(false)),
            (Arg(23_u32), Arg('d')),
            (Arg(34_i32), Arg('c')),
            (Arg(45_i32), Arg(true)),
            ((Arg('b'), Arg('a')), (Arg(12.23_f32), Arg(23.34_f32))),
        );
        check4(data);
    }
}
