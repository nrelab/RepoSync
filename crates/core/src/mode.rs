use serde::{Deserialize, Serialize};

/// The mode of a file entry, mirroring the four git tree modes that RepoSync
/// cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMode {
    /// A regular non-executable file (git mode `100644`).
    File,
    /// An executable file (git mode `100755`).
    Executable,
    /// A symbolic link (git mode `120000`).
    Symlink,
    /// A gitlink/submodule reference (git mode `160000`).
    Gitlink,
}

impl FileMode {
    /// Git mode for a regular file.
    pub const FILE: u32 = 0o100_644;
    /// Git mode for an executable file.
    pub const EXECUTABLE: u32 = 0o100_755;
    /// Git mode for a symbolic link.
    pub const SYMLINK: u32 = 0o120_000;
    /// Git mode for a gitlink (submodule).
    pub const GITLINK: u32 = 0o160_000;

    /// Converts the mode to its git mode integer.
    #[must_use]
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::File => Self::FILE,
            Self::Executable => Self::EXECUTABLE,
            Self::Symlink => Self::SYMLINK,
            Self::Gitlink => Self::GITLINK,
        }
    }

    /// Converts a git mode integer to a [`FileMode`], if known.
    #[must_use]
    pub const fn from_u32(mode: u32) -> Option<Self> {
        match mode {
            Self::FILE => Some(Self::File),
            Self::EXECUTABLE => Some(Self::Executable),
            Self::SYMLINK => Some(Self::Symlink),
            Self::GITLINK => Some(Self::Gitlink),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_mode_round_trip() {
        assert_eq!(FileMode::File.to_u32(), 0o100_644);
        assert_eq!(FileMode::Executable.to_u32(), 0o100_755);
        assert_eq!(FileMode::Symlink.to_u32(), 0o120_000);
        assert_eq!(FileMode::Gitlink.to_u32(), 0o160_000);
        assert_eq!(FileMode::from_u32(0o100_644), Some(FileMode::File));
        assert_eq!(FileMode::from_u32(0o100_755), Some(FileMode::Executable));
        assert_eq!(FileMode::from_u32(0o100_000), None);
    }

    #[test]
    fn serde_names() {
        assert_eq!(
            serde_json::to_string(&FileMode::Executable).unwrap(),
            "\"executable\""
        );
        let mode: FileMode = serde_json::from_str("\"symlink\"").unwrap();
        assert_eq!(mode, FileMode::Symlink);
    }
}
