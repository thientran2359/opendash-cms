use super::{L10nError, MAX_PLURAL_FORMS};

const MAX_TOKENS: usize = 128;
const MAX_AST_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluralExpr {
    Num(u64),
    VarN,
    Not(Box<PluralExpr>),
    BinOp(BinOp, Box<PluralExpr>, Box<PluralExpr>),
    Ternary(Box<PluralExpr>, Box<PluralExpr>, Box<PluralExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluralRule {
    pub nplurals: usize,
    pub expr: PluralExpr,
    pub raw_header: String,
}

impl Default for PluralRule {
    fn default() -> Self {
        Self {
            nplurals: 2,
            expr: PluralExpr::BinOp(
                BinOp::Ne,
                Box::new(PluralExpr::VarN),
                Box::new(PluralExpr::Num(1)),
            ),
            raw_header: "nplurals=2; plural=(n != 1);".to_owned(),
        }
    }
}

impl PluralRule {
    /// Parses a GNU gettext `Plural-Forms` header value (`nplurals=K; plural=<expr>;`).
    pub fn parse(header: &str) -> Result<Self, L10nError> {
        let mut nplurals: Option<usize> = None;
        let mut plural_src: Option<&str> = None;
        for part in header.split(';') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(val) = trimmed.strip_prefix("nplurals=") {
                let parsed: usize = val
                    .trim()
                    .parse()
                    .map_err(|_| L10nError::new("Giá trị nplurals không hợp lệ."))?;
                if !(1..=MAX_PLURAL_FORMS).contains(&parsed) {
                    return Err(L10nError::new("nplurals phải nằm trong khoảng 1..=6."));
                }
                nplurals = Some(parsed);
            } else if let Some(val) = trimmed.strip_prefix("plural=") {
                plural_src = Some(val.trim());
            }
        }
        let nplurals =
            nplurals.ok_or_else(|| L10nError::new("Header Plural-Forms thiếu nplurals."))?;
        let plural_src =
            plural_src.ok_or_else(|| L10nError::new("Header Plural-Forms thiếu plural=."))?;

        let tokens = tokenize(plural_src)?;
        let mut parser = Parser {
            tokens: &tokens,
            pos: 0,
        };
        let expr = parser.parse_expr(0)?;
        if parser.pos != tokens.len() {
            return Err(L10nError::new("Biểu thức plural chứa token thừa."));
        }
        Ok(Self {
            nplurals,
            expr,
            raw_header: format!("nplurals={nplurals}; plural={plural_src};"),
        })
    }

    /// Evaluates the plural index for `count`, clamped to `0..available_forms`.
    pub fn evaluate(&self, count: i64, available_forms: usize) -> usize {
        if available_forms <= 1 {
            return 0;
        }
        let n = count.unsigned_abs();
        let idx = eval_expr(&self.expr, n) as usize;
        let max_idx = self.nplurals.min(available_forms).saturating_sub(1);
        idx.min(max_idx)
    }
}

fn eval_expr(expr: &PluralExpr, n: u64) -> u64 {
    match expr {
        PluralExpr::Num(v) => *v,
        PluralExpr::VarN => n,
        PluralExpr::Not(inner) => u64::from(eval_expr(inner, n) == 0),
        PluralExpr::Ternary(cond, t_br, f_br) => {
            if eval_expr(cond, n) != 0 {
                eval_expr(t_br, n)
            } else {
                eval_expr(f_br, n)
            }
        }
        PluralExpr::BinOp(op, lhs, rhs) => match op {
            BinOp::Or => u64::from(eval_expr(lhs, n) != 0 || eval_expr(rhs, n) != 0),
            BinOp::And => u64::from(eval_expr(lhs, n) != 0 && eval_expr(rhs, n) != 0),
            BinOp::Eq => u64::from(eval_expr(lhs, n) == eval_expr(rhs, n)),
            BinOp::Ne => u64::from(eval_expr(lhs, n) != eval_expr(rhs, n)),
            BinOp::Lt => u64::from(eval_expr(lhs, n) < eval_expr(rhs, n)),
            BinOp::Le => u64::from(eval_expr(lhs, n) <= eval_expr(rhs, n)),
            BinOp::Gt => u64::from(eval_expr(lhs, n) > eval_expr(rhs, n)),
            BinOp::Ge => u64::from(eval_expr(lhs, n) >= eval_expr(rhs, n)),
            BinOp::Add => eval_expr(lhs, n).wrapping_add(eval_expr(rhs, n)),
            BinOp::Sub => eval_expr(lhs, n).wrapping_sub(eval_expr(rhs, n)),
            BinOp::Mod => {
                let r = eval_expr(rhs, n);
                eval_expr(lhs, n).checked_rem(r).unwrap_or(0)
            }
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Num(u64),
    VarN,
    LParen,
    RParen,
    Question,
    Colon,
    Bang,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Mod,
}

fn tokenize(src: &str) -> Result<Vec<Token>, L10nError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if tokens.len() >= MAX_TOKENS {
            return Err(L10nError::new("Biểu thức Plural-Forms vượt quá 128 token."));
        }
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'n' => {
                if i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphanumeric() {
                    return Err(L10nError::new("Biến không hợp lệ trong Plural-Forms."));
                }
                tokens.push(Token::VarN);
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let num: u64 = src[start..i]
                    .parse()
                    .map_err(|_| L10nError::new("Số nguyên quá lớn trong Plural-Forms."))?;
                tokens.push(Token::Num(num));
            }
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            b'?' => {
                tokens.push(Token::Question);
                i += 1;
            }
            b':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            b'+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            b'%' => {
                tokens.push(Token::Mod);
                i += 1;
            }
            b'|' if i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                tokens.push(Token::Or);
                i += 2;
            }
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                tokens.push(Token::And);
                i += 2;
            }
            b'=' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                tokens.push(Token::Eq);
                i += 2;
            }
            b'!' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                tokens.push(Token::Ne);
                i += 2;
            }
            b'!' => {
                tokens.push(Token::Bang);
                i += 1;
            }
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                tokens.push(Token::Le);
                i += 2;
            }
            b'<' => {
                tokens.push(Token::Lt);
                i += 1;
            }
            b'>' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                tokens.push(Token::Ge);
                i += 2;
            }
            b'>' => {
                tokens.push(Token::Gt);
                i += 1;
            }
            _ => return Err(L10nError::new("Ký tự không hợp lệ trong Plural-Forms.")),
        }
    }
    if tokens.is_empty() {
        return Err(L10nError::new("Biểu thức Plural-Forms rỗng."));
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn check_depth(depth: usize) -> Result<(), L10nError> {
        if depth > MAX_AST_DEPTH {
            Err(L10nError::new("Độ sâu biểu thức Plural-Forms vượt quá 16."))
        } else {
            Ok(())
        }
    }

    fn parse_expr(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        Self::check_depth(depth)?;
        let cond = self.parse_or(depth)?;
        if matches!(self.peek(), Some(Token::Question)) {
            self.next();
            let t_br = self.parse_expr(depth + 1)?;
            if !matches!(self.next(), Some(Token::Colon)) {
                return Err(L10nError::new("Thiếu ':' trong toán tử điều kiện ? :."));
            }
            let f_br = self.parse_expr(depth + 1)?;
            Ok(PluralExpr::Ternary(
                Box::new(cond),
                Box::new(t_br),
                Box::new(f_br),
            ))
        } else {
            Ok(cond)
        }
    }

    fn parse_or(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_and(depth)?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.next();
            let rhs = self.parse_and(depth)?;
            lhs = PluralExpr::BinOp(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_eq(depth)?;
        while matches!(self.peek(), Some(Token::And)) {
            self.next();
            let rhs = self.parse_eq(depth)?;
            lhs = PluralExpr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_eq(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_rel(depth)?;
        while let Some(op) = match self.peek() {
            Some(Token::Eq) => Some(BinOp::Eq),
            Some(Token::Ne) => Some(BinOp::Ne),
            _ => None,
        } {
            self.next();
            let rhs = self.parse_rel(depth)?;
            lhs = PluralExpr::BinOp(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_rel(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_add(depth)?;
        while let Some(op) = match self.peek() {
            Some(Token::Lt) => Some(BinOp::Lt),
            Some(Token::Le) => Some(BinOp::Le),
            Some(Token::Gt) => Some(BinOp::Gt),
            Some(Token::Ge) => Some(BinOp::Ge),
            _ => None,
        } {
            self.next();
            let rhs = self.parse_add(depth)?;
            lhs = PluralExpr::BinOp(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_add(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_mod(depth)?;
        while let Some(op) = match self.peek() {
            Some(Token::Plus) => Some(BinOp::Add),
            Some(Token::Minus) => Some(BinOp::Sub),
            _ => None,
        } {
            self.next();
            let rhs = self.parse_mod(depth)?;
            lhs = PluralExpr::BinOp(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_mod(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        let mut lhs = self.parse_unary(depth)?;
        while matches!(self.peek(), Some(Token::Mod)) {
            self.next();
            let rhs = self.parse_unary(depth)?;
            if matches!(rhs, PluralExpr::Num(0)) {
                return Err(L10nError::new(
                    "Phép chia lấy dư cho 0 bị cấm trong Plural-Forms.",
                ));
            }
            lhs = PluralExpr::BinOp(BinOp::Mod, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        Self::check_depth(depth)?;
        if matches!(self.peek(), Some(Token::Bang)) {
            self.next();
            let inner = self.parse_unary(depth + 1)?;
            Ok(PluralExpr::Not(Box::new(inner)))
        } else {
            self.parse_primary(depth)
        }
    }

    fn parse_primary(&mut self, depth: usize) -> Result<PluralExpr, L10nError> {
        Self::check_depth(depth)?;
        match self.next() {
            Some(Token::Num(v)) => Ok(PluralExpr::Num(v)),
            Some(Token::VarN) => Ok(PluralExpr::VarN),
            Some(Token::LParen) => {
                let inner = self.parse_expr(depth + 1)?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err(L10nError::new("Thiếu ')' trong Plural-Forms."));
                }
                Ok(inner)
            }
            _ => Err(L10nError::new("Cú pháp Plural-Forms không hợp lệ.")),
        }
    }
}
