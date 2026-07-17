use domain::models::{SearchHit, SearchKind};

use crate::{Pool, Result};

/// Full-text search across people and parties, people first. Matches on word
/// prefixes, so typing part of a name ("rece") finds "Recep".
pub async fn search(pool: &Pool, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
    let ts = prefix_tsquery(query);
    if ts.is_empty() {
        return Ok(Vec::new());
    }

    let people = sqlx::query!(
        r#"
        select full_name, slug
        from people
        where fts @@ to_tsquery('simple', $1)
        order by full_name
        limit $2
        "#,
        ts,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let parties = sqlx::query!(
        r#"
        select name, slug
        from parties
        where fts @@ to_tsquery('simple', $1)
        order by name
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
        })
        .collect();
    hits.extend(parties.into_iter().map(|r| SearchHit {
        kind: SearchKind::Party,
        name: r.name,
        slug: r.slug,
    }));
    Ok(hits)
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
