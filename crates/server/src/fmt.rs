use chrono::{Datelike, NaiveDate};

pub fn date(d: Option<NaiveDate>) -> String {
    match d {
        Some(d) => date_naive(d),
        None => "?".to_string(),
    }
}

fn date_naive(d: NaiveDate) -> String {
    format!(
        "{:02} {} {}",
        d.day(),
        crate::i18n::month_abbr(d.month()),
        d.year()
    )
}

pub fn date_range(start: Option<NaiveDate>, end: Option<NaiveDate>) -> String {
    let present = crate::i18n::t("present");
    match (start.map(date_naive), end.map(date_naive)) {
        (Some(s), Some(e)) => format!("{s} - {e}"),
        (Some(s), None) => format!("{s} - {present}"),
        (None, Some(e)) => format!("? - {e}"),
        (None, None) => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    // The active locale defaults to Turkish, so months and "present" are Turkish.
    #[test]
    fn date_formats_or_question_mark() {
        assert_eq!(date(None), "?");
        assert_eq!(date(Some(d(2020, 6, 12))), "12 Haz 2020");
    }

    #[test]
    fn date_range_covers_every_case() {
        assert_eq!(
            date_range(Some(d(2019, 1, 1)), Some(d(2023, 3, 4))),
            "01 Oca 2019 - 04 Mar 2023"
        );
        assert!(date_range(Some(d(2019, 1, 1)), None).ends_with(" - günümüz"));
        assert!(date_range(None, Some(d(2023, 1, 1))).starts_with("? - "));
        assert_eq!(date_range(None, None), "?");
    }
}
