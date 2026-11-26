use super::*;

#[test]
fn content_slot_rules_for_entry_index_and_non_entry() {
    let empty = parts(&[]);
    assert!(compile("<main></main>", &empty, CsecContentKind::Entry).is_err());
    assert!(
        compile(
            "<main>{{ od_content() }}</main>",
            &empty,
            CsecContentKind::Entry
        )
        .is_ok()
    );
    assert!(
        compile(
            "<main>{{ od_content() }}{{ od_content() }}</main>",
            &empty,
            CsecContentKind::Entry
        )
        .is_err()
    );
    assert!(compile("<main></main>", &empty, CsecContentKind::Index).is_ok());
    assert!(
        compile(
            "<main>{{ od_content() }}</main>",
            &empty,
            CsecContentKind::Index
        )
        .is_ok()
    );
    assert!(
        compile(
            "<main>{{ od_content() }}{{ od_content() }}</main>",
            &empty,
            CsecContentKind::Index
        )
        .is_err()
    );
    assert!(compile("<main></main>", &empty, CsecContentKind::NonEntry).is_ok());
    assert!(
        compile(
            "<main>{{ od_content() }}</main>",
            &empty,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
    assert!(
        compile(
            "<div>{{ od_part('header') }}</div>",
            &parts(&[("header", "<b>{{ od_content() }}</b>")]),
            CsecContentKind::Entry
        )
        .is_err()
    );
}

#[test]
fn rejects_cycles_and_unreferenced_cycles() {
    let self_ref = parts(&[("a", "<div>{{ od_part('a') }}</div>")]);
    assert!(
        compile(
            "<main>{{ od_part('a') }}</main>",
            &self_ref,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
    let indirect = parts(&[("a", "{{ od_part('b') }}"), ("b", "{{ od_part('a') }}")]);
    assert!(compile("<main></main>", &indirect, CsecContentKind::NonEntry).is_err());
}

#[test]
fn rejects_missing_part_and_excess_depth() {
    assert!(
        compile(
            "<main>{{ od_part('missing') }}</main>",
            &parts(&[]),
            CsecContentKind::NonEntry
        )
        .is_err()
    );
    let valid = parts(&[
        ("a", "{{ od_part('b') }}"),
        ("b", "{{ od_part('c') }}"),
        ("c", "<b>end</b>"),
    ]);
    assert!(
        compile(
            "<main>{{ od_part('a') }}</main>",
            &valid,
            CsecContentKind::NonEntry
        )
        .is_ok()
    );
    let too_deep = parts(&[
        ("a", "{{ od_part('b') }}"),
        ("b", "{{ od_part('c') }}"),
        ("c", "{{ od_part('d') }}"),
        ("d", "end"),
    ]);
    assert!(
        compile(
            "<main>{{ od_part('a') }}</main>",
            &too_deep,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
}

#[test]
fn rejects_more_than_32_expansions_even_with_shared_subgraphs() {
    let repeat = "{{ od_part('leaf') }}".repeat(33);
    let nodes = parts(&[("leaf", "<span>safe</span>")]);
    assert!(
        compile(
            &format!("<main>{repeat}</main>"),
            &nodes,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
    assert!(
        compile(
            &format!("<main>{}</main>", "{{ od_part('leaf') }}".repeat(32)),
            &nodes,
            CsecContentKind::NonEntry
        )
        .is_ok()
    );
    let repeated = parts(&[
        ("a", "{{ od_part('leaf') }}{{ od_part('leaf') }}"),
        ("leaf", "<b>x</b>"),
    ]);
    assert!(
        compile(
            &format!("<main>{}</main>", "{{ od_part('a') }}".repeat(11)),
            &repeated,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
}

#[test]
fn rejects_composition_in_loop_attribute_comment_style_and_set_block() {
    let nodes = parts(&[("header", "<b>OK</b>")]);
    let bad = [
        "<main>{% for n in [1,2] %}{{ od_part('header') }}{% endfor %}</main>",
        "<main>{% for n in [1,2] %}{{ od_content() }}{% endfor %}</main>",
        "<a href=\"{{ od_part('header') }}\">x</a>",
        "<div data-test=\"{{ od_content() }}\"></div>",
        "<!-- {{ od_part('header') }} -->",
        "<style>.x{color:{{ od_content() }}}</style>",
        "{% set value %}{{ od_part('header') }}{% endset %}",
    ];
    for source in bad {
        assert!(
            compile(source, &nodes, CsecContentKind::Index).is_err(),
            "accepted unsafe {source:?}"
        );
    }
}

#[test]
fn rejects_branch_split_attribute_and_comment_context_for_all_runtime_paths() {
    let nodes = parts(&[("header", "<b>Header</b>")]);
    let bad = [
        // Previously the false branch's quote + '>' closed the tag in a
        // linear source scan, while the true branch emitted an OPEN id
        // attribute with od_content as its value.
        "{% if cond %}<div id=\"{% else %}\">{% endif %}{{ od_content() }}\">",
        "{% if cond %}<div id=\"{% else %}\">{% endif %}{{ od_part('header') }}\">",
        "{% if outer %}{% if inner %}<div data-x=\"{% else %}\">{% endif %}{% else %}<section>{% endif %}{{ od_content() }}\">",
        "{% if cond %}<!--{% else %}-->{% endif %}{{ od_content() }}-->",
        "{% if cond %}<div id=\"{% else %}\">{% endif %}{% if cond %}{{ od_content() }}{% else %}ok{% endif %}\">",
        "{% for n in [1] %}<div id=\"{% else %}\">{% endfor %}{{ od_content() }}\">",
    ];
    for source in bad {
        assert!(
            compile(source, &nodes, CsecContentKind::Index).is_err(),
            "branch-dependent HTML context accepted: {source:?}"
        );
    }
}

#[test]
fn rejects_part_that_leaves_attribute_open_in_only_one_branch() {
    let incomplete = parts(&[("header", "{% if cond %}<div id=\"{% else %}\">{% endif %}")]);
    assert!(
        compile(
            "<main>{{ od_part('header') }}{{ od_content() }}</main>",
            &incomplete,
            CsecContentKind::Entry
        )
        .is_err(),
        "part must not transfer an unclosed attribute context into od_content"
    );
}

#[test]
fn allows_safe_conditional_body_markup_and_repeated_closed_tags() {
    let nodes = parts(&[("header", "<h1>{{ title }}</h1>")]);
    let source = "<main>{% if cond %}<section>Yes</section>{% else %}<aside>No</aside>{% endif %}{% for n in [1,2] %}<span>{{ n }}</span>{% endfor %}{{ od_part('header') }}{{ od_content() }}</main>";
    let template = compile(source, &nodes, CsecContentKind::Entry).unwrap();
    let form = "<form method=\"post\" action=\"/contact\"><input name=\"email\"></form>";
    let yes = template
        .render(
            minijinja::context!(cond => true, title => "Hello"),
            Some(form),
        )
        .unwrap();
    let no = template
        .render(
            minijinja::context!(cond => false, title => "Hello"),
            Some(form),
        )
        .unwrap();
    assert!(yes.contains("<section>Yes</section>"));
    assert!(no.contains("<aside>No</aside>"));
    for html in [yes, no] {
        assert!(html.contains("<span>1</span><span>2</span>"));
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains(form));
    }
}

#[test]
fn rejects_nonliteral_nested_or_unsafe_jinja_calls() {
    let nodes = parts(&[("header", "<b>OK</b>")]);
    let bad = [
        "{{ od_part(part_name) }}",
        "{{ od_part('header') | upper }}",
        "{{ od_part('header', 2) }}",
        "{{ od_content('x') }}",
        "{% set x = od_part('header') %}{{ x }}",
        "{{ arbitrary_function('x') }}",
        "{% include 'header' %}",
        "{{ '<script>alert(1)</script>' | safe }}",
    ];
    for source in bad {
        assert!(
            compile(source, &nodes, CsecContentKind::Index).is_err(),
            "accepted unsafe {source:?}"
        );
    }
}

#[test]
fn expanded_parts_share_parent_context_and_fuel() {
    let nodes = parts(&[("header", "<h1>{{ title | upper }}</h1>")]);
    let tpl = compile(
        "<main>{% set title = 'ok' %}{{ od_part('header') }}</main>",
        &nodes,
        CsecContentKind::NonEntry,
    )
    .unwrap();
    assert_eq!(
        tpl.render(Value::UNDEFINED, None).unwrap(),
        "<main><h1>OK</h1></main>"
    );
    let malicious = parts(&[("header", "<script>{{ entry.title }}</script>")]);
    assert!(
        compile(
            "<main>{{ od_part('header') }}</main>",
            &malicious,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
}

#[test]
fn nonce_injection_allows_sanitized_contact_form_but_not_theme_form() {
    let empty = parts(&[]);
    let tpl = compile(
        "<main><article>{{ od_content() }}</article></main>",
        &empty,
        CsecContentKind::Entry,
    )
    .unwrap();
    let form = "<form method=\"post\" action=\"/contact\"><input name=\"email\"></form>";
    assert_eq!(
        tpl.render(Value::UNDEFINED, Some(form)).unwrap(),
        format!("<main><article>{form}</article></main>")
    );
    assert!(
        compile(
            "<main><form>{{ od_content() }}</form></main>",
            &empty,
            CsecContentKind::Entry
        )
        .is_err()
    );
    assert!(
        compile(
            "<main><script>{{ od_content() }}</script></main>",
            &empty,
            CsecContentKind::Entry
        )
        .is_err()
    );
}

#[test]
fn output_and_input_limits_include_injected_content_and_parts() {
    let empty = parts(&[]);
    let tpl = compile(
        "<main>{{ od_content() }}</main>",
        &empty,
        CsecContentKind::Entry,
    )
    .unwrap();
    assert!(
        tpl.render(Value::UNDEFINED, Some(&"X".repeat(MAX_RENDERED_HTML_BYTES)))
            .is_err()
    );
    let huge = parts(&[("header", &"x".repeat(MAX_RENDERED_HTML_BYTES + 1))]);
    assert!(compile("<main></main>", &huge, CsecContentKind::NonEntry).is_err());
    let long = "x".repeat(300_000);
    let nodes = parts(&[("large", &long)]);
    assert!(
        compile(
            "<main>{{ od_part('large') }}{{ od_part('large') }}</main>",
            &nodes,
            CsecContentKind::NonEntry
        )
        .is_err()
    );
}

#[test]
fn independent_unused_parts_do_not_consume_render_output_budget() {
    let sources = (0..5)
        .map(|n| (format!("unused-{n}"), "x".repeat(115_000)))
        .collect();
    let template = compile("<main>safe</main>", &sources, CsecContentKind::NonEntry).unwrap();
    assert_eq!(
        template.render(Value::UNDEFINED, None).unwrap(),
        "<main>safe</main>"
    );
}

#[test]
fn preview_theme_children_share_lexical_parent_and_conditional_execution() {
    let theme = parts(&[
        (
            "heading",
            "<h1>{{ title | upper }}</h1>{{ od_part('card') }}",
        ),
        ("unused", "{{ undefined_value }}"),
        ("card", "<p>theme card must lose to DB</p>"),
    ]);
    let trusted = parts(&[("card", "<article>DB card</article>")]);
    let root = "{% set title = 'outer' %}{{ od_part('heading') }}\
                {% with title = 'inner' %}{{ od_part('heading') }}{% endwith %}\
                {% if false %}{{ od_part('unused') }}{% endif %}\
                {{ od_part('heading') }}";
    assert_eq!(
        preview_csec_part_keys("root", root).unwrap(),
        ["heading", "heading", "unused", "heading"]
    );
    let mut budget = PreviewCompositionBudget::new();
    budget.expansions = 7;
    let html = render_preview_csec_fragment(
        "root",
        root,
        Value::UNDEFINED,
        &theme,
        &trusted,
        &mut budget,
    )
    .unwrap();
    assert_eq!(
        html,
        "<h1>OUTER</h1><article>DB card</article>\
         <h1>INNER</h1><article>DB card</article>\
         <h1>OUTER</h1><article>DB card</article>"
    );
    assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT);
    assert_eq!(budget.expansions, 7);
    assert!(budget.stack.is_empty());
}

#[test]
fn preview_fuel_is_debited_across_independent_theme_segments() {
    let empty = parts(&[]);
    let root = "{% for item in items %}{{ item }}{% endfor %}";
    // Each iteration consumes several VM instructions. Find a fixed
    // fixture that consumes more than half of the request's total fuel
    // while remaining valid in a fresh budget.
    let context = minijinja::context!(items => vec![1_u32; 1500]);
    let mut budget = PreviewCompositionBudget::new();
    let first =
        render_preview_csec_fragment("first", root, context.clone(), &empty, &empty, &mut budget)
            .unwrap();
    assert_eq!(first.len(), 1500);
    assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT / 2);
    assert!(
        render_preview_csec_fragment("second", root, context.clone(), &empty, &empty, &mut budget,)
            .is_err()
    );
    assert!(
        render_preview_csec_fragment(
            "second",
            root,
            context,
            &empty,
            &empty,
            &mut PreviewCompositionBudget::new(),
        )
        .is_ok()
    );
    budget.fuel_remaining = 0;
    assert!(
        render_preview_csec_fragment(
            "exhausted",
            "safe",
            Value::UNDEFINED,
            &empty,
            &empty,
            &mut budget,
        )
        .is_err()
    );
}
