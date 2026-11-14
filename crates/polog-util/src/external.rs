use polog::Level;

/// A description of an event that originates outside `polog`.
///
/// All fields except `level` are `Option`, because foreign facades differ
/// in which metadata they carry:
///
/// - `log` provides `target`, `module_path`, `file`, and `line`, but no `name`
///   or `column`.
/// - `slog` provides `module_path`, `file`, `line`, and `column`, but no `name`
///   and no target.
/// - `tracing` provides all fields except `column`.
///
/// A filter that needs to match on a field the foreign facade does not
/// supply should treat the missing value as "does not match".
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ExternalDescriptor<'a> {
    /// Event or span name, if the foreign facade has one.
    ///
    /// `tracing` supplies a name for every span and every event.
    /// `log` and `slog` have no name concept.
    pub name: Option<&'a str>,

    /// Event level, mapped to `polog::Level` by the ingesting backend.
    pub level: Level,

    /// Event target, if the foreign facade has one.
    ///
    /// `log` and `tracing` set a target on every event.
    /// `slog` has no target concept.
    pub target: Option<&'a str>,

    /// Module path of the emitting call site, if the foreign facade has one.
    ///
    /// `log` and `slog` always provide a module path.
    /// `tracing` may provide a module path.
    pub module_path: Option<&'a str>,

    /// Source file of the emitting call site, if the foreign facade has one.
    pub file: Option<&'a str>,

    /// Source line of the emitting call site, if the foreign facade has one.
    pub line: Option<u32>,

    /// Source column of the emitting call site, if the foreign facade has one.
    pub column: Option<u32>,
}
