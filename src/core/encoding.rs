//! Text encoding support for xvw.
//!
//! Provides conversion between byte sequences and characters for various character encodings
//! (Unicode, Japanese, Chinese, Korean, ISO-8859 family, Windows Code Pages, etc.),
//! with support for continuation byte detection, encoding name resolution, and UI category groupings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// High-level categorization of encodings for UI menus and settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncodingCategory {
    /// Unicode and ASCII encodings (ASCII, UTF-8, UTF-16 LE, UTF-16 BE).
    Unicode,
    /// Japanese encodings (Shift-JIS, EUC-JP, ISO-2022-JP).
    Japanese,
    /// Chinese and Korean encodings (GBK, GB18030, Big5, EUC-KR).
    ChineseKorean,
    /// ISO-8859 European standard encodings (ISO-8859-1 through ISO-8859-16).
    Iso8859,
    /// Windows ANSI code pages (Windows-1250 through Windows-1258).
    Windows,
    /// Legacy DOS and platform encodings (KOI8-R, KOI8-U, Mac OS Roman, IBM866).
    Legacy,
}

impl EncodingCategory {
    /// Human-readable display label for the category.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unicode => "Unicode & ASCII",
            Self::Japanese => "Japanese",
            Self::ChineseKorean => "Chinese & Korean",
            Self::Iso8859 => "ISO-8859",
            Self::Windows => "Windows Code Pages",
            Self::Legacy => "Legacy / DOS / Mac",
        }
    }
}

/// Supported text encodings in xvw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Encoding {
    // Unicode & ASCII
    #[serde(rename = "ascii")]
    #[default]
    Ascii,
    #[serde(rename = "utf-8", alias = "utf8")]
    Utf8,
    #[serde(rename = "utf-16-le", alias = "utf-16le", alias = "utf16le")]
    Utf16Le,
    #[serde(rename = "utf-16-be", alias = "utf-16be", alias = "utf16be")]
    Utf16Be,

    // Japanese
    #[serde(rename = "shift-jis", alias = "shift_jis", alias = "sjis", alias = "cp932", alias = "windows-31j")]
    ShiftJis,
    #[serde(rename = "euc-jp", alias = "euc_jp")]
    EucJp,
    #[serde(rename = "iso-2022-jp", alias = "iso_2022_jp")]
    Iso2022Jp,

    // Chinese & Korean
    #[serde(rename = "gbk", alias = "cp936", alias = "gb2312")]
    Gbk,
    #[serde(rename = "gb18030")]
    Gb18030,
    #[serde(rename = "big5", alias = "cp950")]
    Big5,
    #[serde(rename = "euc-kr", alias = "euc_kr", alias = "cp949")]
    EucKr,

    // ISO-8859 family
    #[serde(rename = "iso-8859-1", alias = "iso_8859_1", alias = "latin1", alias = "latin-1")]
    Iso8859_1,
    #[serde(rename = "iso-8859-2", alias = "iso_8859_2", alias = "latin2", alias = "latin-2")]
    Iso8859_2,
    #[serde(rename = "iso-8859-3", alias = "iso_8859_3", alias = "latin3", alias = "latin-3")]
    Iso8859_3,
    #[serde(rename = "iso-8859-4", alias = "iso_8859_4", alias = "latin4", alias = "latin-4")]
    Iso8859_4,
    #[serde(rename = "iso-8859-5", alias = "iso_8859_5")]
    Iso8859_5,
    #[serde(rename = "iso-8859-6", alias = "iso_8859_6")]
    Iso8859_6,
    #[serde(rename = "iso-8859-7", alias = "iso_8859_7")]
    Iso8859_7,
    #[serde(rename = "iso-8859-8", alias = "iso_8859_8")]
    Iso8859_8,
    #[serde(rename = "iso-8859-8-i", alias = "iso_8859_8_i")]
    Iso8859_8I,
    #[serde(rename = "iso-8859-10", alias = "iso_8859_10", alias = "latin6", alias = "latin-6")]
    Iso8859_10,
    #[serde(rename = "iso-8859-13", alias = "iso_8859_13", alias = "latin7", alias = "latin-7")]
    Iso8859_13,
    #[serde(rename = "iso-8859-14", alias = "iso_8859_14", alias = "latin8", alias = "latin-8")]
    Iso8859_14,
    #[serde(rename = "iso-8859-15", alias = "iso_8859_15", alias = "latin9", alias = "latin-9")]
    Iso8859_15,
    #[serde(rename = "iso-8859-16", alias = "iso_8859_16", alias = "latin10", alias = "latin-10")]
    Iso8859_16,

    // Windows Code Pages
    #[serde(rename = "windows-1250", alias = "cp1250")]
    Windows1250,
    #[serde(rename = "windows-1251", alias = "cp1251")]
    Windows1251,
    #[serde(rename = "windows-1252", alias = "cp1252")]
    Windows1252,
    #[serde(rename = "windows-1253", alias = "cp1253")]
    Windows1253,
    #[serde(rename = "windows-1254", alias = "cp1254")]
    Windows1254,
    #[serde(rename = "windows-1255", alias = "cp1255")]
    Windows1255,
    #[serde(rename = "windows-1256", alias = "cp1256")]
    Windows1256,
    #[serde(rename = "windows-1257", alias = "cp1257")]
    Windows1257,
    #[serde(rename = "windows-1258", alias = "cp1258")]
    Windows1258,

    // Legacy / DOS / Mac
    #[serde(rename = "koi8-r", alias = "koi8r")]
    Koi8R,
    #[serde(rename = "koi8-u", alias = "koi8u")]
    Koi8U,
    #[serde(rename = "macintosh", alias = "mac-roman")]
    Macintosh,
    #[serde(rename = "ibm866", alias = "cp866")]
    Ibm866,
}

impl gpui_kit::Global for Encoding {}

impl Encoding {
    /// Returns the display label used by the UI and menus.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ascii => "ASCII",
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::ShiftJis => "Shift-JIS",
            Self::EucJp => "EUC-JP",
            Self::Iso2022Jp => "ISO-2022-JP",
            Self::Gbk => "GBK",
            Self::Gb18030 => "GB18030",
            Self::Big5 => "Big5",
            Self::EucKr => "EUC-KR",
            Self::Iso8859_1 => "ISO-8859-1 (Latin-1)",
            Self::Iso8859_2 => "ISO-8859-2 (Latin-2)",
            Self::Iso8859_3 => "ISO-8859-3 (Latin-3)",
            Self::Iso8859_4 => "ISO-8859-4 (Latin-4)",
            Self::Iso8859_5 => "ISO-8859-5 (Cyrillic)",
            Self::Iso8859_6 => "ISO-8859-6 (Arabic)",
            Self::Iso8859_7 => "ISO-8859-7 (Greek)",
            Self::Iso8859_8 => "ISO-8859-8 (Hebrew)",
            Self::Iso8859_8I => "ISO-8859-8-I",
            Self::Iso8859_10 => "ISO-8859-10 (Latin-6)",
            Self::Iso8859_13 => "ISO-8859-13 (Baltic Rim)",
            Self::Iso8859_14 => "ISO-8859-14 (Celtic)",
            Self::Iso8859_15 => "ISO-8859-15 (Latin-9)",
            Self::Iso8859_16 => "ISO-8859-16 (South-Eastern)",
            Self::Windows1250 => "Windows-1250 (Central European)",
            Self::Windows1251 => "Windows-1251 (Cyrillic)",
            Self::Windows1252 => "Windows-1252 (Western European)",
            Self::Windows1253 => "Windows-1253 (Greek)",
            Self::Windows1254 => "Windows-1254 (Turkish)",
            Self::Windows1255 => "Windows-1255 (Hebrew)",
            Self::Windows1256 => "Windows-1256 (Arabic)",
            Self::Windows1257 => "Windows-1257 (Baltic)",
            Self::Windows1258 => "Windows-1258 (Vietnamese)",
            Self::Koi8R => "KOI8-R (Russian)",
            Self::Koi8U => "KOI8-U (Ukrainian)",
            Self::Macintosh => "Mac OS Roman",
            Self::Ibm866 => "IBM866 (DOS Cyrillic)",
        }
    }

    /// Returns the category for UI grouping.
    #[allow(dead_code)]
    pub fn category(self) -> EncodingCategory {
        match self {
            Self::Ascii | Self::Utf8 | Self::Utf16Le | Self::Utf16Be => EncodingCategory::Unicode,
            Self::ShiftJis | Self::EucJp | Self::Iso2022Jp => EncodingCategory::Japanese,
            Self::Gbk | Self::Gb18030 | Self::Big5 | Self::EucKr => EncodingCategory::ChineseKorean,
            Self::Iso8859_1
            | Self::Iso8859_2
            | Self::Iso8859_3
            | Self::Iso8859_4
            | Self::Iso8859_5
            | Self::Iso8859_6
            | Self::Iso8859_7
            | Self::Iso8859_8
            | Self::Iso8859_8I
            | Self::Iso8859_10
            | Self::Iso8859_13
            | Self::Iso8859_14
            | Self::Iso8859_15
            | Self::Iso8859_16 => EncodingCategory::Iso8859,
            Self::Windows1250
            | Self::Windows1251
            | Self::Windows1252
            | Self::Windows1253
            | Self::Windows1254
            | Self::Windows1255
            | Self::Windows1256
            | Self::Windows1257
            | Self::Windows1258 => EncodingCategory::Windows,
            Self::Koi8R | Self::Koi8U | Self::Macintosh | Self::Ibm866 => EncodingCategory::Legacy,
        }
    }

    /// Returns the list of all supported encodings grouped by category.
    pub fn categories() -> &'static [(EncodingCategory, &'static [Encoding])] {
        &[
            (
                EncodingCategory::Unicode,
                &[Encoding::Ascii, Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be],
            ),
            (EncodingCategory::Japanese, &[Encoding::ShiftJis, Encoding::EucJp, Encoding::Iso2022Jp]),
            (
                EncodingCategory::ChineseKorean,
                &[Encoding::Gbk, Encoding::Gb18030, Encoding::Big5, Encoding::EucKr],
            ),
            (
                EncodingCategory::Iso8859,
                &[
                    Encoding::Iso8859_1,
                    Encoding::Iso8859_2,
                    Encoding::Iso8859_3,
                    Encoding::Iso8859_4,
                    Encoding::Iso8859_5,
                    Encoding::Iso8859_6,
                    Encoding::Iso8859_7,
                    Encoding::Iso8859_8,
                    Encoding::Iso8859_8I,
                    Encoding::Iso8859_10,
                    Encoding::Iso8859_13,
                    Encoding::Iso8859_14,
                    Encoding::Iso8859_15,
                    Encoding::Iso8859_16,
                ],
            ),
            (
                EncodingCategory::Windows,
                &[
                    Encoding::Windows1250,
                    Encoding::Windows1251,
                    Encoding::Windows1252,
                    Encoding::Windows1253,
                    Encoding::Windows1254,
                    Encoding::Windows1255,
                    Encoding::Windows1256,
                    Encoding::Windows1257,
                    Encoding::Windows1258,
                ],
            ),
            (
                EncodingCategory::Legacy,
                &[Encoding::Koi8R, Encoding::Koi8U, Encoding::Macintosh, Encoding::Ibm866],
            ),
        ]
    }

    /// Returns all supported encodings.
    #[allow(dead_code)]
    pub fn all() -> &'static [Encoding] {
        &[
            Encoding::Ascii,
            Encoding::Utf8,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::ShiftJis,
            Encoding::EucJp,
            Encoding::Iso2022Jp,
            Encoding::Gbk,
            Encoding::Gb18030,
            Encoding::Big5,
            Encoding::EucKr,
            Encoding::Iso8859_1,
            Encoding::Iso8859_2,
            Encoding::Iso8859_3,
            Encoding::Iso8859_4,
            Encoding::Iso8859_5,
            Encoding::Iso8859_6,
            Encoding::Iso8859_7,
            Encoding::Iso8859_8,
            Encoding::Iso8859_8I,
            Encoding::Iso8859_10,
            Encoding::Iso8859_13,
            Encoding::Iso8859_14,
            Encoding::Iso8859_15,
            Encoding::Iso8859_16,
            Encoding::Windows1250,
            Encoding::Windows1251,
            Encoding::Windows1252,
            Encoding::Windows1253,
            Encoding::Windows1254,
            Encoding::Windows1255,
            Encoding::Windows1256,
            Encoding::Windows1257,
            Encoding::Windows1258,
            Encoding::Koi8R,
            Encoding::Koi8U,
            Encoding::Macintosh,
            Encoding::Ibm866,
        ]
    }

    /// Returns the underlying `encoding_rs::Encoding` reference if backed by `encoding_rs`.
    fn encoding_rs_ref(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            Self::Ascii | Self::Iso8859_1 => None,
            Self::Utf8 => Some(encoding_rs::UTF_8),
            Self::Utf16Le => Some(encoding_rs::UTF_16LE),
            Self::Utf16Be => Some(encoding_rs::UTF_16BE),
            Self::ShiftJis => Some(encoding_rs::SHIFT_JIS),
            Self::EucJp => Some(encoding_rs::EUC_JP),
            Self::Iso2022Jp => Some(encoding_rs::ISO_2022_JP),
            Self::Gbk => Some(encoding_rs::GBK),
            Self::Gb18030 => Some(encoding_rs::GB18030),
            Self::Big5 => Some(encoding_rs::BIG5),
            Self::EucKr => Some(encoding_rs::EUC_KR),
            Self::Iso8859_2 => Some(encoding_rs::ISO_8859_2),
            Self::Iso8859_3 => Some(encoding_rs::ISO_8859_3),
            Self::Iso8859_4 => Some(encoding_rs::ISO_8859_4),
            Self::Iso8859_5 => Some(encoding_rs::ISO_8859_5),
            Self::Iso8859_6 => Some(encoding_rs::ISO_8859_6),
            Self::Iso8859_7 => Some(encoding_rs::ISO_8859_7),
            Self::Iso8859_8 => Some(encoding_rs::ISO_8859_8),
            Self::Iso8859_8I => Some(encoding_rs::ISO_8859_8_I),
            Self::Iso8859_10 => Some(encoding_rs::ISO_8859_10),
            Self::Iso8859_13 => Some(encoding_rs::ISO_8859_13),
            Self::Iso8859_14 => Some(encoding_rs::ISO_8859_14),
            Self::Iso8859_15 => Some(encoding_rs::ISO_8859_15),
            Self::Iso8859_16 => Some(encoding_rs::ISO_8859_16),
            Self::Windows1250 => Some(encoding_rs::WINDOWS_1250),
            Self::Windows1251 => Some(encoding_rs::WINDOWS_1251),
            Self::Windows1252 => Some(encoding_rs::WINDOWS_1252),
            Self::Windows1253 => Some(encoding_rs::WINDOWS_1253),
            Self::Windows1254 => Some(encoding_rs::WINDOWS_1254),
            Self::Windows1255 => Some(encoding_rs::WINDOWS_1255),
            Self::Windows1256 => Some(encoding_rs::WINDOWS_1256),
            Self::Windows1257 => Some(encoding_rs::WINDOWS_1257),
            Self::Windows1258 => Some(encoding_rs::WINDOWS_1258),
            Self::Koi8R => Some(encoding_rs::KOI8_R),
            Self::Koi8U => Some(encoding_rs::KOI8_U),
            Self::Macintosh => Some(encoding_rs::MACINTOSH),
            Self::Ibm866 => Some(encoding_rs::IBM866),
        }
    }

    /// Converts from an `encoding_rs::Encoding` reference.
    fn from_encoding_rs(enc: &'static encoding_rs::Encoding) -> Option<Self> {
        if enc == encoding_rs::UTF_8 {
            Some(Self::Utf8)
        } else if enc == encoding_rs::UTF_16LE {
            Some(Self::Utf16Le)
        } else if enc == encoding_rs::UTF_16BE {
            Some(Self::Utf16Be)
        } else if enc == encoding_rs::SHIFT_JIS {
            Some(Self::ShiftJis)
        } else if enc == encoding_rs::EUC_JP {
            Some(Self::EucJp)
        } else if enc == encoding_rs::ISO_2022_JP {
            Some(Self::Iso2022Jp)
        } else if enc == encoding_rs::GBK {
            Some(Self::Gbk)
        } else if enc == encoding_rs::GB18030 {
            Some(Self::Gb18030)
        } else if enc == encoding_rs::BIG5 {
            Some(Self::Big5)
        } else if enc == encoding_rs::EUC_KR {
            Some(Self::EucKr)
        } else if enc == encoding_rs::ISO_8859_2 {
            Some(Self::Iso8859_2)
        } else if enc == encoding_rs::ISO_8859_3 {
            Some(Self::Iso8859_3)
        } else if enc == encoding_rs::ISO_8859_4 {
            Some(Self::Iso8859_4)
        } else if enc == encoding_rs::ISO_8859_5 {
            Some(Self::Iso8859_5)
        } else if enc == encoding_rs::ISO_8859_6 {
            Some(Self::Iso8859_6)
        } else if enc == encoding_rs::ISO_8859_7 {
            Some(Self::Iso8859_7)
        } else if enc == encoding_rs::ISO_8859_8 {
            Some(Self::Iso8859_8)
        } else if enc == encoding_rs::ISO_8859_8_I {
            Some(Self::Iso8859_8I)
        } else if enc == encoding_rs::ISO_8859_10 {
            Some(Self::Iso8859_10)
        } else if enc == encoding_rs::ISO_8859_13 {
            Some(Self::Iso8859_13)
        } else if enc == encoding_rs::ISO_8859_14 {
            Some(Self::Iso8859_14)
        } else if enc == encoding_rs::ISO_8859_15 {
            Some(Self::Iso8859_15)
        } else if enc == encoding_rs::ISO_8859_16 {
            Some(Self::Iso8859_16)
        } else if enc == encoding_rs::WINDOWS_1250 {
            Some(Self::Windows1250)
        } else if enc == encoding_rs::WINDOWS_1251 {
            Some(Self::Windows1251)
        } else if enc == encoding_rs::WINDOWS_1252 {
            Some(Self::Windows1252)
        } else if enc == encoding_rs::WINDOWS_1253 {
            Some(Self::Windows1253)
        } else if enc == encoding_rs::WINDOWS_1254 {
            Some(Self::Windows1254)
        } else if enc == encoding_rs::WINDOWS_1255 {
            Some(Self::Windows1255)
        } else if enc == encoding_rs::WINDOWS_1256 {
            Some(Self::Windows1256)
        } else if enc == encoding_rs::WINDOWS_1257 {
            Some(Self::Windows1257)
        } else if enc == encoding_rs::WINDOWS_1258 {
            Some(Self::Windows1258)
        } else if enc == encoding_rs::KOI8_R {
            Some(Self::Koi8R)
        } else if enc == encoding_rs::KOI8_U {
            Some(Self::Koi8U)
        } else if enc == encoding_rs::MACINTOSH {
            Some(Self::Macintosh)
        } else if enc == encoding_rs::IBM866 {
            Some(Self::Ibm866)
        } else {
            None
        }
    }

    /// Resolves an encoding from a name, code page, or alias (e.g. for Kaitai Struct or user input).
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.trim().to_lowercase().replace('_', "-");
        match normalized.as_str() {
            "ascii" | "us-ascii" | "raw" => Some(Self::Ascii),
            "utf-8" | "utf8" => Some(Self::Utf8),
            "utf-16le" | "utf16le" | "ucs-2le" | "ucs2le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" | "ucs-2be" | "ucs2be" => Some(Self::Utf16Be),
            "shift-jis" | "shiftjis" | "sjis" | "cp932" | "windows-31j" | "ms932" | "x-sjis" => Some(Self::ShiftJis),
            "euc-jp" | "eucjp" | "ujis" => Some(Self::EucJp),
            "iso-2022-jp" | "iso2022jp" => Some(Self::Iso2022Jp),
            "gbk" | "cp936" | "gb2312" | "chinese" => Some(Self::Gbk),
            "gb18030" => Some(Self::Gb18030),
            "big5" | "big5-hkscs" | "cp950" => Some(Self::Big5),
            "euc-kr" | "cp949" | "korean" => Some(Self::EucKr),
            "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "l1" => Some(Self::Iso8859_1),
            "iso-8859-2" | "iso8859-2" | "latin2" | "latin-2" | "l2" => Some(Self::Iso8859_2),
            "iso-8859-3" | "iso8859-3" | "latin3" | "latin-3" | "l3" => Some(Self::Iso8859_3),
            "iso-8859-4" | "iso8859-4" | "latin4" | "latin-4" | "l4" => Some(Self::Iso8859_4),
            "iso-8859-5" | "iso8859-5" | "cyrillic" => Some(Self::Iso8859_5),
            "iso-8859-6" | "iso8859-6" | "arabic" => Some(Self::Iso8859_6),
            "iso-8859-7" | "iso8859-7" | "greek" => Some(Self::Iso8859_7),
            "iso-8859-8" | "iso8859-8" | "hebrew" => Some(Self::Iso8859_8),
            "iso-8859-8-i" | "iso8859-8-i" => Some(Self::Iso8859_8I),
            "iso-8859-10" | "iso8859-10" | "latin6" | "latin-6" | "l6" => Some(Self::Iso8859_10),
            "iso-8859-13" | "iso8859-13" | "latin7" | "latin-7" | "l7" => Some(Self::Iso8859_13),
            "iso-8859-14" | "iso8859-14" | "latin8" | "latin-8" | "l8" => Some(Self::Iso8859_14),
            "iso-8859-15" | "iso8859-15" | "latin9" | "latin-9" | "l9" => Some(Self::Iso8859_15),
            "iso-8859-16" | "iso8859-16" | "latin10" | "latin-10" | "l10" => Some(Self::Iso8859_16),
            "windows-1250" | "cp1250" | "1250" => Some(Self::Windows1250),
            "windows-1251" | "cp1251" | "1251" => Some(Self::Windows1251),
            "windows-1252" | "cp1252" | "1252" => Some(Self::Windows1252),
            "windows-1253" | "cp1253" | "1253" => Some(Self::Windows1253),
            "windows-1254" | "cp1254" | "1254" => Some(Self::Windows1254),
            "windows-1255" | "cp1255" | "1255" => Some(Self::Windows1255),
            "windows-1256" | "cp1256" | "1256" => Some(Self::Windows1256),
            "windows-1257" | "cp1257" | "1257" => Some(Self::Windows1257),
            "windows-1258" | "cp1258" | "1258" => Some(Self::Windows1258),
            "koi8-r" | "koi8r" => Some(Self::Koi8R),
            "koi8-u" | "koi8u" => Some(Self::Koi8U),
            "macintosh" | "mac-roman" | "mac" => Some(Self::Macintosh),
            "ibm866" | "cp866" | "866" => Some(Self::Ibm866),
            _ => encoding_rs::Encoding::for_label(name.as_bytes()).and_then(Self::from_encoding_rs),
        }
    }

    /// The byte alignment for character boundaries during string scanning.
    pub fn alignment(self) -> usize {
        match self {
            Self::Utf16Le | Self::Utf16Be => 2,
            _ => 1,
        }
    }

    /// The maximum number of bytes that a single character can occupy in this encoding.
    pub fn max_bytes_per_char(self) -> usize {
        match self {
            Self::Ascii
            | Self::Iso8859_1
            | Self::Iso8859_2
            | Self::Iso8859_3
            | Self::Iso8859_4
            | Self::Iso8859_5
            | Self::Iso8859_6
            | Self::Iso8859_7
            | Self::Iso8859_8
            | Self::Iso8859_8I
            | Self::Iso8859_10
            | Self::Iso8859_13
            | Self::Iso8859_14
            | Self::Iso8859_15
            | Self::Iso8859_16
            | Self::Windows1250
            | Self::Windows1251
            | Self::Windows1252
            | Self::Windows1253
            | Self::Windows1254
            | Self::Windows1255
            | Self::Windows1256
            | Self::Windows1257
            | Self::Windows1258
            | Self::Koi8R
            | Self::Koi8U
            | Self::Macintosh
            | Self::Ibm866 => 1,
            Self::ShiftJis | Self::EucJp | Self::Big5 | Self::Gbk | Self::EucKr => 2,
            Self::Utf8 | Self::Utf16Le | Self::Utf16Be | Self::Gb18030 | Self::Iso2022Jp => 4,
        }
    }

    /// Whether this encoding supports multi-byte character sequences.
    pub fn is_multibyte(self) -> bool {
        self.max_bytes_per_char() > 1
    }

    /// Encodes one Unicode scalar value using this encoding.
    pub fn encode_char(&self, character: char) -> Option<Vec<u8>> {
        match self {
            Encoding::Ascii => {
                if character.is_ascii() {
                    Some(vec![character as u8])
                } else {
                    None
                }
            }
            Encoding::Iso8859_1 => {
                let code = character as u32;
                if code <= 255 { Some(vec![code as u8]) } else { None }
            }
            Encoding::Utf8 => {
                let mut encoded = [0u8; 4];
                Some(character.encode_utf8(&mut encoded).as_bytes().to_vec())
            }
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let mut units = [0u16; 2];
                let encoded = character.encode_utf16(&mut units);
                let mut bytes = Vec::with_capacity(encoded.len() * 2);
                for unit in encoded.iter().copied() {
                    let pair = if *self == Encoding::Utf16Le { unit.to_le_bytes() } else { unit.to_be_bytes() };
                    bytes.extend_from_slice(&pair);
                }
                Some(bytes)
            }
            _ => {
                if let Some(enc) = self.encoding_rs_ref() {
                    let mut utf8_buf = [0u8; 4];
                    let utf8_str = character.encode_utf8(&mut utf8_buf);
                    let mut encoder = enc.new_encoder();
                    let mut dest = [0u8; 8];
                    let (result, read, written) = encoder.encode_from_utf8_without_replacement(utf8_str, &mut dest, true);
                    if result == encoding_rs::EncoderResult::InputEmpty && read == utf8_str.len() && written > 0 {
                        return Some(dest[..written].to_vec());
                    }
                }
                None
            }
        }
    }

    /// Encodes a Unicode string slice using this encoding.
    ///
    /// Returns `Some(Vec<u8>)` on success, or `None` if the string contains characters
    /// that cannot be represented in this encoding.
    pub fn encode_str(&self, text: &str) -> Option<Vec<u8>> {
        if text.is_empty() {
            return Some(Vec::new());
        }

        match self {
            Encoding::Ascii => {
                if text.is_ascii() {
                    Some(text.as_bytes().to_vec())
                } else {
                    None
                }
            }
            Encoding::Iso8859_1 => {
                let mut bytes = Vec::with_capacity(text.len());
                for c in text.chars() {
                    let code = c as u32;
                    if code <= 255 {
                        bytes.push(code as u8);
                    } else {
                        return None;
                    }
                }
                Some(bytes)
            }
            Encoding::Utf8 => Some(text.as_bytes().to_vec()),
            Encoding::Utf16Le => {
                let mut bytes = Vec::with_capacity(text.len() * 2);
                for unit in text.encode_utf16() {
                    bytes.extend_from_slice(&unit.to_le_bytes());
                }
                Some(bytes)
            }
            Encoding::Utf16Be => {
                let mut bytes = Vec::with_capacity(text.len() * 2);
                for unit in text.encode_utf16() {
                    bytes.extend_from_slice(&unit.to_be_bytes());
                }
                Some(bytes)
            }
            _ => {
                if let Some(enc) = self.encoding_rs_ref() {
                    let mut encoder = enc.new_encoder();
                    let max_len = encoder.max_buffer_length_from_utf8_if_no_unmappables(text.len())?;
                    let mut dest = vec![0u8; max_len];
                    let (result, read, written) = encoder.encode_from_utf8_without_replacement(text, &mut dest, true);
                    if result == encoding_rs::EncoderResult::InputEmpty && read == text.len() {
                        dest.truncate(written);
                        return Some(dest);
                    }
                }
                None
            }
        }
    }

    /// Decodes and formats a preview text string of up to `byte_len` bytes starting at `offset`.
    /// Printable characters are rendered as-is, while control codes, invalid sequences,
    /// or unassigned bytes are replaced with `.`.
    pub fn format_preview(&self, buffer: &[u8], offset: usize, byte_len: usize) -> String {
        if offset >= buffer.len() {
            return String::new();
        }
        let end = (offset + byte_len).min(buffer.len());
        let mut text = String::new();
        let mut scan = offset;
        while scan < end {
            if let Some((c, char_len)) = self.decode_char_at(buffer, scan) {
                let len = char_len.max(1);
                if !c.is_control() && c != '\u{FFFD}' {
                    text.push(c);
                } else {
                    text.push('.');
                }
                scan += len;
            } else {
                text.push('.');
                scan += 1;
            }
        }
        text
    }

    /// Decodes a single character starting at `offset` in `buffer`.
    ///
    /// Returns `Some((character, byte_len))` if a valid printable character was decoded,
    /// or `None` if the sequence is invalid, incomplete, or non-printable.
    pub fn decode_char_at(&self, buffer: &[u8], offset: usize) -> Option<(char, usize)> {
        if offset >= buffer.len() {
            return None;
        }

        match self {
            Encoding::Ascii => {
                let b = buffer[offset];
                if (32..=126).contains(&b) { Some((b as char, 1)) } else { None }
            }
            Encoding::Iso8859_1 => {
                let b = buffer[offset];
                if (32..=126).contains(&b) || (160..=255).contains(&b) {
                    Some((b as char, 1))
                } else {
                    None
                }
            }
            Encoding::Utf8 => {
                let b = buffer[offset];
                let len = if b & 0x80 == 0 {
                    1
                } else if b & 0xE0 == 0xC0 {
                    2
                } else if b & 0xF0 == 0xE0 {
                    3
                } else if b & 0xF8 == 0xF0 {
                    4
                } else {
                    return None;
                };

                if offset + len <= buffer.len()
                    && let Ok(s) = std::str::from_utf8(&buffer[offset..offset + len])
                {
                    let c = s.chars().next().expect("valid utf-8 character");
                    let is_printable = !c.is_control() && c != '\u{FFFD}';
                    if is_printable {
                        return Some((c, len));
                    }
                }
                None
            }
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let is_le = *self == Encoding::Utf16Le;
                if !offset.is_multiple_of(2) {
                    return None;
                }
                if offset + 2 <= buffer.len() {
                    let u1 = if is_le {
                        u16::from_le_bytes([buffer[offset], buffer[offset + 1]])
                    } else {
                        u16::from_be_bytes([buffer[offset], buffer[offset + 1]])
                    };

                    if (0xD800..=0xDBFF).contains(&u1) {
                        // High surrogate
                        if offset + 4 <= buffer.len() {
                            let u2 = if is_le {
                                u16::from_le_bytes([buffer[offset + 2], buffer[offset + 3]])
                            } else {
                                u16::from_be_bytes([buffer[offset + 2], buffer[offset + 3]])
                            };
                            if (0xDC00..=0xDFFF).contains(&u2) {
                                // Low surrogate
                                if let Some(c) = std::char::decode_utf16([u1, u2]).next().and_then(|r| r.ok()) {
                                    let is_printable = !c.is_control() && c != '\u{FFFD}';
                                    if is_printable {
                                        return Some((c, 4));
                                    }
                                }
                            }
                        }
                    } else if !(0xDC00..=0xDFFF).contains(&u1) {
                        // Not a low surrogate
                        if let Some(c) = std::char::decode_utf16([u1]).next().and_then(|r| r.ok()) {
                            let is_printable = !c.is_control() && c != '\u{FFFD}';
                            if is_printable {
                                return Some((c, 2));
                            }
                        }
                    }
                }
                None
            }
            _ => {
                if let Some(enc) = self.encoding_rs_ref() {
                    let max_len = self.max_bytes_per_char();
                    let remaining = buffer.len() - offset;
                    let upper = max_len.min(remaining);

                    for k in 1..=upper {
                        let slice = &buffer[offset..offset + k];
                        let (cow, had_errors) = enc.decode_without_bom_handling(slice);
                        if !had_errors {
                            let mut chars = cow.chars();
                            if let Some(c) = chars.next()
                                && chars.next().is_none()
                                && !c.is_control()
                                && c != '\u{FFFD}'
                            {
                                return Some((c, k));
                            }
                        }
                    }
                }
                None
            }
        }
    }

    /// Determines if the byte at `offset` is a continuation byte of a multibyte character.
    #[allow(dead_code)]
    pub fn is_continuation_byte(&self, buffer: &[u8], offset: usize) -> bool {
        if offset >= buffer.len() || !self.is_multibyte() {
            return false;
        }

        match self {
            Encoding::Ascii => false,
            Encoding::Utf8 => {
                if buffer[offset] & 0xC0 != 0x80 {
                    return false;
                }
                for i in 1..=3 {
                    if offset >= i {
                        let start_idx = offset - i;
                        if buffer[start_idx] & 0xC0 != 0x80 {
                            if let Some((_, len)) = self.decode_char_at(buffer, start_idx) {
                                return start_idx + len > offset;
                            } else {
                                return false;
                            }
                        }
                    }
                }
                false
            }
            Encoding::Utf16Le | Encoding::Utf16Be => {
                if !offset.is_multiple_of(2) {
                    let start_idx = offset - 1;
                    if let Some((_, len)) = self.decode_char_at(buffer, start_idx) {
                        return start_idx + len > offset;
                    }
                    if start_idx >= 2 {
                        let prev_start = start_idx - 2;
                        if let Some((_, len)) = self.decode_char_at(buffer, prev_start) {
                            return prev_start + len > offset;
                        }
                    }
                    false
                } else {
                    if offset >= 2 {
                        let prev_start = offset - 2;
                        if let Some((_, len)) = self.decode_char_at(buffer, prev_start) {
                            return prev_start + len > offset;
                        }
                    }
                    false
                }
            }
            _ => {
                let max_back = self.max_bytes_per_char().saturating_sub(1);
                for i in (1..=max_back).rev() {
                    if offset >= i {
                        let start_idx = offset - i;
                        if let Some((_, len)) = self.decode_char_at(buffer, start_idx)
                            && start_idx + len > offset
                        {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }

    /// Returns the byte range `start..end` of the character containing `offset` in `buffer`.
    ///
    /// If `offset >= buffer.len()`, returns `buffer.len()..buffer.len()`.
    /// If the character cannot be decoded, returns `offset..(offset + 1).min(buffer.len())`.
    pub fn char_range_at(&self, buffer: &[u8], offset: usize) -> std::ops::Range<usize> {
        if offset >= buffer.len() {
            return buffer.len()..buffer.len();
        }

        if !self.is_multibyte() {
            return offset..(offset + 1).min(buffer.len());
        }

        let max_back = 64.min(offset);
        let window_start = offset - max_back;
        let mut sync_start = window_start;

        for pos in (window_start..offset).rev() {
            if buffer[pos] < 0x40 {
                sync_start = pos;
                break;
            }
        }

        let mut scan_pos = sync_start;
        while scan_pos <= offset && scan_pos < buffer.len() {
            if let Some((_, len)) = self.decode_char_at(buffer, scan_pos) {
                let char_len = len.max(1);
                let char_end = (scan_pos + char_len).min(buffer.len());
                if scan_pos <= offset && offset < char_end {
                    return scan_pos..char_end;
                }
                scan_pos = char_end;
            } else {
                if scan_pos == offset {
                    return offset..(offset + 1).min(buffer.len());
                }
                scan_pos += 1;
            }
        }

        offset..(offset + 1).min(buffer.len())
    }

    /// Returns the start byte offset of the character immediately preceding `offset`.
    ///
    /// If `offset == 0`, returns 0.
    pub fn prev_char_boundary(&self, buffer: &[u8], offset: usize) -> usize {
        if offset == 0 || buffer.is_empty() {
            return 0;
        }
        let clamped = offset.min(buffer.len());
        let prev_idx = clamped.saturating_sub(1);
        self.char_range_at(buffer, prev_idx).start
    }

    /// Returns the byte offset of the next character boundary after `offset`.
    ///
    /// If `offset >= buffer.len()`, returns `buffer.len()`.
    pub fn next_char_boundary(&self, buffer: &[u8], offset: usize) -> usize {
        if offset >= buffer.len() {
            return buffer.len();
        }
        self.char_range_at(buffer, offset).end.min(buffer.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::inspector::format_hex_values;

    #[test]
    fn test_format_hex_values() {
        let (h8, h16, h32, h64) = format_hex_values(&[], false);
        assert_eq!(h8, "--");
        assert_eq!(h16, "--");
        assert_eq!(h32, "--");
        assert_eq!(h64, "--");

        let (h8_p, h16_p, h32_p, h64_p) = format_hex_values(&[0x12, 0x34], false);
        assert_eq!(h8_p, "0x12");
        assert_eq!(h16_p, "0x3412");
        assert_eq!(h32_p, "--");
        assert_eq!(h64_p, "--");

        let bytes = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];

        // Little Endian
        let (h8, h16, h32, h64) = format_hex_values(&bytes, false);
        assert_eq!(h8, "0x01");
        assert_eq!(h16, "0x2301");
        assert_eq!(h32, "0x67452301");
        assert_eq!(h64, "0xEFCDAB8967452301");

        // Big Endian
        let (h8_be, h16_be, h32_be, h64_be) = format_hex_values(&bytes, true);
        assert_eq!(h8_be, "0x01");
        assert_eq!(h16_be, "0x0123");
        assert_eq!(h32_be, "0x01234567");
        assert_eq!(h64_be, "0x0123456789ABCDEF");
    }

    #[test]
    fn test_encoding_decode_char_at() {
        let ascii_bytes = b"Hello World";
        assert_eq!(Encoding::Ascii.decode_char_at(ascii_bytes, 0), Some(('H', 1)));

        let utf8_bytes = "こんにちは".as_bytes();
        assert_eq!(Encoding::Utf8.decode_char_at(utf8_bytes, 0), Some(('こ', 3)));

        let invalid_utf8 = vec![0xFF, 0xFE];
        assert_eq!(Encoding::Utf8.decode_char_at(&invalid_utf8, 0), None);

        let utf16le = vec![0x41, 0x00, 0x42, 0x00];
        assert_eq!(Encoding::Utf16Le.decode_char_at(&utf16le, 0), Some(('A', 2)));
        assert_eq!(Encoding::Utf16Le.decode_char_at(&utf16le, 2), Some(('B', 2)));

        let utf16be = vec![0x00, 0x41, 0x00, 0x42];
        assert_eq!(Encoding::Utf16Be.decode_char_at(&utf16be, 0), Some(('A', 2)));
        assert_eq!(Encoding::Utf16Be.decode_char_at(&utf16be, 2), Some(('B', 2)));
    }

    #[test]
    fn test_shift_jis_decode_encode() {
        // "こんにちは" in Shift-JIS: 0x82 0xB1, 0x82 0xF1, 0x82 0xC9, 0x82 0xBF, 0x82 0xCD
        let sjis_bytes = [0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD];
        assert_eq!(Encoding::ShiftJis.decode_char_at(&sjis_bytes, 0), Some(('こ', 2)));
        assert_eq!(Encoding::ShiftJis.decode_char_at(&sjis_bytes, 2), Some(('ん', 2)));
        assert_eq!(Encoding::ShiftJis.decode_char_at(&sjis_bytes, 4), Some(('に', 2)));
        assert_eq!(Encoding::ShiftJis.decode_char_at(&sjis_bytes, 6), Some(('ち', 2)));
        assert_eq!(Encoding::ShiftJis.decode_char_at(&sjis_bytes, 8), Some(('は', 2)));

        // Half-width kana 'ｱ' (0xB1)
        let half_kana = [0xB1];
        assert_eq!(Encoding::ShiftJis.decode_char_at(&half_kana, 0), Some(('ｱ', 1)));

        // ASCII in Shift-JIS
        let sjis_ascii = b"ABC";
        assert_eq!(Encoding::ShiftJis.decode_char_at(sjis_ascii, 0), Some(('A', 1)));

        // Encode Hiragana and Kanji
        assert_eq!(Encoding::ShiftJis.encode_char('こ'), Some(vec![0x82, 0xB1]));
        assert_eq!(Encoding::ShiftJis.encode_char('日'), Some(vec![0x93, 0xFA]));
        assert_eq!(Encoding::ShiftJis.encode_char('A'), Some(vec![0x41]));

        // Continuation byte check
        assert!(!Encoding::ShiftJis.is_continuation_byte(&sjis_bytes, 0));
        assert!(Encoding::ShiftJis.is_continuation_byte(&sjis_bytes, 1));
        assert!(!Encoding::ShiftJis.is_continuation_byte(&sjis_bytes, 2));
        assert!(Encoding::ShiftJis.is_continuation_byte(&sjis_bytes, 3));

        // Character range check
        assert_eq!(Encoding::ShiftJis.char_range_at(&sjis_bytes, 0), 0..2);
        assert_eq!(Encoding::ShiftJis.char_range_at(&sjis_bytes, 1), 0..2);
        assert_eq!(Encoding::ShiftJis.char_range_at(&sjis_bytes, 2), 2..4);
        assert_eq!(Encoding::ShiftJis.char_range_at(&sjis_bytes, 3), 2..4);
    }

    #[test]
    fn test_char_range_at_various_encodings() {
        // UTF-8 "こA" -> [0xE3, 0x81, 0x93, 0x41]
        let utf8 = [0xE3, 0x81, 0x93, 0x41];
        assert_eq!(Encoding::Utf8.char_range_at(&utf8, 0), 0..3);
        assert_eq!(Encoding::Utf8.char_range_at(&utf8, 1), 0..3);
        assert_eq!(Encoding::Utf8.char_range_at(&utf8, 2), 0..3);
        assert_eq!(Encoding::Utf8.char_range_at(&utf8, 3), 3..4);

        // ASCII "ABC"
        let ascii = b"ABC";
        assert_eq!(Encoding::Ascii.char_range_at(ascii, 0), 0..1);
        assert_eq!(Encoding::Ascii.char_range_at(ascii, 1), 1..2);

        // Shift-JIS "ここ" -> [0x82, 0xB1, 0x82, 0xB1]
        let sjis = [0x82, 0xB1, 0x82, 0xB1];
        assert_eq!(Encoding::ShiftJis.next_char_boundary(&sjis, 0), 2);
        assert_eq!(Encoding::ShiftJis.next_char_boundary(&sjis, 1), 2);
        assert_eq!(Encoding::ShiftJis.next_char_boundary(&sjis, 2), 4);
        assert_eq!(Encoding::ShiftJis.prev_char_boundary(&sjis, 4), 2);
        assert_eq!(Encoding::ShiftJis.prev_char_boundary(&sjis, 2), 0);
        assert_eq!(Encoding::ShiftJis.prev_char_boundary(&sjis, 1), 0);
        assert_eq!(Encoding::ShiftJis.prev_char_boundary(&sjis, 0), 0);
    }

    #[test]
    fn test_iso8859_family_decode_encode() {
        // ISO-8859-1: 0xE9 = 'é', 0xA9 = '©'
        let latin1_bytes = [0xE9, 0xA9, 0x41];
        assert_eq!(Encoding::Iso8859_1.decode_char_at(&latin1_bytes, 0), Some(('é', 1)));
        assert_eq!(Encoding::Iso8859_1.decode_char_at(&latin1_bytes, 1), Some(('©', 1)));
        assert_eq!(Encoding::Iso8859_1.decode_char_at(&latin1_bytes, 2), Some(('A', 1)));
        assert_eq!(Encoding::Iso8859_1.encode_char('é'), Some(vec![0xE9]));
        assert_eq!(Encoding::Iso8859_1.encode_char('©'), Some(vec![0xA9]));

        // ISO-8859-15: 0xA4 = '€'
        let latin9_bytes = [0xA4];
        assert_eq!(Encoding::Iso8859_15.decode_char_at(&latin9_bytes, 0), Some(('€', 1)));
        assert_eq!(Encoding::Iso8859_15.encode_char('€'), Some(vec![0xA4]));

        // Windows-1252: 0x80 = '€'
        let win1252_bytes = [0x80];
        assert_eq!(Encoding::Windows1252.decode_char_at(&win1252_bytes, 0), Some(('€', 1)));
        assert_eq!(Encoding::Windows1252.encode_char('€'), Some(vec![0x80]));
    }

    #[test]
    fn test_euc_jp_and_gbk() {
        // EUC-JP "日" = 0xC6 0xFC
        let euc_jp = [0xC6, 0xFC];
        assert_eq!(Encoding::EucJp.decode_char_at(&euc_jp, 0), Some(('日', 2)));
        assert_eq!(Encoding::EucJp.encode_char('日'), Some(vec![0xC6, 0xFC]));

        // GBK "中" = 0xD6 0xD0
        let gbk = [0xD6, 0xD0];
        assert_eq!(Encoding::Gbk.decode_char_at(&gbk, 0), Some(('中', 2)));
        assert_eq!(Encoding::Gbk.encode_char('中'), Some(vec![0xD6, 0xD0]));
    }

    #[test]
    fn test_encoding_encode_char() {
        assert_eq!(Encoding::Ascii.encode_char('A'), Some(vec![0x41]));
        assert_eq!(Encoding::Ascii.encode_char('あ'), None);
        assert_eq!(Encoding::Utf8.encode_char('あ'), Some("あ".as_bytes().to_vec()));
        assert_eq!(Encoding::Utf16Le.encode_char('A'), Some(vec![0x41, 0x00]));
        assert_eq!(Encoding::Utf16Be.encode_char('A'), Some(vec![0x00, 0x41]));
        assert_eq!(Encoding::Utf16Le.encode_char('😀'), Some(vec![0x3d, 0xd8, 0x00, 0xde]));
        assert_eq!(Encoding::Utf16Be.encode_char('😀'), Some(vec![0xd8, 0x3d, 0xde, 0x00]));
    }

    #[test]
    fn test_is_continuation_byte() {
        let utf8_bytes = "こんにちは".as_bytes();
        assert!(!Encoding::Utf8.is_continuation_byte(utf8_bytes, 0));
        assert!(Encoding::Utf8.is_continuation_byte(utf8_bytes, 1));
        assert!(Encoding::Utf8.is_continuation_byte(utf8_bytes, 2));
        assert!(!Encoding::Utf8.is_continuation_byte(utf8_bytes, 3));

        let ascii_bytes = b"Hello";
        assert!(!Encoding::Ascii.is_continuation_byte(ascii_bytes, 0));
        assert!(!Encoding::Ascii.is_continuation_byte(ascii_bytes, 1));
    }

    #[test]
    fn test_from_name_resolution() {
        assert_eq!(Encoding::from_name("shift_jis"), Some(Encoding::ShiftJis));
        assert_eq!(Encoding::from_name("sjis"), Some(Encoding::ShiftJis));
        assert_eq!(Encoding::from_name("cp932"), Some(Encoding::ShiftJis));
        assert_eq!(Encoding::from_name("windows-31j"), Some(Encoding::ShiftJis));
        assert_eq!(Encoding::from_name("latin1"), Some(Encoding::Iso8859_1));
        assert_eq!(Encoding::from_name("ISO-8859-1"), Some(Encoding::Iso8859_1));
        assert_eq!(Encoding::from_name("iso-8859-15"), Some(Encoding::Iso8859_15));
        assert_eq!(Encoding::from_name("windows-1252"), Some(Encoding::Windows1252));
        assert_eq!(Encoding::from_name("cp1252"), Some(Encoding::Windows1252));
        assert_eq!(Encoding::from_name("gbk"), Some(Encoding::Gbk));
        assert_eq!(Encoding::from_name("big5"), Some(Encoding::Big5));
        assert_eq!(Encoding::from_name("euc-jp"), Some(Encoding::EucJp));
        assert_eq!(Encoding::from_name("utf-8"), Some(Encoding::Utf8));
        assert_eq!(Encoding::from_name("unknown_nonexistent"), None);
    }

    #[test]
    fn test_serde_roundtrip() {
        for enc in Encoding::all() {
            let json = serde_json::to_string(enc).expect("serialize encoding");
            let deserialized: Encoding = serde_json::from_str(&json).expect("deserialize encoding");
            assert_eq!(*enc, deserialized);
        }
    }

    #[test]
    fn test_encoding_encode_str() {
        // UTF-8
        assert_eq!(Encoding::Utf8.encode_str("Hello こんにちは"), Some("Hello こんにちは".as_bytes().to_vec()));
        assert_eq!(Encoding::Utf8.encode_str(""), Some(Vec::new()));

        // ASCII
        assert_eq!(Encoding::Ascii.encode_str("Hello World!"), Some(b"Hello World!".to_vec()));
        assert_eq!(Encoding::Ascii.encode_str("こんにちは"), None);

        // ISO-8859-1
        assert_eq!(Encoding::Iso8859_1.encode_str("Café"), Some(vec![b'C', b'a', b'f', 0xE9]));
        assert_eq!(Encoding::Iso8859_1.encode_str("こんにちは"), None);

        // UTF-16 LE & BE
        assert_eq!(Encoding::Utf16Le.encode_str("AB"), Some(vec![0x41, 0x00, 0x42, 0x00]));
        assert_eq!(Encoding::Utf16Be.encode_str("AB"), Some(vec![0x00, 0x41, 0x00, 0x42]));

        // Shift-JIS
        let sjis_bytes = Encoding::ShiftJis.encode_str("こんにちは").unwrap();
        assert_eq!(sjis_bytes, vec![0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD]);
        assert_eq!(Encoding::ShiftJis.encode_str("Hello"), Some(b"Hello".to_vec()));
        assert_eq!(Encoding::ShiftJis.encode_str("€"), None);

        // EUC-JP & GBK
        assert_eq!(Encoding::EucJp.encode_str("日本"), Some(vec![0xC6, 0xFC, 0xCB, 0xDC]));
        assert_eq!(Encoding::Gbk.encode_str("中文"), Some(vec![0xD6, 0xD0, 0xCE, 0xC4]));
    }

    #[test]
    fn test_encoding_format_preview() {
        // ASCII
        let ascii_buf = b"Hello, World!";
        assert_eq!(Encoding::Ascii.format_preview(ascii_buf, 0, 5), "Hello");
        assert_eq!(Encoding::Ascii.format_preview(ascii_buf, 7, 5), "World");

        // Shift-JIS
        let sjis_buf = [0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD, 0x41, 0x42];
        assert_eq!(Encoding::ShiftJis.format_preview(&sjis_buf, 0, 10), "こんにちは");
        assert_eq!(Encoding::ShiftJis.format_preview(&sjis_buf, 10, 2), "AB");

        // UTF-16 LE
        let utf16_buf = [0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00];
        assert_eq!(Encoding::Utf16Le.format_preview(&utf16_buf, 0, 10), "Hello");

        // Unprintable / Invalid bytes replaced by '.'
        let mixed_buf = [0x00, 0x41, 0xFF, 0x42];
        assert_eq!(Encoding::Ascii.format_preview(&mixed_buf, 0, 4), ".A.B");
    }
}
