/// Mapping of high byte values (0x80..=0xFF) for IBM Code Page 437 (OEM-ASCII).
const CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', // 80-87
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}', // 88-8F
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}', // 90-97
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}', // 98-9F
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', // A0-A7
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}', // A8-AF
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', // B0-B7
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', // B8-BF
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}', // C0-C7
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', // C8-CF
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', // D0-D7
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}', // D8-DF
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', // E0-E7
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', // E8-EF
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}', // F0-F7
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}', // F8-FF
];

/// Decodes a byte sequence into a Rust `String` according to the requested Kaitai Struct encoding.
///
/// Fully conforms to Kaitai Struct canonical encodings and aliases defined in `EncodingList.scala`,
/// including Unicode (UTF-8, UTF-16LE/BE, UTF-32LE/BE), ASCII, ISO-8859-(1..16),
/// Windows-1250..1258, IBM437 (DOS), IBM866, Shift_JIS, EUC-JP, ISO-2022-JP, GBK, GB18030, Big5, EUC-KR, etc.
pub fn decode_bytes(bytes: &[u8], encoding: &str) -> String {
    let raw = encoding.trim();
    let norm = raw.to_ascii_uppercase().replace('_', "-");

    match norm.as_str() {
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "LATIN-1" | "L1" | "CSISOLATIN1" | "ISO-IR-100" | "IBM819" | "CP819" | "WINDOWS-28591" => {
            // ISO-8859-1: direct byte value to unicode code point mapping (0x00..=0xFF)
            bytes.iter().map(|&b| b as char).collect()
        }
        "IBM437" | "CP437" | "437" | "CSPC8CODEPAGE437" => {
            // IBM PC OEM Code Page 437
            bytes
                .iter()
                .map(|&b| if b < 0x80 { b as char } else { CP437_HIGH[(b - 0x80) as usize] })
                .collect()
        }
        "UTF-16LE" | "UTF16LE" | "UTF16-LE" | "UTF-16-LE" | "CSUTF16LE" | "UCS-2LE" | "UCS2LE" => {
            let (chunks, _) = bytes.as_chunks::<2>();
            let u16s: Vec<u16> = chunks.iter().map(|&c| u16::from_le_bytes(c)).collect();
            String::from_utf16_lossy(&u16s)
        }
        "UTF-16BE" | "UTF16BE" | "UTF16-BE" | "UTF-16-BE" | "CSUTF16BE" | "UCS-2BE" | "UCS2BE" => {
            let (chunks, _) = bytes.as_chunks::<2>();
            let u16s: Vec<u16> = chunks.iter().map(|&c| u16::from_be_bytes(c)).collect();
            String::from_utf16_lossy(&u16s)
        }
        "UTF-32LE" | "UTF32LE" | "UTF32-LE" | "UTF-32-LE" | "CSUTF32LE" => {
            let (chunks, _) = bytes.as_chunks::<4>();
            chunks
                .iter()
                .map(|&c| {
                    let code = u32::from_le_bytes(c);
                    char::from_u32(code).unwrap_or('\u{FFFD}')
                })
                .collect()
        }
        "UTF-32BE" | "UTF32BE" | "UTF32-BE" | "UTF-32-BE" | "CSUTF32BE" => {
            let (chunks, _) = bytes.as_chunks::<4>();
            chunks
                .iter()
                .map(|&c| {
                    let code = u32::from_be_bytes(c);
                    char::from_u32(code).unwrap_or('\u{FFFD}')
                })
                .collect()
        }
        "ASCII" | "US-ASCII" | "IBM367" | "CP367" | "CSASCII" | "ISO-IR-6" | "RAW" => {
            bytes.iter().map(|&b| if b < 128 { b as char } else { '\u{FFFD}' }).collect()
        }
        "UTF-8" | "UTF8" | "ISO-10646/UTF-8" | "ISO-10646/UTF8" | "CP65001" | "CSUTF8" | "UNICODE-1-1-UTF-8" | "UNICODE-2-0-UTF-8" => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        "SHIFT-JIS" | "SHIFTJIS" | "S-JIS" | "SJIS" | "PCK" | "CSSHIFTJIS" | "CP932" | "WINDOWS-31J" | "MS932" | "X-SJIS" => {
            encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
        }
        "EUC-JP" | "EUCJP" | "UJIS" | "CSEUCJP" => encoding_rs::EUC_JP.decode(bytes).0.into_owned(),
        "ISO-2022-JP" | "ISO2022JP" | "CSISO2022JP" => encoding_rs::ISO_2022_JP.decode(bytes).0.into_owned(),
        "GBK" | "CP936" | "GB2312" | "CHINESE" | "CSGB2312" => encoding_rs::GBK.decode(bytes).0.into_owned(),
        "GB18030" => encoding_rs::GB18030.decode(bytes).0.into_owned(),
        "BIG5" | "CSBIG5" | "CP950" | "BIG5-HKSCS" => encoding_rs::BIG5.decode(bytes).0.into_owned(),
        "EUC-KR" | "EUCKR" | "CSEUCKR" | "KOREAN" | "ISO-IR-149" | "CP949" => encoding_rs::EUC_KR.decode(bytes).0.into_owned(),
        "IBM866" | "CP866" | "866" | "CSIBM866" => encoding_rs::IBM866.decode(bytes).0.into_owned(),
        "KOI8-R" | "KOI8R" | "CSKOI8R" => encoding_rs::KOI8_R.decode(bytes).0.into_owned(),
        "KOI8-U" | "KOI8U" => encoding_rs::KOI8_U.decode(bytes).0.into_owned(),
        "MACINTOSH" | "MAC-ROMAN" | "MACROMAN" | "MAC" | "CSMACINTOSH" => encoding_rs::MACINTOSH.decode(bytes).0.into_owned(),
        _ => {
            if let Some(enc) = encoding_rs::Encoding::for_label(raw.as_bytes()).or_else(|| encoding_rs::Encoding::for_label(norm.as_bytes())) {
                enc.decode(bytes).0.into_owned()
            } else if let Some(xenc) = crate::core::encoding::Encoding::from_name(raw) {
                if let Some(enc) = xenc.encoding_rs_ref() {
                    enc.decode(bytes).0.into_owned()
                } else {
                    String::from_utf8_lossy(bytes).into_owned()
                }
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    }
}
