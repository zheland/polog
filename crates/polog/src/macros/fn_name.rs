#[doc(hidden)]
/// Strips module path from a function name.
///
/// This is an auxiliary function and is used in [`func_name!`] macro.
///
/// # Examples
///
/// ```rust
/// fn func() {
///     assert_eq!(
///         polog::build_fn_name(::core::module_path!(), &|arg: !| match arg {}),
/// #       "main::_doctest_main_crates_polog_src_macros_fn_name_rs_8_0::func", /*
///         "func",
/// #       */
///     );
/// }
///
/// func();
/// ```
///
/// [`func_name!`]: macro@crate::func_name
#[inline]
#[must_use]
pub fn build_fn_name<T>(module_path: &'static str, _: &T) -> &'static str
where
    T: ?Sized,
{
    let name = core::any::type_name::<T>();
    let name = name.strip_suffix("::Here").unwrap_or(name);
    let name = name.strip_suffix("::{{closure}}").unwrap_or(name);
    let name = name.strip_prefix(module_path).unwrap_or(name);
    name.strip_prefix("::").unwrap_or(name)
}

/// Returns the name of the surrounding function or method, without generics
/// instantiated.
/// Returns a `&'static str`.
///
/// # Note
///
/// This is intended for diagnostic use and the exact output is not guaranteed.
/// It provides a best-effort description, but the output may change between
/// versions of the compiler.
///
/// In short: use this for debugging, avoid using the output to affect program
/// behavior.
///
/// # Examples
///
/// ```
/// let current_function_name = polog::uninstantiated_fn_name!();
/// println!("defined in function: {current_function_name}");
/// ```
#[macro_export]
macro_rules! uninstantiated_fn_name {
    () => {{
        struct Here;
        $crate::build_fn_name(::core::module_path!(), &Here)
    }};
}

/// Returns the name of the surrounding function or method, including
/// instantiated generics.
/// Returns a `&'static str`.
///
/// # Note
///
/// This is intended for diagnostic use and the exact output is not guaranteed.
/// It provides a best-effort description, but the output may change between
/// versions of the compiler.
///
/// In short: use this for debugging, avoid using the output to affect program
/// behavior.
///
/// # Examples
///
/// ```
/// let current_function_name = polog::instantiated_fn_name!();
/// println!("defined in function: {current_function_name}");
/// ```
#[macro_export]
macro_rules! instantiated_fn_name {
    () => {
        $crate::build_fn_name(::core::module_path!(), &|arg: !| match arg {})
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_uninstantiated_fn_name() {
        fn foo() -> &'static str {
            uninstantiated_fn_name!()
        }

        struct Bar<T>(T);

        impl<T> Bar<T> {
            fn baz() -> &'static str {
                uninstantiated_fn_name!()
            }
        }

        assert_eq!(foo(), "test_uninstantiated_fn_name::foo");
        assert_eq!(
            Bar::<u32>::baz(),
            "test_uninstantiated_fn_name::Bar<_>::baz"
        );
    }

    #[test]
    fn test_instantiated_fn_name() {
        fn foo() -> &'static str {
            instantiated_fn_name!()
        }

        struct Bar<T>(T);

        impl<T> Bar<T> {
            fn baz() -> &'static str {
                instantiated_fn_name!()
            }
        }

        assert_eq!(foo(), "test_instantiated_fn_name::foo");
        assert_eq!(
            Bar::<u32>::baz(),
            "test_instantiated_fn_name::Bar<u32>::baz"
        );
    }
}
