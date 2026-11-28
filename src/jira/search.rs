pub const ALL_ISSUES_LABEL: &str = "All Issues";
pub const ALL_ISSUES_JQL: &str = "updated <= 365d ORDER BY updated DESC";

pub fn search_jql(base: &str, query: &str) -> Option<String> {
    let clauses: Vec<String> = query.split_whitespace().filter_map(word_clause).collect();
    if clauses.is_empty() {
        return None;
    }
    let (filter, order) = split_order_by(base);
    let filter = filter.trim();
    let mut jql = if filter.is_empty() {
        clauses.join(" AND ")
    } else {
        format!("({filter}) AND {}", clauses.join(" AND "))
    };
    if let Some(order) = order {
        jql.push(' ');
        jql.push_str(order.trim());
    }
    Some(jql)
}

fn word_clause(word: &str) -> Option<String> {
    let clean: String = word
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if clean.chars().count() < 2 {
        return None;
    }
    let mut options = vec![
        format!("text ~ \"{clean}*\""),
        format!("labels = \"{clean}\""),
    ];
    if looks_like_key(&clean) {
        options.push(format!("key = \"{}\"", clean.to_uppercase()));
    }
    Some(format!("({})", options.join(" OR ")))
}

fn split_order_by(jql: &str) -> (&str, Option<&str>) {
    match jql.to_lowercase().rfind("order by") {
        Some(i) => (&jql[..i], Some(&jql[i..])),
        None => (jql, None),
    }
}

fn looks_like_key(word: &str) -> bool {
    let Some((project, number)) = word.rsplit_once('-') else {
        return false;
    };
    project.starts_with(|c: char| c.is_alphabetic())
        && project.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "assignee = currentUser() ORDER BY updated DESC";

    #[test]
    fn one_word_matches_text_or_a_label_inside_the_configured_jql() {
        assert_eq!(
            search_jql(BASE, "login").unwrap(),
            "(assignee = currentUser()) AND (text ~ \"login*\" OR labels = \"login\") \
             ORDER BY updated DESC"
        );
    }

    #[test]
    fn every_word_has_to_match() {
        let jql = search_jql("project = MAPLE", "login bug").unwrap();
        assert_eq!(
            jql,
            "(project = MAPLE) AND (text ~ \"login*\" OR labels = \"login\") \
             AND (text ~ \"bug*\" OR labels = \"bug\")"
        );
    }

    #[test]
    fn punctuation_is_stripped_rather_than_escaped() {
        // an unbalanced quote would otherwise break the whole query
        let jql = search_jql("project = MAPLE", "lo\"gin").unwrap();
        assert!(jql.contains("text ~ \"login*\""), "{jql}");
        assert_eq!(
            jql.matches('"').count(),
            4,
            "no stray quote survives: {jql}"
        );
    }

    #[test]
    fn words_too_short_to_search_are_dropped() {
        let jql = search_jql("project = MAPLE", "a login").unwrap();
        assert!(!jql.contains("\"a*\""), "{jql}");
        assert_eq!(jql.matches(" AND ").count(), 1, "{jql}");

        // nothing usable left, so the caller falls back to the plain list
        assert!(search_jql(BASE, "a").is_none());
        assert!(search_jql(BASE, "  ").is_none());
        assert!(search_jql(BASE, "!!").is_none());
    }

    #[test]
    fn an_empty_base_leaves_no_dangling_parentheses() {
        assert_eq!(
            search_jql("", "login").unwrap(),
            "(text ~ \"login*\" OR labels = \"login\")"
        );
        assert_eq!(
            search_jql("ORDER BY created ASC", "login").unwrap(),
            "(text ~ \"login*\" OR labels = \"login\") ORDER BY created ASC"
        );
    }

    #[test]
    fn a_base_with_or_is_parenthesised_so_and_cannot_capture_one_side() {
        let jql = search_jql("project = MAPLE OR project = BIRCH", "login").unwrap();
        assert!(
            jql.starts_with("(project = MAPLE OR project = BIRCH) AND "),
            "{jql}"
        );
    }

    #[test]
    fn a_key_like_word_also_matches_the_key_itself() {
        let jql = search_jql("project = MAPLE", "abc-123").unwrap();
        assert!(jql.contains("key = \"ABC-123\""), "uppercased: {jql}");

        let jql = search_jql("project = MAPLE", "login").unwrap();
        assert!(!jql.contains("key = "), "{jql}");
    }

    #[test]
    fn looks_like_key_wants_letters_a_dash_and_digits() {
        for key in ["ABC-123", "abc-1", "PROJ_2-45"] {
            assert!(looks_like_key(key), "{key} is an issue key");
        }
        for word in ["ABC", "123", "ABC-", "-12", "ABC-12a", "", "-"] {
            assert!(!looks_like_key(word), "{word:?} is not an issue key");
        }
    }
}
