//! URL slug generation.
//!
//! Slugs are ASCII, lowercase and hyphen-separated. Turkish letters are
//! transliterated explicitly (ç→c, ğ→g, ı/İ→i, ö→o, ş→s, ü→u) rather than
//! relying on `to_lowercase()`, whose handling of the dotted/dotless i is wrong
//! for Turkish. Characters that are neither ASCII alphanumerics nor a mapped
//! letter act as separators.

/// Build a URL slug from arbitrary text.
///
/// ```
/// use domain::slug::slugify;
/// assert_eq!(slugify("İsmet Çağöz"), "ismet-cagoz");
/// assert_eq!(slugify("Işık"), "isik");
/// ```
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_separator = false;

    for ch in input.chars() {
        match map_char(ch) {
            Some(mapped) => {
                if pending_separator && !out.is_empty() {
                    out.push('-');
                }
                pending_separator = false;
                out.push(mapped);
            }
            None => pending_separator = true,
        }
    }

    out
}

/// Map a character to its slug form, or `None` if it acts as a separator.
fn map_char(ch: char) -> Option<char> {
    match ch {
        'ç' | 'Ç' => Some('c'),
        'ğ' | 'Ğ' => Some('g'),
        'ı' | 'İ' => Some('i'),
        'ö' | 'Ö' => Some('o'),
        'ş' | 'Ş' => Some('s'),
        'ü' | 'Ü' => Some('u'),
        'a'..='z' | '0'..='9' => Some(ch),
        'A'..='Z' => Some(ch.to_ascii_lowercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn transliterates_turkish_letters() {
        assert_eq!(slugify("çğıöşü"), "cgiosu");
        assert_eq!(slugify("ÇĞİÖŞÜ"), "cgiosu");
    }

    #[test]
    fn dotted_and_dotless_i() {
        // ASCII I, dotless ı, dotted İ and dotted i all become ASCII i.
        assert_eq!(slugify("Işık"), "isik");
        assert_eq!(slugify("İzmir"), "izmir");
        assert_eq!(slugify("Iğdır"), "igdir");
    }

    #[test]
    fn full_names() {
        assert_eq!(slugify("Kerem Ağaoğlu"), "kerem-agaoglu");
        assert_eq!(slugify("Ayşe Yılmaz"), "ayse-yilmaz");
    }

    #[test]
    fn collapses_and_trims_separators() {
        assert_eq!(slugify("  Test   Partisi!  "), "test-partisi");
        assert_eq!(slugify("A.K. Partisi"), "a-k-partisi");
        assert_eq!(slugify("---"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn keeps_digits() {
        assert_eq!(slugify("28. Dönem"), "28-donem");
    }
}
