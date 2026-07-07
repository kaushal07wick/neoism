fn join_words<'a>(words: impl IntoIterator<Item = &'a str>) -> String {
    words
        .into_iter()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn edit_smoke_joins_words() {
    let message = join_words([" neoism ", "", "edit", " smoke"]);

    assert_eq!(message, "neoism edit smoke");
}

#[test]
fn edit_smoke_ignores_empty_words() {
    let message = join_words(["", "  ", "tools", "", " work "]);

    assert_eq!(message, "tools work");
}
