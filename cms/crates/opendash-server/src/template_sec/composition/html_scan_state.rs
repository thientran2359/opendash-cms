use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CommentScanSubState {
    /// Immediately after `<!--`: `>` closes (`<!-->`), `-` enters `StartDash`.
    Start,
    /// Immediately after `<!---`: `>` closes (`<!--->`), `-` enters `Dash2`.
    StartDash,
    /// Inside comment body.
    Body,
    /// Saw one `-` in comment body.
    Dash1,
    /// Saw `--`: `>` closes (`-->`), `!` enters `Dash2Bang`, `-` stays `Dash2`.
    Dash2,
    /// Saw `--!`: `>` closes (`--!>`), `-` enters `Dash1`.
    Dash2Bang,
}

pub(crate) fn step_comment_substate(
    sub: CommentScanSubState,
    byte: u8,
) -> Option<CommentScanSubState> {
    match sub {
        CommentScanSubState::Start => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::StartDash),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::StartDash => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Body => match byte {
            b'-' => Some(CommentScanSubState::Dash1),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash1 => match byte {
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash2 => match byte {
            b'>' => None,
            b'!' => Some(CommentScanSubState::Dash2Bang),
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash2Bang => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::Dash1),
            _ => Some(CommentScanSubState::Body),
        },
    }
}

impl HtmlScanState {
    pub(crate) fn consume(self, byte: u8) -> Self {
        match self {
            Self::Body if byte == b'<' => Self::Tag {
                quote: None,
                prefix: "<".to_owned(),
            },
            Self::Body => Self::Body,
            Self::Comment(sub) => match step_comment_substate(sub, byte) {
                Some(next) => Self::Comment(next),
                None => Self::Body,
            },
            Self::Style { closing_prefix } => {
                const CLOSE: &[u8] = b"</style>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Style {
                        closing_prefix: next,
                    }
                }
            }
            Self::Script { closing_prefix } => {
                const CLOSE: &[u8] = b"</script>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Script {
                        closing_prefix: next,
                    }
                }
            }
            Self::Title { closing_prefix } => {
                const CLOSE: &[u8] = b"</title>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Title {
                        closing_prefix: next,
                    }
                }
            }
            Self::Tag {
                mut quote,
                mut prefix,
            } => {
                if prefix == "<" {
                    if byte == b'<' {
                        return Self::Tag {
                            quote: None,
                            prefix: "<".to_owned(),
                        };
                    }
                    if !(byte.is_ascii_alphabetic() || matches!(byte, b'/' | b'!' | b'?')) {
                        return Self::Body;
                    }
                }
                if prefix.len() < 8 {
                    prefix.push(char::from(byte));
                }
                if prefix == "<!--" {
                    return Self::Comment(CommentScanSubState::Start);
                }
                if let Some(q) = quote {
                    if byte == q {
                        quote = None;
                    }
                } else if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    let lower = prefix.to_ascii_lowercase();
                    if lower.starts_with("<style")
                        && lower
                            .as_bytes()
                            .get(6)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Style { closing_prefix: 0 };
                    }
                    if lower.starts_with("<script")
                        && lower
                            .as_bytes()
                            .get(7)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Script { closing_prefix: 0 };
                    }
                    if lower.starts_with("<title")
                        && lower
                            .as_bytes()
                            .get(6)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Title { closing_prefix: 0 };
                    }
                    return Self::Body;
                }
                Self::Tag { quote, prefix }
            }
        }
    }
}
