use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use serde::{Deserialize, Serialize};

/// Behavior of Backspace, Delete, and Cut while editing in Overwrite mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverwriteDeleteBehavior {
    /// Ignore delete/cut operations in Overwrite mode and notify the user, keeping file size fixed.
    #[default]
    Disallow,
    /// Overwrite the target byte(s) with `0x00` in Overwrite mode, keeping file size fixed.
    ZeroFill,
}

impl OverwriteDeleteBehavior {
    pub const ALL: [Self; 2] = [Self::Disallow, Self::ZeroFill];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Disallow => "Disallow (Protect Size)",
            Self::ZeroFill => "Zero-Fill (0x00)",
        }
    }
}

/// Display and representation settings for binary viewing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    pub encoding: Encoding,
    pub radix: DisplayRadix,
    pub group_size: ByteGroupSize,
    pub is_big_endian: bool,
}
