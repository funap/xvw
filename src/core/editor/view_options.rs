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
    /// Remove the target byte(s) in Overwrite mode, reducing file size.
    DeleteBytes,
}

impl OverwriteDeleteBehavior {
    pub const ALL: [Self; 3] = [Self::Disallow, Self::ZeroFill, Self::DeleteBytes];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Disallow => "Disallow (Protect Size)",
            Self::ZeroFill => "Zero-Fill (0x00)",
            Self::DeleteBytes => "Delete Bytes (Resize)",
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
