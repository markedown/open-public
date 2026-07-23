use domain::models::{SearchCountry, SearchHit, SearchKind};

use crate::{Pool, Result};

/// Full-text search across people and parties, people first. Matches on word
/// prefixes, so typing part of a name ("rece") finds "Recep".
pub async fn search(pool: &Pool, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
    let ts = prefix_tsquery(query);
    if ts.is_empty() {
        return Ok(Vec::new());
    }

    // Left joins: a row with no country still belongs in the results, it just
    // has nowhere to link to, and dropping it would quietly hide data.
    let people = sqlx::query!(
        r#"
        select p.full_name, p.slug, c.slug as "country_slug?", c.name as "country_name?"
        from people p
        left join countries c on c.id = p.country_id
        where p.fts @@ to_tsquery('simple', $1)
        order by p.full_name collate "name_sort"
        limit $2
        "#,
        ts,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let parties = sqlx::query!(
        r#"
        select p.name, p.slug, c.slug as "country_slug?", c.name as "country_name?"
        from parties p
        left join countries c on c.id = p.country_id
        where p.fts @@ to_tsquery('simple', $1)
        order by p.name collate "name_sort"
        limit $2
        "#,
        ts,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let mut hits: Vec<SearchHit> = people
        .into_iter()
        .map(|r| SearchHit {
            kind: SearchKind::Person,
            name: r.full_name,
            slug: r.slug,
            country: country_of(r.country_slug, r.country_name),
        })
        .collect();
    hits.extend(parties.into_iter().map(|r| SearchHit {
        kind: SearchKind::Party,
        name: r.name,
        slug: r.slug,
        country: country_of(r.country_slug, r.country_name),
    }));
    Ok(hits)
}

/// Both halves come from the same left-joined row, so they are present or
/// absent together.
fn country_of(slug: Option<String>, name: Option<String>) -> Option<SearchCountry> {
    Some(SearchCountry {
        slug: slug?,
        name: name?,
    })
}

/// Turn a user query into a prefix tsquery: "rece tay" becomes "rece:* & tay:*".
/// Non-alphanumeric characters are dropped per term so the result is always
/// safe to hand to `to_tsquery` (which otherwise errors on its operators).
fn prefix_tsquery(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| {
            term.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}:*"))
        .collect::<Vec<_>>()
        .join(" & ")
}

#[cfg(test)]
mod tests {
    use super::prefix_tsquery;

    #[test]
    fn builds_prefix_query_and_drops_operators() {
        assert_eq!(prefix_tsquery("rece"), "rece:*");
        assert_eq!(prefix_tsquery("recep tay"), "recep:* & tay:*");
        // Operator characters that would break to_tsquery are stripped.
        assert_eq!(prefix_tsquery("a & b | c"), "a:* & b:* & c:*");
        assert_eq!(prefix_tsquery("   "), "");
        assert_eq!(prefix_tsquery("!:&|"), "");
    }
}
