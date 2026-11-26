use super::*;

use super::*;

#[test]
fn syntax_validator_accepts_valid_js_without_executing_and_rejects_broken_js() {
    validate_js_module_syntax(
        r#"
        od.addFilter("filter:entry/view", (env) => ({ badge: od.escapeHtml("<a>") }));
        "#,
    )
    .expect("valid JS passes");
    // Must not execute top-level runtime errors during static parse check.
    validate_js_module_syntax("throw new Error('must not run during parse');")
        .expect("static parse must not execute");
    assert!(validate_js_module_syntax("function broken( { ").is_err());
}

#[test]
fn runtime_limit_errors_classify_as_out_of_fuel() {
    let mut ctx = Context::default();
    ctx.runtime_limits_mut().set_loop_iteration_limit(100);
    let loop_err = ctx
        .eval(Source::from_bytes(b"while (true) {}"))
        .unwrap_err();
    assert_eq!(classify_js_error(&loop_err, false).0, "out_of_fuel");

    let mut ctx2 = Context::default();
    ctx2.runtime_limits_mut().set_recursion_limit(16);
    let recur_err = ctx2
        .eval(Source::from_bytes(
            b"function f(n) { return f(n + 1); } f(0);",
        ))
        .unwrap_err();
    assert_eq!(classify_js_error(&recur_err, false).0, "out_of_fuel");
}
