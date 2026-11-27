use super::*;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]

pub(crate) enum UrlSourceKind {
    TrustedEntry,
    TrustedEntryBatch,
    TrustedLink,
    TrustedField,
    TrustedRawUrl,
    FieldNode(FieldTemplateNode),
}

pub(crate) fn step_theme_raw_char(
    state: ThemeHtmlState,
    ch: char,
    opts: HtmlStepOptions,
    svg_depth: usize,
) -> Result<ThemeHtmlState, &'static str> {
    if let ThemeHtmlState::Trusted {
        inner,
        svg_depth: depth,
    } = state
    {
        let completed_svg_tag = match inner.as_ref() {
            ThemeHtmlState::Tag {
                tag_name,
                is_close,
                phase,
                quote,
                ..
            } if tag_name == "svg" && ch == '>' && quote.is_none() => {
                Some((*is_close, *phase == TagPhase::SelfClosingSlash))
            }
            _ => None,
        };
        let next = step_theme_raw_char(
            *inner,
            ch,
            HtmlStepOptions {
                allow_trusted_theme_markup: true,
                ..opts
            },
            depth,
        )?;
        let depth = if let Some((is_close, self_closing)) = completed_svg_tag {
            if is_close {
                depth.checked_sub(1).ok_or("Thẻ </svg> không có thẻ mở.")?
            } else if self_closing {
                depth
            } else {
                depth
                    .checked_add(1)
                    .filter(|d| *d <= 64)
                    .ok_or("SVG lồng nhau quá sâu.")?
            }
        } else {
            depth
        };
        return Ok(ThemeHtmlState::with_trust(next, true, depth));
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
            mut tag_name,
            is_close,
            mut phase,
            mut attr_name,
            mut quote,
            mut attr_has_static_bytes,
            mut attr_expr_count,
            mut seen_attrs,
            mut link_rel_stylesheet,
            mut link_has_valid_href,
            mut meta_ok,
            mut attr_static_buf,
        } => {
            if ascii == Some(0) {
                return Err("Thẻ HTML chứa byte NUL bị cấm.");
            }
            match phase {
                TagPhase::TagName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-') => {
                        if tag_name.len() >= 32 {
                            return Err("Tên thẻ HTML vượt giới hạn 32 ký tự.");
                        }
                        tag_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => {
                        check_theme_tag_name(&tag_name, is_close, opts, svg_depth)?;
                        phase = TagPhase::BeforeAttr;
                    }
                    Some(b'/') => {
                        if is_close
                            || tag_name.is_empty()
                            || matches!(tag_name.as_str(), "style" | "title" | "script")
                        {
                            return Err("Dấu tự đóng `/` trên thẻ HTML không hợp lệ.");
                        }
                        check_theme_tag_name(&tag_name, is_close, opts, svg_depth)?;
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
                            svg_depth,
                        );
                    }
                    _ => return Err("Tên thẻ HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::BeforeAttr => {
                    if is_close {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'>') => {
                                return finalize_theme_tag(
                                    &tag_name,
                                    is_close,
                                    link_rel_stylesheet,
                                    link_has_valid_href,
                                    meta_ok,
                                    opts,
                                    svg_depth,
                                );
                            }
                            _ => return Err("Thẻ đóng HTML không được chứa thuộc tính hoặc /."),
                        }
                    } else {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'a'..=b'z' | b'A'..=b'Z') => {
                                attr_name.clear();
                                attr_name.push(ch.to_ascii_lowercase());
                                quote = None;
                                attr_has_static_bytes = false;
                                attr_expr_count = 0;
                                attr_static_buf.clear();
                                phase = TagPhase::AttrName;
                            }
                            Some(b'/') => {
                                if matches!(tag_name.as_str(), "style" | "title" | "script") {
                                    return Err("Thẻ <style>/<title> không được tự đóng.");
                                }
                                phase = TagPhase::SelfClosingSlash;
                            }
                            Some(b'>') => {
                                return finalize_theme_tag(
                                    &tag_name,
                                    is_close,
                                    link_rel_stylesheet,
                                    link_has_valid_href,
                                    meta_ok,
                                    opts,
                                    svg_depth,
                                );
                            }
                            _ => return Err("Thuộc tính HTML phải bắt đầu bằng chữ cái ASCII."),
                        }
                    }
                }
                TagPhase::AttrName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b':' | b'-') => {
                        if attr_name.len() >= 32 {
                            return Err("Tên thuộc tính HTML vượt giới hạn 32 ký tự.");
                        }
                        attr_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => {
                        phase = TagPhase::AfterAttrName;
                    }
                    Some(b'=') => {
                        phase = TagPhase::BeforeAttrValue;
                    }
                    Some(b'/') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        if matches!(tag_name.as_str(), "style" | "title" | "script") {
                            return Err("Thẻ <style>/<title> không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
                            svg_depth,
                        );
                    }
                    _ => return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::AfterAttrName => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(b'=') => {
                        phase = TagPhase::BeforeAttrValue;
                    }
                    Some(b'/') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        if matches!(tag_name.as_str(), "style" | "title" | "script") {
                            return Err("Thẻ <style>/<title> không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
                            svg_depth,
                        );
                    }
                    Some(b'a'..=b'z' | b'A'..=b'Z') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        attr_name.push(ch.to_ascii_lowercase());
                        phase = TagPhase::AttrName;
                    }
                    _ => return Err("Cú pháp sau tên thuộc tính HTML không hợp lệ."),
                },
                TagPhase::BeforeAttrValue => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(q @ (b'"' | b'\'')) => {
                        quote = Some(q);
                        phase = TagPhase::AttrValue;
                    }
                    Some(b'>' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f) => {
                        return Err("Giá trị thuộc tính HTML chứa ký tự không hợp lệ.");
                    }
                    _ => {
                        quote = None;
                        phase = TagPhase::AttrValue;
                        attr_has_static_bytes = true;
                        if is_security_checked_attr(&attr_name) {
                            if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                            }
                            attr_static_buf.push(ch);
                        }
                    }
                },
                TagPhase::AttrValue => {
                    if let Some(q) = quote {
                        if ascii == Some(q) {
                            finalize_theme_attr(
                                &tag_name,
                                &mut attr_name,
                                &mut quote,
                                &mut attr_has_static_bytes,
                                &mut attr_expr_count,
                                &mut seen_attrs,
                                &mut link_rel_stylesheet,
                                &mut link_has_valid_href,
                                &mut meta_ok,
                                &mut attr_static_buf,
                                opts.allow_trusted_theme_markup,
                            )?;
                            phase = TagPhase::BeforeAttr;
                        } else {
                            if matches!(attr_name.as_str(), "href" | "src" | "poster" | "cite")
                                && attr_expr_count > 0
                            {
                                return Err(
                                    "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                                );
                            }
                            attr_has_static_bytes = true;
                            if is_security_checked_attr(&attr_name) {
                                if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                    return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                                }
                                attr_static_buf.push(ch);
                            }
                        }
                    } else if ascii.is_some_and(is_ascii_html_whitespace) {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        phase = TagPhase::BeforeAttr;
                    } else if ascii == Some(b'>') {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
                            svg_depth,
                        );
                    } else if matches!(
                        ascii,
                        Some(b'"' | b'\'' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f)
                    ) {
                        return Err(
                            "Giá trị thuộc tính HTML không ngoặc kép chứa ký tự không hợp lệ.",
                        );
                    } else {
                        attr_has_static_bytes = true;
                        if is_security_checked_attr(&attr_name) {
                            if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                            }
                            attr_static_buf.push(ch);
                        }
                    }
                }
                TagPhase::SelfClosingSlash => {
                    if ascii == Some(b'>') {
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
                            svg_depth,
                        );
                    }
                    return Err("Sau `/` ở cuối thẻ mở phải là `>`.");
                }
            }
            Ok(ThemeHtmlState::Tag {
                tag_name,
                is_close,
                phase,
                attr_name,
                quote,
                attr_has_static_bytes,
                attr_expr_count,
                seen_attrs,
                link_rel_stylesheet,
                link_has_valid_href,
                meta_ok,
                attr_static_buf,
            })
        }
        ThemeHtmlState::Trusted { .. } => unreachable!("trusted state unwrapped above"),
    }
}
