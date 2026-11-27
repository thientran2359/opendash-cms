mod head_and_seo;
mod normalize;
mod types;

pub use head_and_seo::*;
pub use normalize::*;
pub use types::*;

/// Returns `true` when normalized HTML markup contains any `<form ...>` open tag.
pub fn contains_html_form_tag(html: &str) -> bool {
    let Ok(tokens) = normalize::tokenize(html, true) else {
        return false;
    };
    tokens.into_iter().any(|(kind, name_lower)| {
        matches!(kind, types::TokenKind::OpenTag { .. }) && name_lower == "form"
    })
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
mod search_form_tag_tests {
    use super::contains_html_form_tag;

    #[test]
    fn detects_form_open_tags_and_ignores_text_or_comments() {
        assert!(contains_html_form_tag(
            r#"<main><form method="get" action="/"></form></main>"#
        ));
        assert!(contains_html_form_tag(
            r#"<SECTION><FORM action="/en"></FORM></SECTION>"#
        ));
        assert!(!contains_html_form_tag(
            r#"<main><p>&lt;form&gt; literal</p></main>"#
        ));
        assert!(!contains_html_form_tag(r#"<main><!-- <form> --></main>"#));
        assert!(!contains_html_form_tag(""));
    }
}
