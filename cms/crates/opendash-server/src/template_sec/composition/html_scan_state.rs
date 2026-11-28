use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HtmlNestingDepths {
    pub(crate) svg: u8,
    pub(crate) math: u8,
    pub(crate) foreign_object: u8,
    pub(crate) a: u8,
    pub(crate) button: u8,
    pub(crate) summary: u8,
}

impl HtmlNestingDepths {
    pub(crate) fn is_zero(self) -> bool {
        self.svg == 0
            && self.math == 0
            && self.foreign_object == 0
            && self.a == 0
            && self.button == 0
            && self.summary == 0
    }
}

pub(crate) fn update_depths_from_tag_prefix(
    prefix: &str,
    trailing_slash: bool,
    depths: &mut HtmlNestingDepths,
) {
    let Some(after_lt) = prefix.strip_prefix('<') else {
        return;
    };
    let (is_close, name_src) = match after_lt.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, after_lt),
    };
    let name_len = name_src
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
        .count();
    if name_len == 0 {
        return;
    }
    let tag = name_src[..name_len].to_ascii_lowercase();
    match tag.as_str() {
        "svg" => {
            if is_close {
                depths.svg = depths.svg.saturating_sub(1);
            } else if !trailing_slash {
                depths.svg = depths.svg.saturating_add(1);
            }
        }
        "math" => {
            if is_close {
                depths.math = depths.math.saturating_sub(1);
            } else if !trailing_slash {
                depths.math = depths.math.saturating_add(1);
            }
        }
        "foreignobject" => {
            if is_close {
                depths.foreign_object = depths.foreign_object.saturating_sub(1);
            } else if !trailing_slash {
                depths.foreign_object = depths.foreign_object.saturating_add(1);
            }
        }
        "a" => {
            if is_close {
                depths.a = depths.a.saturating_sub(1);
            } else {
                depths.a = depths.a.saturating_add(1);
            }
        }
        "button" => {
            if is_close {
                depths.button = depths.button.saturating_sub(1);
            } else {
                depths.button = depths.button.saturating_add(1);
            }
        }
        "summary" => {
            if is_close {
                depths.summary = depths.summary.saturating_sub(1);
            } else {
                depths.summary = depths.summary.saturating_add(1);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HtmlBodyScanner {
    pub(crate) state: HtmlScanState,
    pub(crate) depths: HtmlNestingDepths,
}

impl HtmlBodyScanner {
    pub(crate) fn new() -> Self {
        Self {
            state: HtmlScanState::Body,
            depths: HtmlNestingDepths::default(),
        }
    }

    pub(crate) fn consume(&mut self, byte: u8) {
        if byte == b'>'
            && let HtmlScanState::Tag {
                quote: None,
                ref prefix,
                trailing_slash,
            } = self.state
        {
            update_depths_from_tag_prefix(prefix, trailing_slash, &mut self.depths);
        }
        self.state = self.state.clone().consume(byte);
    }

    pub(crate) fn is_safe_container_body(&self) -> bool {
        self.state == HtmlScanState::Body && self.depths.is_zero()
    }
}

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
                trailing_slash: false,
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
            Self::Textarea { closing_prefix } => {
                const CLOSE: &[u8] = b"</textarea>";
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
                    Self::Textarea {
                        closing_prefix: next,
                    }
                }
            }
            Self::Tag {
                mut quote,
                mut prefix,
                mut trailing_slash,
            } => {
                if prefix == "<" {
                    if byte == b'<' {
                        return Self::Tag {
                            quote: None,
                            prefix: "<".to_owned(),
                            trailing_slash: false,
                        };
                    }
                    if !(byte.is_ascii_alphabetic() || matches!(byte, b'/' | b'!' | b'?')) {
                        return Self::Body;
                    }
                }
                if prefix.len() < 24 {
                    prefix.push(char::from(byte));
                }
                if prefix == "<!--" {
                    return Self::Comment(CommentScanSubState::Start);
                }
                if let Some(q) = quote {
                    trailing_slash = false;
                    if byte == q {
                        quote = None;
                    }
                } else if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                    trailing_slash = false;
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
                    if lower.starts_with("<textarea")
                        && lower
                            .as_bytes()
                            .get(9)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Textarea { closing_prefix: 0 };
                    }
                    return Self::Body;
                } else if byte == b'/' {
                    trailing_slash = true;
                } else if !byte.is_ascii_whitespace() {
                    trailing_slash = false;
                }
                Self::Tag {
                    quote,
                    prefix,
                    trailing_slash,
                }
            }
        }
    }
}
