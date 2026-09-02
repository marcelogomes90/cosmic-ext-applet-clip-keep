pub fn filter<'a>(haystacks: impl IntoIterator<Item = &'a str>, query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();

    haystacks
        .into_iter()
        .enumerate()
        .filter_map(|(index, text)| {
            (needle.is_empty() || text.to_lowercase().contains(&needle)).then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [&str; 4] = [
        "https://github.com/pop-os/libcosmic",
        "fn main() { println!(\"hi\") }",
        "marcelo@exemplo.com",
        "GITHUB TOKEN ghp_abcdef",
    ];

    #[test]
    fn an_empty_query_keeps_everything() {
        assert_eq!(filter(ROWS, "").len(), ROWS.len());
        assert_eq!(filter(ROWS, "   ").len(), ROWS.len());
    }

    #[test]
    fn a_query_ignores_case_on_both_sides() {
        assert_eq!(filter(ROWS, "github"), vec![0, 3]);
    }

    #[test]
    fn matches_keep_their_place_in_the_history() {
        assert_eq!(filter(ROWS, "GITHUB"), vec![0, 3]);
        assert!(filter(ROWS, "zzzz").is_empty());
    }
}
