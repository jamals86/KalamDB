/// Backend mode selected for the PostgreSQL extension build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendMode {
    Embedded,
    Remote,
}

impl BackendMode {
    /// Returns the active backend mode for the current build.
    pub fn current() -> Self {
        #[cfg(feature = "remote")]
        {
            return Self::Remote;
        }

        Self::Embedded
    }
}
