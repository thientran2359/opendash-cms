use super::*;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]

pub(crate) enum UrlSourceKind {
    TrustedEntry,
    TrustedEntryBatch,
    TrustedTerm,
    TrustedLink,
    TrustedField,
    FieldNode(FieldTemplateNode),
}

pub(crate) fn step_theme_raw_char(
    state: ThemeHtmlState,
    ch: char,
    opts: HtmlStepOptions,
    depths: HtmlDepths,
) -> Result<ThemeHtmlState, &'static str> {
    if let ThemeHtmlState::Trusted { inner, depths } = state {
        let completed_tracked_tag = match inner.as_ref() {
            ThemeHtmlState::Tag {
                tag_name,
                is_close,
                phase,
                quote,
                ..
            } if ch == '>'
                && quote.is_none()
                && (matches!(tag_name.as_str(), "svg" | "math" | "form")
                    || (opts.forbid_style_tag
                        && matches!(
                            tag_name.as_str(),
                            "foreignobject" | "a" | "button" | "summary"
                        ))) =>
            {
                Some((
                    tag_name.clone(),
                    *is_close,
                    *phase == TagPhase::SelfClosingSlash,
                ))
            }
            _ => None,
        };
        let next = step_theme_raw_char(*inner, ch, opts, depths)?;
        let depths = if let Some((tag, is_close, self_closing)) = completed_tracked_tag {
            advance_depths_with_strict(depths, &tag, is_close, self_closing, opts.forbid_style_tag)?
        } else {
            depths
        };
        return Ok(ThemeHtmlState::with_depths(next, true, depths));
    }
    let ascii = if ch.is_ascii() { Some(ch as u8) } else { None };
    match state {
        ThemeHtmlState::Body => {
            if ascii == Some(b'<') {
                Ok(ThemeHtmlState::BodyLt)
            } else {
                Ok(ThemeHtmlState::Body)
            }
        }
        ThemeHtmlState::BodyLt => match ascii {
            Some(b'a'..=b'z' | b'A'..=b'Z') => Ok(new_theme_tag_state(ch, false)),
            Some(b'/') => Ok(ThemeHtmlState::BodyLtSlash),
            Some(b'!') => Ok(ThemeHtmlState::BodyLtBang {
                buf: [0; 7],
                len: 0,
            }),
            Some(b'<') => Ok(ThemeHtmlState::BodyLt),
            Some(b'?') => {
                Err("Template không được chứa processing instruction hoặc bogus comment `<?`.")
            }
            _ => Ok(ThemeHtmlState::Body),
        },
        ThemeHtmlState::BodyLtSlash => match ascii {
            Some(b'a'..=b'z' | b'A'..=b'Z') => Ok(new_theme_tag_state(ch, true)),
            _ => Err("Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII."),
        },
        ThemeHtmlState::BodyLtBang { mut buf, mut len } => {
            let Some(b) = ascii else {
                return Err("Template chỉ cho phép HTML comment <!-- hoặc <!DOCTYPE>.");
            };
            if len == 7 {
                if opts.is_part {
                    return Err("Part không được chứa DOCTYPE.");
                }
                if opts.forbid_style_tag {
                    return Err("Template block không được chứa <!DOCTYPE>.");
                }
                if b == b'>' {
                    return Ok(ThemeHtmlState::Body);
                }
                if is_ascii_html_whitespace(b) {
                    return Ok(ThemeHtmlState::Doctype);
                }
                return Err("<!DOCTYPE> không hợp lệ.");
            }
            buf[len as usize] = b.to_ascii_lowercase();
            len += 1;
            if len == 2 && &buf[..2] == b"--" {
                return Ok(ThemeHtmlState::Comment(CommentScanSubState::Start));
            }
            let prefix = &buf[..len as usize];
            if b"--".starts_with(prefix) || b"doctype".starts_with(prefix) {
                Ok(ThemeHtmlState::BodyLtBang { buf, len })
            } else {
                Err("Template chỉ cho phép HTML comment <!-- hoặc <!DOCTYPE>.")
            }
        }
        ThemeHtmlState::Doctype => {
            if let Some(b) = ascii {
                if b == b'>' {
                    return Ok(ThemeHtmlState::Body);
                }
                if matches!(b, b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f) {
                    return Err("<!DOCTYPE> chứa ký tự không hợp lệ.");
                }
            }
            Ok(ThemeHtmlState::Doctype)
        }
        ThemeHtmlState::Comment(sub) => {
            if sub == CommentScanSubState::Dash2Bang && ascii == Some(b'>') {
                return Err("Template cấm đóng HTML comment bằng `--!>`.");
            }
            let next = match ascii {
                Some(b) => step_comment_substate(sub, b),
                None => Some(CommentScanSubState::Body),
            };
            Ok(match next {
                Some(s) => ThemeHtmlState::Comment(s),
                None => ThemeHtmlState::Body,
            })
        }
        ThemeHtmlState::Style {
            mut closing_prefix,
            mut css_buf,
        } => {
            const EXPECTED: &[u8; 7] = b"</style";
            if closing_prefix == 7 {
                if ascii == Some(b'>') {
                    let trim_to = css_buf.len().saturating_sub(7);
                    css_buf.truncate(trim_to);
                    if css_buf.len() > MAX_RENDERED_HTML_BYTES {
                        return Err("Khối <style> vượt giới hạn 512 KiB.");
                    }
                    validate_safe_css(&css_buf)?;
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </style> không được chứa khoảng trắng hoặc dấu /.");
                }
                if css_buf.len() + ch.len_utf8() > MAX_RENDERED_HTML_BYTES + 8 {
                    return Err("Khối <style> vượt giới hạn 512 KiB.");
                }
                css_buf.push(ch);
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
                return Ok(ThemeHtmlState::Style {
                    closing_prefix,
                    css_buf,
                });
            }
            if css_buf.len() + ch.len_utf8() > MAX_RENDERED_HTML_BYTES + 8 {
                return Err("Khối <style> vượt giới hạn 512 KiB.");
            }
            css_buf.push(ch);
            closing_prefix = match ascii {
                Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => closing_prefix + 1,
                Some(b'<') => 1,
                _ => 0,
            };
            Ok(ThemeHtmlState::Style {
                closing_prefix,
                css_buf,
            })
        }
        ThemeHtmlState::Script {
            mut closing_prefix,
            mut comment_prefix,
            mut cdata_prefix,
        } => {
            // Keep the HTML5 escaped and SVG CDATA scanners in the Theme AST
            // walker too. A fake closing tag inside either state may cause
            // untrusted Jinja output after it to execute as JavaScript.
            comment_prefix = next_script_escaped_prefix(b"<!--", comment_prefix, ascii);
            cdata_prefix = next_script_escaped_prefix(b"<![CDATA[", cdata_prefix, ascii);
            if comment_prefix == 4 || cdata_prefix == 9 {
                return Err("C-SEC cam HTML escaped state va SVG CDATA trong <script>.");
            }
            const EXPECTED: &[u8; 8] = b"</script";
            if closing_prefix == EXPECTED.len() {
                if ascii == Some(b'>') {
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </script> không được chứa khoảng trắng hoặc /.");
                }
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
            } else {
                closing_prefix = match ascii {
                    Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => {
                        closing_prefix + 1
                    }
                    Some(b'<') => 1,
                    _ => 0,
                };
            }
            Ok(ThemeHtmlState::Script {
                closing_prefix,
                comment_prefix,
                cdata_prefix,
            })
        }
        ThemeHtmlState::Title { mut closing_prefix } => {
            const EXPECTED: &[u8; 7] = b"</title";
            if closing_prefix == 7 {
                if ascii == Some(b'>') {
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </title> không được chứa khoảng trắng hoặc dấu /.");
                }
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
                return Ok(ThemeHtmlState::Title { closing_prefix });
            }
            closing_prefix = match ascii {
                Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => closing_prefix + 1,
                Some(b'<') => 1,
                _ => 0,
            };
            Ok(ThemeHtmlState::Title { closing_prefix })
        }
        ThemeHtmlState::Tag {
            tag_name,
            is_close,
            mut phase,
            attr_name,
            quote,
            attr_has_static_bytes,
            attr_expr_count,
            seen_attrs,
            link_rel_stylesheet,
            link_has_valid_href,
            meta_ok,
            form_has_action,
            attr_static_buf,
        } => {
            if ascii == Some(0) {
                return Err("Thẻ HTML chứa byte NUL bị cấm.");
            }
            let mut t = TagAcc {
                tag_name,
                is_close,
                attr_name,
                quote,
                attr_has_static_bytes,
                attr_expr_count,
                seen_attrs,
                link_rel_stylesheet,
                link_has_valid_href,
                meta_ok,
                form_has_action,
                attr_static_buf,
            };
            let forbid_sc = |name: &str| {
                matches!(name, "style" | "title" | "script") || is_non_void_search_form_tag(name)
            };
            match phase {
                TagPhase::TagName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-') => {
                        if t.tag_name.len() >= 32 {
                            return Err("Tên thẻ HTML vượt giới hạn 32 ký tự.");
                        }
                        t.tag_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => {
                        check_theme_tag_name(&t.tag_name, t.is_close, opts, depths)?;
                        phase = TagPhase::BeforeAttr;
                    }
                    Some(b'/') => {
                        if t.is_close || t.tag_name.is_empty() || forbid_sc(&t.tag_name) {
                            return Err("Dấu tự đóng `/` trên thẻ HTML không hợp lệ.");
                        }
                        check_theme_tag_name(&t.tag_name, t.is_close, opts, depths)?;
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => return t.finish_tag(opts, depths),
                    _ => return Err("Tên thẻ HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::BeforeAttr => {
                    if t.is_close {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'>') => return t.finish_tag(opts, depths),
                            _ => return Err("Thẻ đóng HTML không được chứa thuộc tính hoặc /."),
                        }
                    } else {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'a'..=b'z' | b'A'..=b'Z') => {
                                t.reset_attr(ch);
                                phase = TagPhase::AttrName;
                            }
                            Some(b'/') => {
                                if forbid_sc(&t.tag_name) {
                                    return Err("Thẻ HTML này không được tự đóng.");
                                }
                                phase = TagPhase::SelfClosingSlash;
                            }
                            Some(b'>') => return t.finish_tag(opts, depths),
                            _ => return Err("Thuộc tính HTML phải bắt đầu bằng chữ cái ASCII."),
                        }
                    }
                }
                TagPhase::AttrName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b':' | b'-') => {
                        if t.attr_name.len() >= 32 {
                            return Err("Tên thuộc tính HTML vượt giới hạn 32 ký tự.");
                        }
                        t.attr_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => phase = TagPhase::AfterAttrName,
                    Some(b'=') => phase = TagPhase::BeforeAttrValue,
                    Some(b'/') => {
                        t.finish_attr(opts)?;
                        if forbid_sc(&t.tag_name) {
                            return Err("Thẻ HTML này không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        t.finish_attr(opts)?;
                        return t.finish_tag(opts, depths);
                    }
                    _ => return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::AfterAttrName => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(b'=') => phase = TagPhase::BeforeAttrValue,
                    Some(b'/') => {
                        t.finish_attr(opts)?;
                        if forbid_sc(&t.tag_name) {
                            return Err("Thẻ HTML này không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        t.finish_attr(opts)?;
                        return t.finish_tag(opts, depths);
                    }
                    Some(b'a'..=b'z' | b'A'..=b'Z') => {
                        t.finish_attr(opts)?;
                        t.attr_name.push(ch.to_ascii_lowercase());
                        phase = TagPhase::AttrName;
                    }
                    _ => return Err("Cú pháp sau tên thuộc tính HTML không hợp lệ."),
                },
                TagPhase::BeforeAttrValue => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(q @ (b'"' | b'\'')) => {
                        t.quote = Some(q);
                        phase = TagPhase::AttrValue;
                    }
                    Some(b'>' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f) => {
                        return Err("Giá trị thuộc tính HTML chứa ký tự không hợp lệ.");
                    }
                    _ => {
                        t.quote = None;
                        phase = TagPhase::AttrValue;
                        t.push_static_char(ch)?;
                    }
                },
                TagPhase::AttrValue => {
                    if let Some(q) = t.quote {
                        if ascii == Some(q) {
                            t.finish_attr(opts)?;
                            phase = TagPhase::BeforeAttr;
                        } else {
                            if matches!(
                                t.attr_name.as_str(),
                                "href" | "src" | "poster" | "cite" | "action"
                            ) && t.attr_expr_count > 0
                            {
                                return Err(
                                    "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                                );
                            }
                            t.push_static_char(ch)?;
                        }
                    } else if ascii.is_some_and(is_ascii_html_whitespace) {
                        t.finish_attr(opts)?;
                        phase = TagPhase::BeforeAttr;
                    } else if ascii == Some(b'>') {
                        t.finish_attr(opts)?;
                        return t.finish_tag(opts, depths);
                    } else if matches!(
                        ascii,
                        Some(b'"' | b'\'' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f)
                    ) {
                        return Err(
                            "Giá trị thuộc tính HTML không ngoặc kép chứa ký tự không hợp lệ.",
                        );
                    } else {
                        t.push_static_char(ch)?;
                    }
                }
                TagPhase::SelfClosingSlash => {
                    if ascii == Some(b'>') {
                        return t.finish_tag(opts, depths);
                    }
                    return Err("Sau `/` ở cuối thẻ mở phải là `>`.");
                }
            }
            Ok(t.into_state(phase))
        }
        ThemeHtmlState::Trusted { .. } => unreachable!("trusted state unwrapped above"),
    }
}
