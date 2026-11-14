//! Heterogeneous argument tuples with absolute and type-filtered access.
//!
//! Tuples are nested up to 12 elements per level and use [`Arg`] to mark leaf
//! values.

use core::any::{Any, TypeId};
use core::error::Error;
use core::fmt::{self, Debug, Display};

/// Trait for heterogeneous tuples that can be inspected by absolute index.
///
/// Implementors are [`Arg<T>`] leaf values and nested tuples of them (including
/// `()`).
/// All methods are designed to be monomorphised away when the tuple structure
/// is statically known.
///
/// The trait is implemented for nested tuples up to 12 elements and for
/// [`Arg<T>`] leaf values.
/// Rust does not support variadic tuples, so nested tuples up to 12 elements
/// are used as a reasonable compromise for common use cases and compile-time
/// performance.
pub trait MixedArgs {
    /// Total number of leaf [`Arg`] values in this (potentially nested) tuple.
    fn num_args(&self) -> usize;

    /// Returns type metadata (type name, size, alignment) for the leaf at
    /// absolute position `index`.
    ///
    /// Returns `None` when `index` is out of bounds.
    fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata>;

    /// Returns static type metadata (type name, size, alignment, and
    /// [`TypeId`]) for the leaf at absolute position `index`.
    /// Unlike [`MixedArgs::nth_arg_type_metadata`], this also includes the
    ///
    /// [`TypeId`], which allows later downcasting.
    ///
    /// Returns `None` when `index` is out of bounds.
    fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static;

    /// Returns the leaf at absolute `index` as `&dyn Any`.
    ///
    /// Returns `None` when `index` is out of bounds.
    fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
    where
        Self: 'static;
}

/// Builder for [`MixedArgs`] tuples.
///
/// Appends values to a heterogeneous argument tuple while preserving the
/// nesting scheme used to support more than 12 arguments.
pub trait MixedArgsBuilder: MixedArgs {
    /// The type of the heterogeneous tuple obtained by appending one more
    /// [`Arg<T>`] leaf to `Self`.
    ///
    /// For tuples with fewer than 12 elements at the outermost level, the new
    /// `Arg<T>` is appended as an additional member, so for
    /// `Self = (T1, T2, ..., Tn)` with `n < 12`,
    /// `Self::WithArg<T> = (T1, T2, ..., Tn, Arg<T>)`.
    ///
    /// When `Self` already has exactly 12 elements at the outermost level, a
    /// new nesting level is created: the resulting type is the 2-tuple
    /// `(Self, Arg<T>)`, which wraps the original 12 elements inside a nested
    /// group.
    type WithArg<T>: MixedArgsBuilder;

    /// Consumes `self` and returns a new tuple with the given value wrapped in
    /// [`Arg`] and appended to the argument list.
    ///
    /// The behaviour mirrors the [`WithArg`] associated type:
    /// - If the outermost tuple contains fewer than 12 elements, the new
    ///   element becomes an additional member of that tuple.
    /// - If the outermost tuple already contains exactly 12 elements, the whole
    ///   tuple is wrapped in a new 2-tuple `(self, Arg(arg))`, adding a nesting
    ///   layer. This is the mechanism by which argument lists longer than 12
    ///   elements are represented.
    ///
    /// [`WithArg`]: MixedArgsBuilder::WithArg
    fn with_arg<T>(self, arg: T) -> Self::WithArg<T>;
}

/// Access the `n`-th leaf of a heterogeneous tuple as `&T` or `&mut T`
/// (typically a trait object such as `&dyn Display`).
///
/// Where [`MixedArgs`] works with concrete types, `DynMixedArgs<T>` erases to
/// a single trait `T`. This is useful when the concrete tuple type is no longer
/// known but its behavior through `T` is.
///
/// Requires [`DynArg`] to be implemented for each leaf type and the target
/// trait object type (e.g.
/// `impl<'a, T> DynArg<dyn Display + 'a> for T where T: 'a + Display`).
pub trait DynMixedArgs<T>: MixedArgs
where
    T: ?Sized,
{
    /// Returns the `index`-th leaf as `&T`.
    ///
    /// Returns `None` when `index` is out of bounds.
    fn nth_dyn(&self, index: usize) -> Option<&T>;
}

/// Builder for [`DynMixedArgs`] tuples.
///
/// This is the type-erased counterpart of [`MixedArgsBuilder`]. It appends a
/// value `U` that can be viewed as `T` via [`DynArg<T>`].
pub trait DynMixedArgsBuilder<T>: MixedArgsBuilder + DynMixedArgs<T>
where
    T: ?Sized,
{
    /// The type obtained by appending one more [`Arg<U>`] leaf to `Self`.
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::WithArg`].
    type WithDynArg<U>: DynMixedArgsBuilder<T>
    where
        U: DynArg<T>;

    /// Consumes `self` and appends `arg` wrapped in [`Arg`].
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::with_arg`].
    fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
    where
        U: DynArg<T>;
}

/// Type-erased formatting of the `n`-th leaf via [`Display`].
///
/// This trait is implemented for tuples whose leaves can be viewed as
/// `dyn Display`.
pub trait DynDisplayableMixedArgs<'a>: DynMixedArgs<dyn 'a + Display> {
    /// Formats the `index`-th leaf into `writer` via [`Display`].
    ///
    /// Returns:
    /// - `Some(Ok(()))` if formatting succeeded.
    /// - `Some(Err(fmt::Error))` if the underlying [`Display`] implementation
    ///   failed.
    /// - `None` if `index` is out of bounds.
    fn dyn_display_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>>;
}

/// Monomorphised formatting of the `n`-th leaf via [`Display`].
///
/// This is the synchronous, statically dispatched counterpart of
/// [`DynDisplayableMixedArgs`]. No trait object is created.
/// The compiler inlines the concrete [`Display`] implementation for the leaf at
/// the known index.
pub trait DisplayableMixedArgs<'a>: DynDisplayableMixedArgs<'a> {
    /// Formats the `index`-th leaf into `writer` via [`Display`].
    ///
    /// Returns:
    /// - `Some(Ok(()))` if formatting succeeded.
    /// - `Some(Err(fmt::Error))` if the underlying [`Display`] implementation
    ///   failed.
    /// - `None` if `index` is out of bounds.
    fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write;
}

/// Builder for [`DisplayableMixedArgs`] tuples.
///
/// Appends values that implement [`Display`] and keeps both the type-erased and
/// monomorphised formatting paths available.
pub trait DisplayableMixedArgsBuilder<'a>:
    DynMixedArgsBuilder<dyn 'a + Display> + DisplayableMixedArgs<'a>
{
    /// The type obtained by appending one more [`Arg<T>`] leaf to `Self`.
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::WithArg`].
    type WithDisplayableArg<T>: DisplayableMixedArgsBuilder<'a>
    where
        T: 'a + Display;

    /// Consumes `self` and appends `arg` wrapped in [`Arg`].
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::with_arg`].
    fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
    where
        T: 'a + Display;
}

/// Type-erased formatting of the `n`-th leaf via [`Debug`].
///
/// This trait is implemented for tuples whose leaves can be viewed as
/// `dyn Debug`.
pub trait DynDebuggableMixedArgs<'a>: DynMixedArgs<dyn 'a + Debug> {
    /// Formats the `index`-th leaf into `writer` via [`Debug`].
    ///
    /// Returns:
    /// - `Some(Ok(()))` if formatting succeeded.
    /// - `Some(Err(fmt::Error))` if the underlying [`Debug`] implementation
    ///   failed.
    /// - `None` if `index` is out of bounds.
    fn dyn_debug_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>>;
}

/// Monomorphised formatting of the `n`-th leaf via [`Debug`].
///
/// This is the synchronous, statically dispatched counterpart of
/// [`DynDebuggableMixedArgs`].
/// No trait object is created.
/// The compiler inlines the concrete [`Debug`] implementation for the leaf at
/// the known index.
pub trait DebuggableMixedArgs<'a>: DynDebuggableMixedArgs<'a> {
    /// Formats the `index`-th leaf into `writer` via [`Debug`].
    ///
    /// Returns:
    /// - `Some(Ok(()))` if formatting succeeded.
    /// - `Some(Err(fmt::Error))` if the underlying [`Debug`] implementation
    ///   failed.
    /// - `None` if `index` is out of bounds.
    fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write;
}

/// Builder for [`DebuggableMixedArgs`] tuples.
///
/// Appends values that implement [`Debug`] and keeps both the type-erased and
/// monomorphised formatting paths available.
pub trait DebuggableMixedArgsBuilder<'a>:
    DynMixedArgsBuilder<dyn 'a + Debug> + DebuggableMixedArgs<'a>
{
    /// The type obtained by appending one more [`Arg<T>`] leaf to `Self`.
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::WithArg`].
    type WithDebuggableArg<T>: DebuggableMixedArgsBuilder<'a>
    where
        T: 'a + Debug;

    /// Consumes `self` and appends `arg` wrapped in [`Arg`].
    ///
    /// The nesting behavior matches [`MixedArgsBuilder::with_arg`].
    fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
    where
        T: 'a + Debug;
}

/// A newtype wrapper that marks a value as a leaf within a heterogeneous
/// argument tuple.
///
/// Wrapping every value in `Arg` prevents conflicting trait implementations
/// (e.g. a tuple `(i32, i32)` would collide with blanket impls for tuples) and
/// makes the boundary between a leaf value and a nested sub-tuple explicit.
///
/// `Arg<T>` implements the relevant argument traits (for example
/// [`MixedArgsBuilder`], [`DynMixedArgsBuilder`],
/// [`DisplayableMixedArgsBuilder`], and [`DebuggableMixedArgsBuilder`]) so that
/// a nested tuple like `(Arg(A), (Arg(B), ((),), Arg(C)))` behaves as a flat
/// list of three arguments.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
#[repr(transparent)]
pub struct Arg<T>(
    /// The wrapped value.
    pub T,
);

/// Metadata for a leaf argument's concrete type.
///
/// This includes the type name, size, and alignment, but not a [`TypeId`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct TypeMetadata {
    /// The type name as reported by [`core::any::type_name`].
    pub name: &'static str,
    /// The size of the type in bytes.
    pub size: usize,
    /// The alignment of the type in bytes.
    pub align: usize,
}

/// Static metadata for a leaf argument's concrete type.
///
/// This includes the type name, size, alignment, and [`TypeId`], allowing
/// callers to downcast a `&dyn Any` obtained via
/// [`MixedArgs::nth_arg_as_dyn_any`] back to the concrete leaf type.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct StaticTypeMetadata {
    /// The [`TypeId`] of the concrete type.
    pub id: TypeId,
    /// The type name as reported by [`core::any::type_name`].
    pub name: &'static str,
    /// The size of the type in bytes.
    pub size: usize,
    /// The alignment of the type in bytes.
    pub align: usize,
}

/// Enables a leaf value (inside [`Arg`]) to be viewed as `&T` or `&mut T`,
/// where `T` is typically a trait object such as `dyn Display`.
pub trait DynArg<T>
where
    T: ?Sized,
{
    /// Returns `self` as `&T`.
    fn as_dyn(&self) -> &T;
}

impl MixedArgs for () {
    #[inline]
    fn num_args(&self) -> usize {
        0
    }

    #[inline]
    fn nth_arg_type_metadata(&self, _index: usize) -> Option<TypeMetadata> {
        None
    }

    #[inline]
    fn nth_static_arg_type_metadata(&self, _index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static,
    {
        None
    }

    #[inline]
    fn nth_arg_as_dyn_any(&self, _index: usize) -> Option<&dyn Any>
    where
        Self: 'static,
    {
        None
    }
}

impl MixedArgsBuilder for () {
    type WithArg<T> = (Arg<T>,);

    #[inline]
    fn with_arg<T>(self, arg: T) -> Self::WithArg<T> {
        let () = self;
        (Arg(arg),)
    }
}

impl<T> DynMixedArgs<T> for ()
where
    T: ?Sized,
{
    #[inline]
    fn nth_dyn(&self, _index: usize) -> Option<&T> {
        None
    }
}

impl<T> DynMixedArgsBuilder<T> for ()
where
    T: ?Sized,
{
    type WithDynArg<U>
        = (Arg<U>,)
    where
        U: DynArg<T>;

    #[inline]
    fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
    where
        U: DynArg<T>,
    {
        let () = self;
        (Arg(arg),)
    }
}

impl DynDisplayableMixedArgs<'_> for () {
    #[inline]
    fn dyn_display_fmt_nth(
        &self,
        _writer: &mut dyn fmt::Write,
        _index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        None
    }
}

impl DisplayableMixedArgs<'_> for () {
    #[inline]
    fn display_fmt_nth<W>(&self, _writer: &mut W, _index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        None
    }
}

impl<'a> DisplayableMixedArgsBuilder<'a> for () {
    type WithDisplayableArg<T>
        = (Arg<T>,)
    where
        T: 'a + Display;

    #[inline]
    fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
    where
        T: 'a + Display,
    {
        let () = self;
        (Arg(arg),)
    }
}

impl DynDebuggableMixedArgs<'_> for () {
    #[inline]
    fn dyn_debug_fmt_nth(
        &self,
        _writer: &mut dyn fmt::Write,
        _index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        None
    }
}

impl DebuggableMixedArgs<'_> for () {
    #[inline]
    fn debug_fmt_nth<W>(&self, _writer: &mut W, _index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        None
    }
}

impl<'a> DebuggableMixedArgsBuilder<'a> for () {
    type WithDebuggableArg<T>
        = (Arg<T>,)
    where
        T: 'a + Debug;

    #[inline]
    fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
    where
        T: 'a + Debug,
    {
        let () = self;
        (Arg(arg),)
    }
}

impl<T1> MixedArgs for Arg<T1> {
    #[inline]
    fn num_args(&self) -> usize {
        1
    }

    #[inline]
    fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
        (index == 0).then(|| TypeMetadata {
            name: core::any::type_name::<T1>(),
            size: size_of::<T1>(),
            align: align_of::<T1>(),
        })
    }

    #[inline]
    fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static,
    {
        (index == 0).then(|| StaticTypeMetadata {
            id: TypeId::of::<T1>(),
            name: core::any::type_name::<T1>(),
            size: size_of::<T1>(),
            align: align_of::<T1>(),
        })
    }

    #[inline]
    fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
    where
        Self: 'static,
    {
        (index == 0).then_some(&self.0)
    }
}

impl<T1> MixedArgsBuilder for Arg<T1> {
    type WithArg<T> = (Self, Arg<T>);

    #[inline]
    fn with_arg<T>(self, arg: T) -> Self::WithArg<T> {
        let first = self;
        (first, Arg(arg))
    }
}

impl<T1, T> DynMixedArgs<T> for Arg<T1>
where
    T1: DynArg<T>,
    T: ?Sized,
{
    #[inline]
    fn nth_dyn(&self, index: usize) -> Option<&T> {
        (index == 0).then(|| self.0.as_dyn())
    }
}

impl<T1, T> DynMixedArgsBuilder<T> for Arg<T1>
where
    T1: DynArg<T>,
    T: ?Sized,
{
    type WithDynArg<U>
        = (Self, Arg<U>)
    where
        U: DynArg<T>;

    #[inline]
    fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
    where
        U: DynArg<T>,
    {
        let first = self;
        (first, Arg(arg))
    }
}

impl<'a, T1> DynDisplayableMixedArgs<'a> for Arg<T1>
where
    T1: 'a + Display,
{
    #[inline]
    fn dyn_display_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        (index == 0).then(|| write!(writer, "{}", self.0))
    }
}
impl<'a, T1> DisplayableMixedArgs<'a> for Arg<T1>
where
    T1: 'a + Display,
{
    #[inline]
    fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        (index == 0).then(|| write!(writer, "{}", self.0))
    }
}

impl<'a, T1> DisplayableMixedArgsBuilder<'a> for Arg<T1>
where
    T1: 'a + Display,
{
    type WithDisplayableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Display;

    #[inline]
    fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
    where
        T: 'a + Display,
    {
        let first = self;
        (first, Arg(arg))
    }
}

impl<'a, T1> DynDebuggableMixedArgs<'a> for Arg<T1>
where
    T1: 'a + Debug,
{
    #[inline]
    fn dyn_debug_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        (index == 0).then(|| write!(writer, "{:?}", self.0))
    }
}

impl<'a, T1> DebuggableMixedArgs<'a> for Arg<T1>
where
    T1: 'a + Debug,
{
    #[inline]
    fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        (index == 0).then(|| write!(writer, "{:?}", self.0))
    }
}

impl<'a, T1> DebuggableMixedArgsBuilder<'a> for Arg<T1>
where
    T1: 'a + Debug,
{
    type WithDebuggableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Debug;

    #[inline]
    fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
    where
        T: 'a + Debug,
    {
        let first = self;
        (first, Arg(arg))
    }
}

impl<Ts> MixedArgs for &Ts
where
    Ts: MixedArgs,
{
    #[inline]
    fn num_args(&self) -> usize {
        Ts::num_args(&**self)
    }

    #[inline]
    fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
        Ts::nth_arg_type_metadata(&**self, index)
    }

    #[inline]
    fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static,
    {
        Ts::nth_static_arg_type_metadata(&**self, index)
    }

    #[inline]
    fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
    where
        Self: 'static,
    {
        Ts::nth_arg_as_dyn_any(&**self, index)
    }
}

impl<Ts> MixedArgsBuilder for &Ts
where
    Ts: MixedArgsBuilder,
{
    type WithArg<T> = (Self, Arg<T>);

    #[inline]
    fn with_arg<T>(self, arg: T) -> Self::WithArg<T> {
        (self, Arg(arg))
    }
}

impl<Ts, T> DynMixedArgs<T> for &Ts
where
    Ts: DynMixedArgs<T>,
    T: ?Sized,
{
    #[inline]
    fn nth_dyn(&self, index: usize) -> Option<&T> {
        Ts::nth_dyn(&**self, index)
    }
}

impl<Ts, T> DynMixedArgsBuilder<T> for &Ts
where
    Ts: DynMixedArgsBuilder<T>,
    T: ?Sized,
{
    type WithDynArg<U>
        = (Self, Arg<U>)
    where
        U: DynArg<T>;

    #[inline]
    fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
    where
        U: DynArg<T>,
    {
        (self, Arg(arg))
    }
}

impl<'a, Ts> DynDisplayableMixedArgs<'a> for &Ts
where
    Ts: 'a + DynDisplayableMixedArgs<'a>,
{
    #[inline]
    fn dyn_display_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        Ts::dyn_display_fmt_nth(&**self, writer, index)
    }
}
impl<'a, Ts> DisplayableMixedArgs<'a> for &Ts
where
    Ts: 'a + DisplayableMixedArgs<'a>,
{
    #[inline]
    fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        Ts::display_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DisplayableMixedArgsBuilder<'a> for &Ts
where
    Ts: 'a + DisplayableMixedArgsBuilder<'a>,
{
    type WithDisplayableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Display;

    #[inline]
    fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
    where
        T: 'a + Display,
    {
        (self, Arg(arg))
    }
}

impl<'a, Ts> DynDebuggableMixedArgs<'a> for &Ts
where
    Ts: 'a + DynDebuggableMixedArgs<'a>,
{
    #[inline]
    fn dyn_debug_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        Ts::dyn_debug_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DebuggableMixedArgs<'a> for &Ts
where
    Ts: 'a + DebuggableMixedArgs<'a>,
{
    #[inline]
    fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        Ts::debug_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DebuggableMixedArgsBuilder<'a> for &Ts
where
    Ts: 'a + DebuggableMixedArgsBuilder<'a>,
{
    type WithDebuggableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Debug;

    #[inline]
    fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
    where
        T: 'a + Debug,
    {
        (self, Arg(arg))
    }
}

impl<Ts> MixedArgs for &mut Ts
where
    Ts: MixedArgs,
{
    #[inline]
    fn num_args(&self) -> usize {
        Ts::num_args(&**self)
    }

    #[inline]
    fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
        Ts::nth_arg_type_metadata(&**self, index)
    }

    #[inline]
    fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
    where
        Self: 'static,
    {
        Ts::nth_static_arg_type_metadata(&**self, index)
    }

    #[inline]
    fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
    where
        Self: 'static,
    {
        Ts::nth_arg_as_dyn_any(&**self, index)
    }
}

impl<Ts> MixedArgsBuilder for &mut Ts
where
    Ts: MixedArgsBuilder,
{
    type WithArg<T> = (Self, Arg<T>);

    #[inline]
    fn with_arg<T>(self, arg: T) -> Self::WithArg<T> {
        (self, Arg(arg))
    }
}

impl<Ts, T> DynMixedArgs<T> for &mut Ts
where
    Ts: DynMixedArgs<T>,
    T: ?Sized,
{
    #[inline]
    fn nth_dyn(&self, index: usize) -> Option<&T> {
        Ts::nth_dyn(&**self, index)
    }
}

impl<Ts, T> DynMixedArgsBuilder<T> for &mut Ts
where
    Ts: DynMixedArgsBuilder<T>,
    T: ?Sized,
{
    type WithDynArg<U>
        = (Self, Arg<U>)
    where
        U: DynArg<T>;

    #[inline]
    fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
    where
        U: DynArg<T>,
    {
        (self, Arg(arg))
    }
}

impl<'a, Ts> DynDisplayableMixedArgs<'a> for &mut Ts
where
    Ts: 'a + DynDisplayableMixedArgs<'a>,
{
    #[inline]
    fn dyn_display_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        Ts::dyn_display_fmt_nth(&**self, writer, index)
    }
}
impl<'a, Ts> DisplayableMixedArgs<'a> for &mut Ts
where
    Ts: 'a + DisplayableMixedArgs<'a>,
{
    #[inline]
    fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        Ts::display_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DisplayableMixedArgsBuilder<'a> for &mut Ts
where
    Ts: 'a + DisplayableMixedArgsBuilder<'a>,
{
    type WithDisplayableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Display;

    #[inline]
    fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
    where
        T: 'a + Display,
    {
        (self, Arg(arg))
    }
}

impl<'a, Ts> DynDebuggableMixedArgs<'a> for &mut Ts
where
    Ts: 'a + DynDebuggableMixedArgs<'a>,
{
    #[inline]
    fn dyn_debug_fmt_nth(
        &self,
        writer: &mut dyn fmt::Write,
        index: usize,
    ) -> Option<Result<(), fmt::Error>> {
        Ts::dyn_debug_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DebuggableMixedArgs<'a> for &mut Ts
where
    Ts: 'a + DebuggableMixedArgs<'a>,
{
    #[inline]
    fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
    where
        Self: Sized,
        W: fmt::Write,
    {
        Ts::debug_fmt_nth(&**self, writer, index)
    }
}

impl<'a, Ts> DebuggableMixedArgsBuilder<'a> for &mut Ts
where
    Ts: 'a + DebuggableMixedArgsBuilder<'a>,
{
    type WithDebuggableArg<T>
        = (Self, Arg<T>)
    where
        T: 'a + Debug;

    #[inline]
    fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
    where
        T: 'a + Debug,
    {
        (self, Arg(arg))
    }
}

macro_rules! impl_variadics {
    ( [ $( $args:tt )* ] [] ) => {};
    ( [ $( $args:tt )* ] [ $next:tt $( $rest:tt )* ] ) => {
        impl_variadic!( [ $( $args )* $next ] [ $( $rest )* ] );
        impl_variadics!( [ $( $args )* $next ] [ $( $rest )* ] );
    };
}

macro_rules! impl_variadic {
    ( [ $( ( $field:tt $type:ident ) )* ] [ $( $rest:tt )* ] ) => {
        impl<$( $type ),*> MixedArgs for ($( $type, )*)
        where
            $( $type: MixedArgs, )*
        {
            #[inline]
            fn num_args(&self) -> usize {
                0 $( + $type::num_args(&self.$field) )*
            }

            #[inline]
            fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::nth_arg_type_metadata(&self.$field, index);
                    };
                )*
                let _ = index;
                None
            }

            #[inline]
            fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
            where
                Self: 'static
            {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::nth_static_arg_type_metadata(&self.$field, index);
                    };
                )*
                let _ = index;
                None
            }

            #[inline]
            fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
            where
                Self: 'static,
            {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::nth_arg_as_dyn_any(&self.$field, index);
                    };
                )*
                let _ = index;
                None
            }
        }

        impl<$( $type ),*> MixedArgsBuilder for ($( $type, )*)
        where
            $( $type: MixedArgsBuilder, )*
        {
            type WithArg<U>
                = impl_variadic_with_arg_ty!( [ $( ( $field $type ) )* ] [ $( $rest )* ] U )
            where
                Self: Sized;

            #[inline]
            fn with_arg<U>(self, arg: U) -> Self::WithArg<U>
            where
                Self: Sized,
            {
                impl_variadic_with_arg_method!(
                    [ $( ( $field $type ) )* ] [ $( $rest )* ] self arg
                )
            }
        }

        impl<$( $type, )* T> DynMixedArgs<T> for ($( $type, )*)
        where
            $( $type: DynMixedArgs<T>, )*
            T: ?Sized,
        {
            #[inline]
            fn nth_dyn(&self, index: usize) -> Option<&T> {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::nth_dyn(&self.$field, index);
                    };
                )*
                let _ = index;
                None
            }
        }

        impl<$( $type ),*, T> DynMixedArgsBuilder<T> for ($( $type, )*)
        where
            $( $type: DynMixedArgsBuilder<T>, )*
            T: ?Sized,
        {
            type WithDynArg<U>
                = impl_variadic_with_arg_ty!( [ $( ( $field $type ) )* ] [ $( $rest )* ] U )
            where
                Self: Sized,
                U: DynArg<T>;

            #[inline]
            fn with_dyn_arg<U>(self, arg: U) -> Self::WithDynArg<U>
            where
                Self: Sized,
                U: DynArg<T>,
            {
                impl_variadic_with_arg_method!(
                    [ $( ( $field $type ) )* ] [ $( $rest )* ] self arg
                )
            }
        }

        impl<'a $(, $type )*> DynDisplayableMixedArgs<'a> for ($( $type, )*)
        where
            $( $type: DynDisplayableMixedArgs<'a>, )*
        {
            #[inline]
            fn dyn_display_fmt_nth(
                &self,
                writer: &mut dyn fmt::Write,
                index: usize,
            ) -> Option<Result<(), fmt::Error>> {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::dyn_display_fmt_nth(&self.$field, writer, index);
                    };
                )*
                let _ = index;
                None
            }
        }


        impl<'a $(, $type )*> DisplayableMixedArgs<'a> for ($( $type, )*)
        where
            $( $type: DisplayableMixedArgs<'a>, )*
        {
            #[inline]
            fn display_fmt_nth<W>(
                &self,
                writer: &mut W,
                index: usize,
            ) -> Option<Result<(), fmt::Error>>
            where
                Self: Sized,
                W: fmt::Write,
            {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::display_fmt_nth(&self.$field, writer, index);
                    };
                )*
                let _ = index;
                None
            }
        }

        impl<'a $(, $type )*> DisplayableMixedArgsBuilder<'a> for ($( $type, )*)
        where
            $( $type: DisplayableMixedArgsBuilder<'a>, )*
        {
            type WithDisplayableArg<T>
                = impl_variadic_with_arg_ty!( [ $( ( $field $type ) )* ] [ $( $rest )* ] T )
            where
                T: 'a + Display;

            #[inline]
            fn with_displayable_arg<T>(self, arg: T) -> Self::WithDisplayableArg<T>
            where
                T: 'a + Display,
            {
                impl_variadic_with_arg_method!(
                    [ $( ( $field $type ) )* ] [ $( $rest )* ] self arg
                )
            }
        }

        impl<'a $(, $type )*> DynDebuggableMixedArgs<'a> for ($( $type, )*)
        where
            $( $type: DynDebuggableMixedArgs<'a>, )*
        {
            #[inline]
            fn dyn_debug_fmt_nth(
                &self,
                writer: &mut dyn fmt::Write,
                index: usize,
            ) -> Option<Result<(), fmt::Error>> {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::dyn_debug_fmt_nth(&self.$field, writer, index);
                    };
                )*
                let _ = index;
                None
            }
        }

        impl<'a $(, $type )*> DebuggableMixedArgs<'a> for ($( $type, )*)
        where
            $( $type: DebuggableMixedArgs<'a>, )*
        {
            #[inline]
            fn debug_fmt_nth<W>(
                &self,
                writer: &mut W,
                index: usize,
            ) -> Option<Result<(), fmt::Error>>
            where
                Self: Sized,
                W: fmt::Write,
            {
                $(
                    let count = self.$field.num_args();
                    let Some(index) = index.checked_sub(count) else {
                        return $type::debug_fmt_nth(&self.$field, writer, index);
                    };
                )*
                let _ = index;
                None
            }
        }

        impl<'a $(, $type )*> DebuggableMixedArgsBuilder<'a> for ($( $type, )*)
        where
            $( $type: DebuggableMixedArgsBuilder<'a>, )*
        {
            type WithDebuggableArg<T>
                = impl_variadic_with_arg_ty!( [ $( ( $field $type ) )* ] [ $( $rest )* ] T )
            where
                T: 'a + Debug;

            #[inline]
            fn with_debuggable_arg<T>(self, arg: T) -> Self::WithDebuggableArg<T>
            where
                T: 'a + Debug,
            {
                impl_variadic_with_arg_method!(
                    [ $( ( $field $type ) )* ] [ $( $rest )* ] self arg
                )
            }
        }
    };
}

macro_rules! impl_variadic_with_arg_ty {
    ( [ $( ( $field:tt $type:ident ) )* ] [] $ty:ty ) => {
        ( ( $($type ),* ), Arg<$ty> )
    };
    ( [ $( ( $field:tt $type:ident ) )* ] [ $( $rest:tt )* ] $ty:ty ) => {
        ( $($type, )* Arg<$ty> )
    };
}

macro_rules! impl_variadic_with_arg_method {
    ( [ $( ( $field:tt $type:ident ) )* ] [] $self:tt $arg:tt ) => {
        ( ( $( $self.$field ),* ), Arg($arg) )
    };
    ( [ $( ( $field:tt $type:ident ) )* ] [ $( $rest:tt )* ] $self:tt $arg:tt ) => {
        ( $( $self.$field, )* Arg($arg) )
    };
}

// Generate impls for tuples of 1..=12 elements.
impl_variadics! {
    [] [
        (0 T1) (1 T2) (2 T3) (3 T4)
        (4 T5) (5 T6) (6 T7) (7 T8)
        (8 T9) (9 T10) (10 T11) (11 T12)
    ]
}

impl<'a, T> DynArg<dyn Display + 'a> for T
where
    T: 'a + Display,
{
    #[inline]
    fn as_dyn(&self) -> &(dyn Display + 'a) {
        self
    }
}

impl<'a, T> DynArg<dyn Debug + 'a> for T
where
    T: 'a + Debug,
{
    #[inline]
    fn as_dyn(&self) -> &(dyn Debug + 'a) {
        self
    }
}

impl<'a, T> DynArg<dyn Error + 'a> for T
where
    T: 'a + Error,
{
    #[inline]
    fn as_dyn(&self) -> &(dyn Error + 'a) {
        self
    }
}

/// Formats the wrapped value using its [`Display`] implementation.
///
/// `Arg` is transparent for formatting: the output is identical to formatting
/// the wrapped value directly.
/// This impl lets an [`Arg`] appear in user-written format expressions and in
/// helpers that accept the already-wrapped argument type.
impl<T> Display for Arg<T>
where
    T: Display,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Formats the wrapped value using its [`Debug`] implementation.
///
/// `Arg` is transparent for formatting: the output is identical to formatting
/// the wrapped value directly.
impl<T> Debug for Arg<T>
where
    T: Debug,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

#[cfg(feature = "alloc")]
mod alloc_impls {
    use alloc::boxed::Box;
    use alloc::sync::Arc;
    use core::alloc::Allocator;
    use core::any::Any;
    use core::fmt::{self, Debug, Display};

    use crate::{
        Arg, DebuggableMixedArgs, DebuggableMixedArgsBuilder, DisplayableMixedArgs,
        DisplayableMixedArgsBuilder, DynArg, DynDebuggableMixedArgs, DynDisplayableMixedArgs,
        DynMixedArgs, DynMixedArgsBuilder, MixedArgs, MixedArgsBuilder, StaticTypeMetadata,
        TypeMetadata,
    };

    impl<T, A: Allocator> MixedArgs for Box<T, A>
    where
        T: ?Sized + MixedArgs,
    {
        #[inline]
        fn num_args(&self) -> usize {
            T::num_args(&**self)
        }

        #[inline]
        fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
            T::nth_arg_type_metadata(&**self, index)
        }

        #[inline]
        fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
        where
            Self: 'static,
        {
            T::nth_static_arg_type_metadata(&**self, index)
        }

        #[inline]
        fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
        where
            Self: 'static,
        {
            T::nth_arg_as_dyn_any(&**self, index)
        }
    }

    impl<T, A: Allocator> MixedArgsBuilder for Box<T, A>
    where
        T: ?Sized + MixedArgsBuilder,
    {
        // Opaque on purpose.
        // `Box<T>` must also cover `T: ?Sized` (e.g. `Box<dyn MixedArgs>`),
        // and an unsized `T` cannot be moved out and re-boxed.
        type WithArg<U>
            = (Self, Arg<U>)
        where
            Self: Sized;

        #[inline]
        fn with_arg<U>(self, arg: U) -> Self::WithArg<U> {
            (self, Arg(arg))
        }
    }

    impl<T, U, A: Allocator> DynMixedArgs<U> for Box<T, A>
    where
        T: ?Sized + DynMixedArgs<U>,
        U: ?Sized,
    {
        #[inline]
        fn nth_dyn(&self, index: usize) -> Option<&U> {
            T::nth_dyn(&**self, index)
        }
    }

    impl<T, U, A: Allocator> DynMixedArgsBuilder<U> for Box<T, A>
    where
        T: ?Sized + DynMixedArgsBuilder<U>,
        U: ?Sized,
    {
        type WithDynArg<V>
            = (Self, Arg<V>)
        where
            V: DynArg<U>;

        #[inline]
        fn with_dyn_arg<V>(self, arg: V) -> Self::WithDynArg<V>
        where
            V: DynArg<U>,
        {
            (self, Arg(arg))
        }
    }

    impl<'a, T, A: Allocator> DynDisplayableMixedArgs<'a> for Box<T, A>
    where
        T: ?Sized + DynDisplayableMixedArgs<'a>,
    {
        #[inline]
        fn dyn_display_fmt_nth(
            &self,
            writer: &mut dyn fmt::Write,
            index: usize,
        ) -> Option<Result<(), fmt::Error>> {
            T::dyn_display_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T, A: Allocator> DisplayableMixedArgs<'a> for Box<T, A>
    where
        T: DisplayableMixedArgs<'a>,
    {
        #[inline]
        fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
        where
            Self: Sized,
            W: fmt::Write,
        {
            T::display_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T, A: Allocator> DisplayableMixedArgsBuilder<'a> for Box<T, A>
    where
        T: DisplayableMixedArgsBuilder<'a>,
    {
        type WithDisplayableArg<U>
            = (Self, Arg<U>)
        where
            U: 'a + Display;

        #[inline]
        fn with_displayable_arg<U>(self, arg: U) -> Self::WithDisplayableArg<U>
        where
            U: 'a + Display,
        {
            (self, Arg(arg))
        }
    }

    impl<'a, T, A: Allocator> DynDebuggableMixedArgs<'a> for Box<T, A>
    where
        T: ?Sized + DynDebuggableMixedArgs<'a>,
    {
        #[inline]
        fn dyn_debug_fmt_nth(
            &self,
            writer: &mut dyn fmt::Write,
            index: usize,
        ) -> Option<Result<(), fmt::Error>> {
            T::dyn_debug_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T, A: Allocator> DebuggableMixedArgs<'a> for Box<T, A>
    where
        T: DebuggableMixedArgs<'a>,
    {
        #[inline]
        fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
        where
            Self: Sized,
            W: fmt::Write,
        {
            T::debug_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T, A: Allocator> DebuggableMixedArgsBuilder<'a> for Box<T, A>
    where
        T: DebuggableMixedArgsBuilder<'a>,
    {
        type WithDebuggableArg<U>
            = (Self, Arg<U>)
        where
            U: 'a + Debug;

        #[inline]
        fn with_debuggable_arg<U>(self, arg: U) -> Self::WithDebuggableArg<U>
        where
            U: 'a + Debug,
        {
            (self, Arg(arg))
        }
    }

    impl<T> MixedArgs for Arc<T>
    where
        T: ?Sized + MixedArgs,
    {
        #[inline]
        fn num_args(&self) -> usize {
            T::num_args(&**self)
        }

        #[inline]
        fn nth_arg_type_metadata(&self, index: usize) -> Option<TypeMetadata> {
            T::nth_arg_type_metadata(&**self, index)
        }

        #[inline]
        fn nth_static_arg_type_metadata(&self, index: usize) -> Option<StaticTypeMetadata>
        where
            Self: 'static,
        {
            T::nth_static_arg_type_metadata(&**self, index)
        }

        #[inline]
        fn nth_arg_as_dyn_any(&self, index: usize) -> Option<&dyn Any>
        where
            Self: 'static,
        {
            T::nth_arg_as_dyn_any(&**self, index)
        }
    }

    impl<T> MixedArgsBuilder for Arc<T>
    where
        T: ?Sized + MixedArgsBuilder,
    {
        // Opaque on purpose.
        // `Arc<T>` must also cover `T: ?Sized` (e.g. `Arc<dyn MixedArgs>`),
        // and an unsized `T` cannot be moved out and re-Arced.
        type WithArg<U>
            = (Self, Arg<U>)
        where
            Self: Sized;

        #[inline]
        fn with_arg<U>(self, arg: U) -> Self::WithArg<U> {
            (self, Arg(arg))
        }
    }

    impl<T, U> DynMixedArgs<U> for Arc<T>
    where
        T: ?Sized + DynMixedArgs<U>,
        U: ?Sized,
    {
        #[inline]
        fn nth_dyn(&self, index: usize) -> Option<&U> {
            T::nth_dyn(&**self, index)
        }
    }

    impl<T, U> DynMixedArgsBuilder<U> for Arc<T>
    where
        T: ?Sized + DynMixedArgsBuilder<U>,
        U: ?Sized,
    {
        type WithDynArg<V>
            = (Self, Arg<V>)
        where
            V: DynArg<U>;

        #[inline]
        fn with_dyn_arg<V>(self, arg: V) -> Self::WithDynArg<V>
        where
            V: DynArg<U>,
        {
            (self, Arg(arg))
        }
    }

    impl<'a, T> DynDisplayableMixedArgs<'a> for Arc<T>
    where
        T: ?Sized + DynDisplayableMixedArgs<'a>,
    {
        #[inline]
        fn dyn_display_fmt_nth(
            &self,
            writer: &mut dyn fmt::Write,
            index: usize,
        ) -> Option<Result<(), fmt::Error>> {
            T::dyn_display_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T> DisplayableMixedArgs<'a> for Arc<T>
    where
        T: DisplayableMixedArgs<'a>,
    {
        #[inline]
        fn display_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
        where
            Self: Sized,
            W: fmt::Write,
        {
            T::display_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T> DisplayableMixedArgsBuilder<'a> for Arc<T>
    where
        T: DisplayableMixedArgsBuilder<'a>,
    {
        type WithDisplayableArg<U>
            = (Self, Arg<U>)
        where
            U: 'a + Display;

        #[inline]
        fn with_displayable_arg<U>(self, arg: U) -> Self::WithDisplayableArg<U>
        where
            U: 'a + Display,
        {
            (self, Arg(arg))
        }
    }

    impl<'a, T> DynDebuggableMixedArgs<'a> for Arc<T>
    where
        T: ?Sized + DynDebuggableMixedArgs<'a>,
    {
        #[inline]
        fn dyn_debug_fmt_nth(
            &self,
            writer: &mut dyn fmt::Write,
            index: usize,
        ) -> Option<Result<(), fmt::Error>> {
            T::dyn_debug_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T> DebuggableMixedArgs<'a> for Arc<T>
    where
        T: DebuggableMixedArgs<'a>,
    {
        #[inline]
        fn debug_fmt_nth<W>(&self, writer: &mut W, index: usize) -> Option<Result<(), fmt::Error>>
        where
            Self: Sized,
            W: fmt::Write,
        {
            T::debug_fmt_nth(&**self, writer, index)
        }
    }

    impl<'a, T> DebuggableMixedArgsBuilder<'a> for Arc<T>
    where
        T: DebuggableMixedArgsBuilder<'a>,
    {
        type WithDebuggableArg<U>
            = (Self, Arg<U>)
        where
            U: 'a + Debug;

        #[inline]
        fn with_debuggable_arg<U>(self, arg: U) -> Self::WithDebuggableArg<U>
        where
            U: 'a + Debug,
        {
            (self, Arg(arg))
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::format;
    use alloc::string::String;
    use alloc::sync::Arc;
    use core::any::TypeId;
    use core::error::Error;
    use core::fmt;
    #[cfg(feature = "std")]
    use std::sync::Mutex;

    use crate::{
        Arg, DebuggableMixedArgs, DebuggableMixedArgsBuilder, DisplayableMixedArgs,
        DisplayableMixedArgsBuilder, DynArg, DynDebuggableMixedArgs, DynDisplayableMixedArgs,
        DynMixedArgs, DynMixedArgsBuilder, MixedArgs, MixedArgsBuilder, StaticTypeMetadata,
        TypeMetadata,
    };

    #[test]
    fn test_mixed_args_unit() {
        let mut unused = String::new();

        assert_eq!(().num_args(), 0);
        assert_eq!(().nth_arg_type_metadata(0), None);
        assert_eq!(().nth_static_arg_type_metadata(0), None);
        assert!(().nth_arg_as_dyn_any(0).is_none());
        assert!(DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&(), 0).is_none());
        assert!(DynMixedArgs::<dyn fmt::Display>::nth_dyn(&(), 0).is_none());

        assert!(().dyn_debug_fmt_nth(&mut unused, 0).is_none());
        assert!(().dyn_display_fmt_nth(&mut unused, 0).is_none());
        assert!(().debug_fmt_nth(&mut unused, 0).is_none());
        assert!(().display_fmt_nth(&mut unused, 0).is_none());

        let args = &&mut &&mut ();

        assert_eq!(args.num_args(), 0);
        assert_eq!(args.nth_arg_type_metadata(0), None);
        assert_eq!((&&&&()).nth_static_arg_type_metadata(0), None);
        assert!((&&&&()).nth_arg_as_dyn_any(0).is_none());
        assert!(DynMixedArgs::<dyn fmt::Debug>::nth_dyn(args, 0).is_none());
        assert!(DynMixedArgs::<dyn fmt::Display>::nth_dyn(args, 0).is_none());

        assert!(args.dyn_debug_fmt_nth(&mut unused, 0).is_none());
        assert!(args.dyn_display_fmt_nth(&mut unused, 0).is_none());
        assert!(args.debug_fmt_nth(&mut unused, 0).is_none());
        assert!(args.display_fmt_nth(&mut unused, 0).is_none());

        #[cfg(feature = "std")]
        {
            static STATIC_LOCK: Mutex<()> = Mutex::new(());
            static mut STATIC_MUT: &&mut &&mut () = &&mut &&mut ();

            let _guard = STATIC_LOCK.lock();
            #[expect(static_mut_refs, reason = "intentionally tested")]
            // SAFETY: Access is by the `STATIC_LOCK`.
            unsafe {
                assert_eq!(STATIC_MUT.nth_static_arg_type_metadata(0), None);
                assert!(STATIC_MUT.nth_arg_as_dyn_any(0).is_none());
            }
        }
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_mixed_args_arg() {
        let mut debug = String::new();
        let mut display = String::new();

        let arg = Arg("abc");
        assert_eq!(format!("{arg}"), "abc");
        assert_eq!(format!("{arg:?}"), "\"abc\"");

        assert_eq!(arg.num_args(), 1);
        assert_eq!(
            arg.nth_arg_type_metadata(0),
            Some(TypeMetadata {
                name: "&str",
                size: size_of::<&str>(),
                align: align_of::<&str>()
            })
        );
        assert_eq!(
            arg.nth_static_arg_type_metadata(0),
            Some(StaticTypeMetadata {
                id: TypeId::of::<&str>(),
                name: "&str",
                size: size_of::<&str>(),
                align: align_of::<&str>()
            })
        );
        assert!(arg.nth_arg_as_dyn_any(0).is_some());
        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&arg, 0).unwrap();
        assert_eq!(format!("{fmt:?}"), "\"abc\"");
        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(&arg, 0).unwrap();
        assert_eq!(format!("{fmt}"), "abc");

        arg.dyn_debug_fmt_nth(&mut debug, 0).unwrap().unwrap();
        arg.dyn_display_fmt_nth(&mut display, 0).unwrap().unwrap();
        arg.debug_fmt_nth(&mut debug, 0).unwrap().unwrap();
        arg.display_fmt_nth(&mut display, 0).unwrap().unwrap();

        let arg = &&mut &&mut Arg("abc");

        assert_eq!(arg.num_args(), 1);
        assert_eq!(
            (&&&&Arg("abc")).nth_arg_type_metadata(0),
            Some(TypeMetadata {
                name: "&str",
                size: size_of::<&str>(),
                align: align_of::<&str>()
            })
        );
        assert_eq!(
            (&&&&Arg("abc")).nth_static_arg_type_metadata(0),
            Some(StaticTypeMetadata {
                id: TypeId::of::<&str>(),
                name: "&str",
                size: size_of::<&str>(),
                align: align_of::<&str>()
            })
        );
        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(arg, 0).unwrap();
        assert_eq!(format!("{fmt:?}"), "\"abc\"");
        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(arg, 0).unwrap();
        assert_eq!(format!("{fmt}"), "abc");

        arg.dyn_debug_fmt_nth(&mut debug, 0).unwrap().unwrap();
        arg.dyn_display_fmt_nth(&mut display, 0).unwrap().unwrap();
        arg.debug_fmt_nth(&mut debug, 0).unwrap().unwrap();
        arg.display_fmt_nth(&mut display, 0).unwrap().unwrap();

        assert_eq!(debug, "\"abc\"\"abc\"\"abc\"\"abc\"");
        assert_eq!(display, "abcabcabcabc");
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_mixed_args_nested_tuple() {
        let args = (
            Arg(true),
            (
                (Arc::new(Box::new(Arg(123))), (&&Arg('b'),)),
                Arg("abc"),
                (Arc::new(Box::new(())), ()),
            ),
            (),
            Arc::new(Arg((123, 234))),
        );

        assert_eq!(args.num_args(), 5);

        assert_eq!(
            args.nth_arg_type_metadata(0),
            Some(TypeMetadata {
                name: "bool",
                size: 1,
                align: 1,
            })
        );
        assert!(matches!(
            args.nth_arg_type_metadata(1),
            Some(TypeMetadata {
                size: 4,
                align: 4,
                ..
            })
        ));
        assert_eq!(
            args.nth_arg_type_metadata(2),
            Some(TypeMetadata {
                name: "char",
                size: 4,
                align: 4,
            })
        );
        assert!(matches!(
            args.nth_arg_type_metadata(4),
            Some(TypeMetadata {
                name: _,
                size: 8,
                align: 4
            })
        ));
        assert_eq!(args.nth_arg_type_metadata(5), None);

        assert_eq!(
            args.nth_static_arg_type_metadata(0),
            Some(StaticTypeMetadata {
                id: TypeId::of::<bool>(),
                name: "bool",
                size: 1,
                align: 1,
            })
        );
        assert_eq!(
            args.nth_static_arg_type_metadata(1),
            Some(StaticTypeMetadata {
                id: TypeId::of::<i32>(),
                name: "i32",
                size: 4,
                align: 4,
            })
        );
        assert_eq!(
            args.nth_static_arg_type_metadata(2),
            Some(StaticTypeMetadata {
                id: TypeId::of::<char>(),
                name: "char",
                size: 4,
                align: 4,
            })
        );
        assert_eq!(args.nth_static_arg_type_metadata(5), None);

        assert_eq!(
            args.nth_arg_as_dyn_any(0).unwrap().downcast_ref(),
            Option::<&bool>::Some(&true)
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(0).unwrap().downcast_ref(),
            Option::<&i32>::None
        );

        assert_eq!(
            args.nth_arg_as_dyn_any(1).unwrap().downcast_ref(),
            Option::<&i32>::Some(&123)
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(1).unwrap().downcast_ref(),
            Option::<&bool>::None
        );

        assert_eq!(
            args.nth_arg_as_dyn_any(4).unwrap().downcast_ref(),
            Option::<&(i32, i32)>::Some(&(123, 234))
        );
        assert_eq!(
            args.nth_arg_as_dyn_any(4).unwrap().downcast_ref(),
            Option::<&i32>::None
        );

        assert!(args.nth_arg_as_dyn_any(5).is_none());
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_mixed_args_nested_tuple_fmt() {
        let debug_args = (
            Arg(true),
            (
                (Arc::new(Box::new(Arg(123))), (&&Arg('b'),)),
                Arg("abc"),
                (Arc::new(Box::new(())), ()),
            ),
            (),
            Arc::new(Arg((123, 234))),
        );

        let display_args = (
            Arg(true),
            (
                (Arc::new(Box::new(Arg(123))), (&&Arg('b'),)),
                Arg("abc"),
                (Arc::new(Box::new(())), ()),
            ),
            (),
            Arc::new(Arg(12345)),
        );

        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&debug_args, 0).unwrap();
        assert_eq!(format!("{fmt:?}"), "true");
        let fmt = DynMixedArgs::<dyn fmt::Debug>::nth_dyn(&debug_args, 1).unwrap();
        assert_eq!(format!("{fmt:?}"), "123");

        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(&display_args, 0).unwrap();
        assert_eq!(format!("{fmt}"), "true");
        let fmt = DynMixedArgs::<dyn fmt::Display>::nth_dyn(&display_args, 1).unwrap();
        assert_eq!(format!("{fmt}"), "123");

        let mut debug = String::new();
        let mut display = String::new();
        for i in 0..5 {
            debug_args.debug_fmt_nth(&mut debug, i).unwrap().unwrap();
            display_args
                .display_fmt_nth(&mut display, i)
                .unwrap()
                .unwrap();
        }
        assert!(debug_args.debug_fmt_nth(&mut debug, 5).is_none());
        assert!(display_args.display_fmt_nth(&mut display, 5).is_none());

        assert_eq!(debug, "true123'b'\"abc\"(123, 234)");
        assert_eq!(display, "true123babc12345");

        let mut debug = String::new();
        let mut display = String::new();
        for i in 0..5 {
            debug_args
                .dyn_debug_fmt_nth(&mut debug, i)
                .unwrap()
                .unwrap();
            display_args
                .dyn_display_fmt_nth(&mut display, i)
                .unwrap()
                .unwrap();
        }
        assert!(debug_args.dyn_debug_fmt_nth(&mut debug, 5).is_none());
        assert!(display_args.dyn_display_fmt_nth(&mut display, 5).is_none());

        assert_eq!(debug, "true123'b'\"abc\"(123, 234)");
        assert_eq!(display, "true123babc12345");
    }

    #[test]
    fn test_mixed_args_as_dyn() {
        let fmt_error = fmt::Error;

        let display: &dyn fmt::Display = DynArg::<dyn fmt::Display>::as_dyn(&fmt_error);
        let debug: &dyn fmt::Debug = DynArg::<dyn fmt::Debug>::as_dyn(&fmt_error);
        let error: &dyn Error = DynArg::<dyn Error>::as_dyn(&fmt_error);

        assert_eq!(
            format!("{display}"),
            "an error occurred when formatting an argument"
        );
        assert_eq!(format!("{debug:?}"), "Error");

        assert_eq!(
            format!("{error}"),
            "an error occurred when formatting an argument"
        );
        assert_eq!(format!("{error:?}"), "Error");
    }

    #[rustfmt::skip] // bad formatting
    #[test]
    fn test_mixed_args_with_arg() {
        use DynMixedArgsBuilder as B;

        let args0 = ();
        // `with_arg` for tuple with less than 12 elements will append new value
        // at the new last tuple argument.
        let args1 = (Arg(1),);
        let args2 = (Arg(1), Arg(2));
        let args3 = (Arg(1), Arg(2), Arg(3));
        let args4 = (Arg(1), Arg(2), Arg(3), Arg(4));
        let args5 = (Arg(1), Arg(2), Arg(3), Arg(4), Arg(5));
        let args6 = (Arg(1), Arg(2), Arg(3), Arg(4), Arg(5), Arg(6));
        let args7 = (Arg(1), Arg(2), Arg(3), Arg(4), Arg(5), Arg(6), Arg(7));
        let args8 = (
            Arg(1), Arg(2), Arg(3), Arg(4),
            Arg(5), Arg(6), Arg(7), Arg(8),
        );
        let args9 = (
            Arg(1), Arg(2), Arg(3), Arg(4),
            Arg(5), Arg(6), Arg(7), Arg(8),
            Arg(9)
        );
        let args10 = (
            Arg(1), Arg(2), Arg(3), Arg(4),
            Arg(5), Arg(6), Arg(7), Arg(8),
            Arg(9), Arg(10)
        );
        let args11 = (
            Arg(1), Arg(2), Arg(3), Arg(4),
            Arg(5), Arg(6), Arg(7), Arg(8),
            Arg(9), Arg(10), Arg(11)
        );
        let args12 = (
            Arg(1), Arg(2), Arg(3), Arg(4),
            Arg(5), Arg(6), Arg(7), Arg(8),
            Arg(9), Arg(10), Arg(11), Arg(12),
        );
        // `with_arg` for tuple with 12 elements will make a new tuple of
        // 12 elements subtuple and new element.
        let args13 = (
            (
                Arg(1), Arg(2), Arg(3), Arg(4),
                Arg(5), Arg(6), Arg(7), Arg(8),
                Arg(9), Arg(10), Arg(11), Arg(12),
            ),
            Arg(13),
        );
        let args14 = (
            (
                Arg(1), Arg(2), Arg(3), Arg(4),
                Arg(5), Arg(6), Arg(7), Arg(8),
                Arg(9), Arg(10), Arg(11), Arg(12),
            ),
            Arg(13), Arg(14),
        );

        assert_eq!(args0.with_arg(1), args1);
        assert_eq!(args1.with_arg(2), args2);
        assert_eq!(args2.with_arg(3), args3);
        assert_eq!(args3.with_arg(4), args4);
        assert_eq!(args4.with_arg(5), args5);
        assert_eq!(args5.with_arg(6), args6);
        assert_eq!(args6.with_arg(7), args7);
        assert_eq!(args7.with_arg(8), args8);
        assert_eq!(args8.with_arg(9), args9);
        assert_eq!(args9.with_arg(10), args10);
        assert_eq!(args10.with_arg(11), args11);
        assert_eq!(args11.with_arg(12), args12);
        assert_eq!(args12.with_arg(13), args13);
        assert_eq!(args13.with_arg(14), args14);

        let args12_extra_nested = (
            (Arg(1), Arg(2), Arg(3), Arg(4)),
            (Arg(5), Arg(6), Arg(7), Arg(8)),
            (Arg(9), Arg(10), Arg(11), Arg(12)),
        );
        let args13_extra_nested = (
            (Arg(1), Arg(2), Arg(3), Arg(4)),
            (Arg(5), Arg(6), Arg(7), Arg(8)),
            (Arg(9), Arg(10), Arg(11), Arg(12)),
            Arg(13),
        );

        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(args0, 1), args1);
        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(args1, 2), args2);
        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(args2, 3), args3);
        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(args12, 13), args13);
        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(args13, 14), args14);

        assert_eq!(args0.with_displayable_arg(1), args1);
        assert_eq!(args1.with_displayable_arg(2), args2);
        assert_eq!(args2.with_displayable_arg(3), args3);
        assert_eq!(args13.with_displayable_arg(14), args14);
        assert_eq!(
            args12_extra_nested.with_displayable_arg(13),
            args13_extra_nested
        );

        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(args0, 1), args1);
        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(args1, 2), args2);
        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(args2, 3), args3);
        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(args12, 13), args13);
        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(args13, 14), args14);

        assert_eq!(args0.with_debuggable_arg(1), args1);
        assert_eq!(args1.with_debuggable_arg(2), args2);
        assert_eq!(args2.with_debuggable_arg(3), args3);
        assert_eq!(args13.with_debuggable_arg(14), args14);
        assert_eq!(
            args12_extra_nested.with_debuggable_arg(13),
            args13_extra_nested
        );
    }

    #[test]
    fn test_mixed_args_with_arg_custom() {
        use DynMixedArgsBuilder as B;

        let args2 = (Arg(1), Arg(2));

        assert_eq!(Arg(1).with_arg(2), args2);
        assert_eq!(B::<dyn fmt::Display>::with_dyn_arg(Arg(1), 2), args2);
        assert_eq!(Arg(1).with_displayable_arg(2), args2);
        assert_eq!(B::<dyn fmt::Debug>::with_dyn_arg(Arg(1), 2), args2);
        assert_eq!(Arg(1).with_debuggable_arg(2), args2);

        assert_eq!((&&args2).with_arg(3), (&&args2, Arg(3)));
        assert_eq!((&mut &args2).with_arg(3), (&mut &args2, Arg(3)));
        assert_eq!((Box::new(&args2)).with_arg(3), (Box::new(&args2), Arg(3)));
        assert_eq!((Arc::new(&args2)).with_arg(3), (Arc::new(&args2), Arg(3)));

        assert_eq!(
            B::<dyn fmt::Display>::with_dyn_arg(&&args2, 3),
            (&&args2, Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Display>::with_dyn_arg(&mut &args2, 3),
            (&mut &args2, Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Display>::with_dyn_arg(Box::new(&args2), 3),
            (Box::new(&args2), Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Display>::with_dyn_arg(Arc::new(&args2), 3),
            (Arc::new(&args2), Arg(3))
        );

        assert_eq!((&&args2).with_displayable_arg(3), (&&args2, Arg(3)));
        assert_eq!((&mut &args2).with_displayable_arg(3), (&mut &args2, Arg(3)));
        assert_eq!(
            (Box::new(&args2)).with_displayable_arg(3),
            (Box::new(&args2), Arg(3))
        );
        assert_eq!(
            (Arc::new(&args2)).with_displayable_arg(3),
            (Arc::new(&args2), Arg(3))
        );

        assert_eq!(
            B::<dyn fmt::Debug>::with_dyn_arg(&&args2, 3),
            (&&args2, Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Debug>::with_dyn_arg(&mut &args2, 3),
            (&mut &args2, Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Debug>::with_dyn_arg(Box::new(&args2), 3),
            (Box::new(&args2), Arg(3))
        );
        assert_eq!(
            B::<dyn fmt::Debug>::with_dyn_arg(Arc::new(&args2), 3),
            (Arc::new(&args2), Arg(3))
        );

        assert_eq!((&&args2).with_debuggable_arg(3), (&&args2, Arg(3)));
        assert_eq!((&mut &args2).with_debuggable_arg(3), (&mut &args2, Arg(3)));
        assert_eq!(
            (Box::new(&args2)).with_debuggable_arg(3),
            (Box::new(&args2), Arg(3))
        );
        assert_eq!(
            (Arc::new(&args2)).with_debuggable_arg(3),
            (Arc::new(&args2), Arg(3))
        );
    }

    #[expect(clippy::unwrap_used, reason = "okay in tests")]
    #[test]
    fn test_mixed_args_dyn() {
        pub trait SaturatedToU8 {
            fn saturated_to_u8(&self) -> u8;
        }

        impl<T> SaturatedToU8 for T
        where
            T: Copy + PartialOrd + Default + TryInto<u8>,
        {
            fn saturated_to_u8(&self) -> u8 {
                let is_positive = self >= &T::default();
                (*self)
                    .try_into()
                    .unwrap_or(if is_positive { u8::MAX } else { u8::MIN })
            }
        }

        impl<'a, T> DynArg<dyn SaturatedToU8 + 'a> for T
        where
            T: 'a + SaturatedToU8,
        {
            #[inline]
            fn as_dyn(&self) -> &(dyn SaturatedToU8 + 'a) {
                self
            }
        }

        // A separate function with generic `T: DynMixedArgsBuilder<dyn
        // Display>` ensures the test can only rely on the
        // `DynMixedArgsBuilder<dyn Display>` trait.
        fn check_dyn_display<T>(data: &T)
        where
            T: DynMixedArgs<dyn fmt::Display>,
        {
            use core::fmt::Write;

            let mut string = String::new();
            write!(&mut string, "{}", data.nth_dyn(0).unwrap()).unwrap();
            write!(&mut string, "{}", data.nth_dyn(1).unwrap()).unwrap();
            write!(&mut string, "{}", data.nth_dyn(2).unwrap()).unwrap();
            write!(&mut string, "{}", data.nth_dyn(3).unwrap()).unwrap();
            assert!(data.nth_dyn(4).is_none());
            assert!(data.nth_dyn(5).is_none());
            assert_eq!(string.as_str(), "12trueaHello World");
        }

        // A separate function with generic `T: DynMixedArgsBuilder<dyn
        // Display>` ensures the test can only rely on the
        // `DynMixedArgsBuilder<dyn Display>` trait.
        fn check_static_display<'a, T>(data: &T)
        where
            T: DisplayableMixedArgs<'a>,
        {
            let mut string = String::new();
            data.display_fmt_nth(&mut string, 0).unwrap().unwrap();
            data.display_fmt_nth(&mut string, 1).unwrap().unwrap();
            data.display_fmt_nth(&mut string, 2).unwrap().unwrap();
            data.display_fmt_nth(&mut string, 3).unwrap().unwrap();
            assert_eq!(data.display_fmt_nth(&mut string, 4), None);
            assert_eq!(data.display_fmt_nth(&mut string, 5), None);
            assert_eq!(string.as_str(), "12trueaHello World");
        }

        fn check_custom<T>(data: &T)
        where
            T: DynMixedArgs<dyn SaturatedToU8>,
        {
            assert_eq!(data.nth_dyn(0).unwrap().saturated_to_u8(), 12);
            assert_eq!(data.nth_dyn(1).unwrap().saturated_to_u8(), 1);
            assert_eq!(data.nth_dyn(2).unwrap().saturated_to_u8(), 0);
            assert_eq!(data.nth_dyn(3).unwrap().saturated_to_u8(), 255);
            assert!(data.nth_dyn(4).is_none());
            assert!(data.nth_dyn(5).is_none());
        }

        let data = (
            (Arg(12_i64), (), (((Arg(true),), ()),)),
            ((Arg('a'), Arg("Hello World")),),
            ((),),
        );
        check_dyn_display(&data);
        check_static_display(&data);

        let data = (
            (Arg(12_i64), (), (((Arg(true),), ()),)),
            ((Arg(-32_i32), Arg(123_456_789_u64)),),
            ((),),
        );
        check_custom(&data);
    }
}
