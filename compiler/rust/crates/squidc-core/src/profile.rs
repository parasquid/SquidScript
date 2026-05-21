use serde::{Deserialize, Serialize};

pub const PORTABLE_TARGET_ID: &str = "portable";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildProfile {
    Dev,
    Release,
}

impl BuildProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dev" => Some(Self::Dev),
            "release" => Some(Self::Release),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }
}
