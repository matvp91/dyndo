#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DashOptions {
    pub compact: bool,
    /// Split the manifest into a Period at each segment boundary.
    ///
    /// Off by default: a boundary only asks for a segment to start there, which
    /// says nothing about whether a client should treat what follows as a
    /// separate presentation.
    pub multi_period: bool,
}
