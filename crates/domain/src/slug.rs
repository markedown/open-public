//! URL slug generation.
//!
//! Slugs are ASCII, lowercase and hyphen-separated. Letters outside ASCII are
//! transliterated explicitly rather than through `to_lowercase()`, whose
//! handling of the Turkish dotted and dotless i is wrong: ç→c, ğ→g, ı/İ→i,
//! ö→o, ş→s, ü→u. Serbian Latin follows the same scheme (č/ć→c, đ→dj, š→s,
//! ž→z), Serbian Cyrillic is first romanized letter by letter (љ→lj, њ→nj,
//! џ→dz), and the common Western European accents drop to their base letter.
//! Characters that are neither ASCII alphanumerics nor a mapped letter act as
//! separators.

/// Build a URL slug from arbitrary text.
///
/// ```
/// use domain::slug::slugify;
/// assert_eq!(slugify("İsmet Çağöz"), "ismet-cagoz");
/// assert_eq!(slugify("Işık"), "isik");
/// assert_eq!(slugify("Đorđe Šćekić"), "djordje-scekic");
/// assert_eq!(slugify("Љиљана Његош"), "ljiljana-njegos");
/// ```
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_separator = false;

    for ch in input.chars() {
        let mut buf = [0u8; 4];
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(&*ch.to_ascii_lowercase().encode_utf8(&mut buf))
        } else {
            map_char(ch)
        };
        match mapped {
            Some(mapped) => {
                if pending_separator && !out.is_empty() {
                    out.push('-');
                }
                pending_separator = false;
                out.push_str(mapped);
            }
            None => pending_separator = true,
        }
    }

    out
}

/// Map a non-ASCII character to its slug form, or `None` if it acts as a
/// separator.
fn map_char(ch: char) -> Option<&'static str> {
    // The Turkish i-forms come first: Unicode lowercases İ to "i" plus a
    // combining dot, which would otherwise end up as a separator.
    if matches!(ch, 'ı' | 'İ') {
        return Some("i");
    }
    // Every remaining mapped letter has a single-character lowercase form,
    // so one table of lowercase letters covers both cases.
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    let mapped = match lower {
        // Turkish
        'ç' => "c",
        'ğ' => "g",
        'ö' => "o",
        'ş' => "s",
        'ü' => "u",
        // Serbian Latin (also Croatian, Bosnian, Montenegrin)
        'č' | 'ć' => "c",
        'đ' => "dj",
        'š' => "s",
        'ž' => "z",
        // Serbian Cyrillic, romanized as Serbian Latin, then as above
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'ђ' => "dj",
        'е' => "e",
        'ж' | 'з' => "z",
        'и' => "i",
        'ј' => "j",
        'к' => "k",
        'л' => "l",
        'љ' => "lj",
        'м' => "m",
        'н' => "n",
        'њ' => "nj",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' | 'ш' => "s",
        'т' => "t",
        'ћ' | 'ц' | 'ч' => "c",
        'у' => "u",
        'ф' => "f",
        'х' => "h",
        'џ' => "dz",
        // Western European accents
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ò' | 'ó' | 'ô' | 'õ' | 'ø' => "o",
        'ù' | 'ú' | 'û' => "u",
        'ý' | 'ÿ' => "y",
        'ñ' => "n",
        'æ' => "ae",
        'œ' => "oe",
        'ß' => "ss",
        _ => return None,
    };
    Some(mapped)
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
    fn transliterates_serbian_latin() {
        assert_eq!(slugify("čćđšž"), "ccdjsz");
        assert_eq!(slugify("ČĆĐŠŽ"), "ccdjsz");
        assert_eq!(slugify("Đorđe Šćekić"), "djordje-scekic");
    }

    #[test]
    fn romanizes_serbian_cyrillic() {
        assert_eq!(
            slugify("абвгдђежзијклљмнњопрстћуфхцчџш"),
            "abvgddjezzijklljmnnjoprstcufhccdzs"
        );
        assert_eq!(
            slugify("АБВГДЂЕЖЗИЈКЛЉМНЊОПРСТЋУФХЦЧЏШ"),
            "abvgddjezzijklljmnnjoprstcufhccdzs"
        );
        // A name written in either script gets the same slug.
        assert_eq!(slugify("Јелена Ђурић"), slugify("Jelena Đurić"));
        assert_eq!(slugify("Народна скупштина"), "narodna-skupstina");
    }

    #[test]
    fn drops_western_european_accents() {
        assert_eq!(
            slugify("àáâãäå èéêë ìíîï òóôõø ùúû ýÿ ñ"),
            "aaaaaa-eeee-iiii-ooooo-uuu-yy-n"
        );
        assert_eq!(slugify("ÀÉÎÕÛ"), "aeiou");
        assert_eq!(slugify("Æsir Œuvre Straße"), "aesir-oeuvre-strasse");
        assert_eq!(slugify("Sébastien"), "sebastien");
    }

    #[test]
    fn unmapped_scripts_are_separators() {
        assert_eq!(slugify("Test 東京 Partisi"), "test-partisi");
        assert_eq!(slugify("Ελλάδα"), "");
    }

    #[test]
    fn keeps_digits() {
        assert_eq!(slugify("28. Dönem"), "28-donem");
    }
}
