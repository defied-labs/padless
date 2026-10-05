use std::fmt;

use serde::Deserialize;

use crate::code::Code;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Table {
    #[default]
    Windows,
    Unicode,
}

impl Table {
    #[must_use]
    pub fn resolve(self, code: &Code) -> Option<char> {
        match self {
            Self::Windows => windows_char(code),
            Self::Unicode => unicode_char(code),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Unicode => "unicode",
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

fn windows_char(code: &Code) -> Option<char> {
    let byte = code.low_byte();
    if byte == 0 {
        return None;
    }
    let decoded = if code.has_leading_zero() {
        windows_1252(byte)
    } else {
        cp437(byte)
    };
    Some(decoded)
}

fn unicode_char(code: &Code) -> Option<char> {
    u32::try_from(code.value())
        .ok()
        .filter(|value| *value != 0)
        .and_then(char::from_u32)
}

fn cp437(byte: u8) -> char {
    match byte {
        0x00..=0x1F => CP437_LOW[usize::from(byte)],
        0x7F => '\u{2302}',
        0x80..=0xFF => CP437_HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

fn windows_1252(byte: u8) -> char {
    match byte {
        0x80..=0x9F => WINDOWS_1252_C1[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

static CP437_LOW: [char; 32] = [
    '\u{0000}', '\u{263A}', '\u{263B}', '\u{2665}', '\u{2666}', '\u{2663}', '\u{2660}', '\u{2022}',
    '\u{25D8}', '\u{25CB}', '\u{25D9}', '\u{2642}', '\u{2640}', '\u{266A}', '\u{266B}', '\u{263C}',
    '\u{25BA}', '\u{25C4}', '\u{2195}', '\u{203C}', '\u{00B6}', '\u{00A7}', '\u{25AC}', '\u{21A8}',
    '\u{2191}', '\u{2193}', '\u{2192}', '\u{2190}', '\u{221F}', '\u{2194}', '\u{25B2}', '\u{25BC}',
];

static CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

static WINDOWS_1252_C1: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn resolve(table: Table, text: &str) -> Option<char> {
        table.resolve(&text.parse().unwrap())
    }

    #[test]
    fn windows_codes_without_leading_zero_use_cp437() {
        assert_eq!(resolve(Table::Windows, "1"), Some('\u{263A}'));
        assert_eq!(resolve(Table::Windows, "65"), Some('A'));
        assert_eq!(resolve(Table::Windows, "127"), Some('\u{2302}'));
        assert_eq!(resolve(Table::Windows, "130"), Some('\u{00E9}'));
        assert_eq!(resolve(Table::Windows, "156"), Some('\u{00A3}'));
        assert_eq!(resolve(Table::Windows, "171"), Some('\u{00BD}'));
        assert_eq!(resolve(Table::Windows, "219"), Some('\u{2588}'));
        assert_eq!(resolve(Table::Windows, "227"), Some('\u{03C0}'));
        assert_eq!(resolve(Table::Windows, "248"), Some('\u{00B0}'));
        assert_eq!(resolve(Table::Windows, "255"), Some('\u{00A0}'));
    }

    #[test]
    fn windows_codes_with_leading_zero_use_windows_1252() {
        assert_eq!(resolve(Table::Windows, "065"), Some('A'));
        assert_eq!(resolve(Table::Windows, "0128"), Some('\u{20AC}'));
        assert_eq!(resolve(Table::Windows, "0150"), Some('\u{2013}'));
        assert_eq!(resolve(Table::Windows, "0151"), Some('\u{2014}'));
        assert_eq!(resolve(Table::Windows, "0153"), Some('\u{2122}'));
        assert_eq!(resolve(Table::Windows, "0169"), Some('\u{00A9}'));
        assert_eq!(resolve(Table::Windows, "0233"), Some('\u{00E9}'));
        assert_eq!(resolve(Table::Windows, "0255"), Some('\u{00FF}'));
    }

    #[test]
    fn windows_1252_undefined_positions_map_to_c1_controls() {
        for byte in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
            let text = format!("0{byte}");
            assert_eq!(resolve(Table::Windows, &text), Some(char::from(byte)));
        }
    }

    #[test]
    fn windows_codes_wrap_modulo_256() {
        assert_eq!(resolve(Table::Windows, "321"), Some('A'));
        assert_eq!(resolve(Table::Windows, "0321"), Some('A'));
        assert_eq!(resolve(Table::Windows, "386"), Some('\u{00E9}'));
    }

    #[test]
    fn windows_code_zero_produces_nothing() {
        assert_eq!(resolve(Table::Windows, "0"), None);
        assert_eq!(resolve(Table::Windows, "00"), None);
        assert_eq!(resolve(Table::Windows, "256"), None);
    }

    #[test]
    fn unicode_codes_are_decimal_code_points() {
        assert_eq!(resolve(Table::Unicode, "65"), Some('A'));
        assert_eq!(resolve(Table::Unicode, "8364"), Some('\u{20AC}'));
        assert_eq!(resolve(Table::Unicode, "0233"), Some('\u{00E9}'));
        assert_eq!(resolve(Table::Unicode, "128512"), Some('\u{1F600}'));
        assert_eq!(resolve(Table::Unicode, "1114111"), Some('\u{10FFFF}'));
    }

    #[test]
    fn unicode_rejects_surrogates_out_of_range_and_zero() {
        assert_eq!(resolve(Table::Unicode, "55296"), None);
        assert_eq!(resolve(Table::Unicode, "57343"), None);
        assert_eq!(resolve(Table::Unicode, "1114112"), None);
        assert_eq!(resolve(Table::Unicode, "9999999999"), None);
        assert_eq!(resolve(Table::Unicode, "0"), None);
    }

    #[test]
    fn cp437_tables_contain_no_duplicates_in_high_half() {
        let mut high = CP437_HIGH.to_vec();
        high.sort_unstable();
        high.dedup();
        assert_eq!(high.len(), CP437_HIGH.len());
    }

    proptest! {
        #[test]
        fn windows_resolution_depends_only_on_low_byte_and_leading_zero(
            value in 1u64..9_999_999,
            leading_zero in any::<bool>(),
        ) {
            let base = if leading_zero { format!("0{value}") } else { value.to_string() };
            let shifted_value = value + 256;
            let shifted = if leading_zero {
                format!("0{shifted_value}")
            } else {
                shifted_value.to_string()
            };
            prop_assert_eq!(resolve(Table::Windows, &base), resolve(Table::Windows, &shifted));
        }

        #[test]
        fn windows_resolves_every_nonzero_byte(byte in 1u8..=255, leading_zero in any::<bool>()) {
            let text = if leading_zero { format!("0{byte}") } else { byte.to_string() };
            prop_assert!(resolve(Table::Windows, &text).is_some());
        }

        #[test]
        fn unicode_round_trips_every_scalar_value(c in any::<char>().prop_filter("non-nul", |c| *c != '\0')) {
            prop_assert_eq!(resolve(Table::Unicode, &u32::from(c).to_string()), Some(c));
        }
    }
}
