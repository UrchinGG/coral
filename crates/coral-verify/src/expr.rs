use anyhow::{Result, bail};
use serde_json::Value;

const MAX_DEPTH: usize = 16;
const MAX_OUTPUT_LEN: usize = 256;


#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Ident(String),
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    And,
    Or,
    Not,
    If,
    Else,
    Comma,
    Colon,
}


fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => i += 1,
            '.' => { tokens.push(Token::Dot); i += 1 }
            '+' => { tokens.push(Token::Plus); i += 1 }
            '-' => { tokens.push(Token::Minus); i += 1 }
            '*' => { tokens.push(Token::Star); i += 1 }
            '/' => { tokens.push(Token::Slash); i += 1 }
            '%' => { tokens.push(Token::Percent); i += 1 }
            '(' => { tokens.push(Token::LParen); i += 1 }
            ')' => { tokens.push(Token::RParen); i += 1 }
            ',' => { tokens.push(Token::Comma); i += 1 }
            ':' => { tokens.push(Token::Colon); i += 1 }
            '>' if i + 1 < chars.len() && chars[i + 1] == '=' => { tokens.push(Token::Ge); i += 2 }
            '<' if i + 1 < chars.len() && chars[i + 1] == '=' => { tokens.push(Token::Le); i += 2 }
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => { tokens.push(Token::Eq); i += 2 }
            '!' if i + 1 < chars.len() && chars[i + 1] == '=' => { tokens.push(Token::Ne); i += 2 }
            '>' => { tokens.push(Token::Gt); i += 1 }
            '<' => { tokens.push(Token::Lt); i += 1 }
            '"' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' { i += 1 }
                tokens.push(Token::String(chars[start..i].iter().collect()));
                if i < chars.len() { i += 1 }
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1 }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Number(s.parse::<f64>()?));
            }
            c if c.is_ascii_alphanumeric() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') { i += 1 }
                let word: String = chars[start..i].iter().collect();
                tokens.push(match word.as_str() {
                    "and" => Token::And,
                    "or" => Token::Or,
                    "not" => Token::Not,
                    "if" => Token::If,
                    "else" => Token::Else,
                    _ => Token::Ident(word),
                });
            }
            c => bail!("unexpected character: '{c}'"),
        }
    }

    Ok(tokens)
}


#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    String(String),
    Field(Vec<String>),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryNot(Box<Expr>),
    Cond {
        branches: Vec<(Box<Expr>, Box<Expr>)>,
        fallback: Box<Expr>,
    },
}


#[derive(Debug, Clone)]
enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Gt, Lt, Ge, Le, Eq, Ne,
    And, Or,
}


struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}


impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, depth: 0 }
    }

    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(tok)
    }

    fn expect(&mut self, expected: &Token) -> Result<()> {
        match self.advance() {
            Some(ref tok) if tok == expected => Ok(()),
            other => bail!("expected {expected:?}, got {other:?}"),
        }
    }

    fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { bail!("expression too deeply nested") }
        Ok(())
    }

    fn leave(&mut self) { self.depth -= 1 }

    fn parse(mut self) -> Result<Expr> {
        let expr = self.parse_or()?;
        if self.pos < self.tokens.len() {
            bail!("unexpected token: {:?}", self.tokens[self.pos]);
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr> {
        self.enter()?;
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance();
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(self.parse_and()?));
        }
        self.leave();
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        self.enter()?;
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.advance();
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(self.parse_comparison()?));
        }
        self.leave();
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        self.enter()?;
        let left = self.parse_additive()?;
        let op = match self.peek() {
            Some(Token::Gt) => BinOp::Gt,
            Some(Token::Lt) => BinOp::Lt,
            Some(Token::Ge) => BinOp::Ge,
            Some(Token::Le) => BinOp::Le,
            Some(Token::Eq) => BinOp::Eq,
            Some(Token::Ne) => BinOp::Ne,
            _ => { self.leave(); return Ok(left) }
        };
        self.advance();
        let right = self.parse_additive()?;
        self.leave();
        Ok(Expr::BinOp(Box::new(left), op, Box::new(right)))
    }

    fn parse_additive(&mut self) -> Result<Expr> {
        self.enter()?;
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => break,
            };
            self.advance();
            left = Expr::BinOp(Box::new(left), op, Box::new(self.parse_multiplicative()?));
        }
        self.leave();
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr> {
        self.enter()?;
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinOp::Mul,
                Some(Token::Slash) => BinOp::Div,
                Some(Token::Percent) => BinOp::Mod,
                _ => break,
            };
            self.advance();
            left = Expr::BinOp(Box::new(left), op, Box::new(self.parse_unary()?));
        }
        self.leave();
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        self.enter()?;
        let expr = if matches!(self.peek(), Some(Token::Not)) {
            self.advance();
            Expr::UnaryNot(Box::new(self.parse_unary()?))
        } else {
            self.parse_primary()?
        };
        self.leave();
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.advance() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::String(s)) => Ok(Expr::String(s)),
            Some(Token::If) => self.parse_cond(),
            Some(Token::Ident(name)) => {
                let mut path = vec![name];
                while matches!(self.peek(), Some(Token::Dot)) {
                    self.advance();
                    match self.advance() {
                        Some(Token::Ident(next)) => path.push(next),
                        other => bail!("expected field name after '.', got {other:?}"),
                    }
                }
                Ok(Expr::Field(path))
            }
            Some(Token::LParen) => {
                let expr = self.parse_or()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            other => bail!("unexpected token: {other:?}"),
        }
    }

    fn parse_cond(&mut self) -> Result<Expr> {
        self.enter()?;
        let mut branches = Vec::new();
        let mut subject: Option<Expr> = None;

        loop {
            let condition = if subject.is_some() && self.peek_is_comparison_op() {
                let left = subject.clone().unwrap();
                let op = self.parse_comparison_op()?;
                let right = self.parse_additive()?;
                Expr::BinOp(Box::new(left), op, Box::new(right))
            } else {
                let cond = self.parse_or()?;
                if subject.is_none() {
                    if let Expr::BinOp(ref left, _, _) = cond {
                        subject = Some(left.as_ref().clone());
                    }
                }
                cond
            };

            self.expect(&Token::Colon)?;
            branches.push((Box::new(condition), Box::new(self.parse_or()?)));

            if !matches!(self.peek(), Some(Token::Comma)) {
                bail!("expected ',' or 'else' in conditional");
            }
            self.advance();

            if matches!(self.peek(), Some(Token::Else)) {
                self.advance();
                self.expect(&Token::Colon)?;
                let fallback = self.parse_or()?;
                self.leave();
                return Ok(Expr::Cond { branches, fallback: Box::new(fallback) });
            }
        }
    }

    fn peek_is_comparison_op(&self) -> bool {
        matches!(self.peek(), Some(Token::Lt | Token::Gt | Token::Le | Token::Ge | Token::Eq | Token::Ne))
    }

    fn parse_comparison_op(&mut self) -> Result<BinOp> {
        match self.advance() {
            Some(Token::Lt) => Ok(BinOp::Lt),
            Some(Token::Gt) => Ok(BinOp::Gt),
            Some(Token::Le) => Ok(BinOp::Le),
            Some(Token::Ge) => Ok(BinOp::Ge),
            Some(Token::Eq) => Ok(BinOp::Eq),
            Some(Token::Ne) => Ok(BinOp::Ne),
            other => bail!("expected comparison operator, got {other:?}"),
        }
    }
}


#[derive(Debug, Clone)]
pub enum EvalResult {
    Number(f64),
    Text(String),
    Bool(bool),
    Null,
}


impl EvalResult {
    fn as_number(&self) -> f64 {
        match self {
            EvalResult::Number(n) => *n,
            EvalResult::Bool(true) => 1.0,
            _ => 0.0,
        }
    }

    fn as_bool(&self) -> bool {
        match self {
            EvalResult::Bool(b) => *b,
            EvalResult::Number(n) => *n != 0.0,
            EvalResult::Text(s) => !s.is_empty(),
            EvalResult::Null => false,
        }
    }

    fn format(&self, fmt: &str) -> String {
        match self {
            EvalResult::Number(n) => match parse_format_spec(fmt) {
                Some(precision) => format!("{n:.precision$}"),
                None => format!("{n}"),
            },
            EvalResult::Text(s) => s.clone(),
            EvalResult::Bool(b) => b.to_string(),
            EvalResult::Null => String::new(),
        }
    }
}


impl std::fmt::Display for EvalResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalResult::Number(n) if *n == (*n as i64) as f64 => write!(f, "{}", *n as i64),
            EvalResult::Number(n) => write!(f, "{n}"),
            EvalResult::Text(s) => write!(f, "{s}"),
            EvalResult::Bool(b) => write!(f, "{b}"),
            EvalResult::Null => Ok(()),
        }
    }
}


fn parse_format_spec(fmt: &str) -> Option<usize> {
    let fmt = fmt.trim();
    if fmt.starts_with('.') && fmt.ends_with('f') {
        fmt[1..fmt.len() - 1].parse().ok()
    } else {
        None
    }
}


fn resolve_field(ctx: &Value, path: &[String]) -> EvalResult {
    let mut current = ctx;
    for key in path {
        match current.get(key) {
            Some(v) => current = v,
            None => return EvalResult::Null,
        }
    }
    value_to_result(current)
}


fn value_to_result(v: &Value) -> EvalResult {
    match v {
        Value::Number(n) => EvalResult::Number(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => EvalResult::Text(s.clone()),
        Value::Bool(b) => EvalResult::Bool(*b),
        Value::Null => EvalResult::Null,
        _ => EvalResult::Text(v.to_string()),
    }
}


fn eval(expr: &Expr, ctx: &Value) -> EvalResult {
    match expr {
        Expr::Number(n) => EvalResult::Number(*n),
        Expr::String(s) => EvalResult::Text(s.clone()),
        Expr::Field(path) => resolve_field(ctx, path),
        Expr::UnaryNot(inner) => EvalResult::Bool(!eval(inner, ctx).as_bool()),
        Expr::Cond { branches, fallback } => {
            for (condition, value) in branches {
                if eval(condition, ctx).as_bool() {
                    return eval(value, ctx);
                }
            }
            eval(fallback, ctx)
        }
        Expr::BinOp(left, op, right) => {
            let l = eval(left, ctx);
            let r = eval(right, ctx);
            match op {
                BinOp::Add => EvalResult::Number(l.as_number() + r.as_number()),
                BinOp::Sub => EvalResult::Number(l.as_number() - r.as_number()),
                BinOp::Mul => EvalResult::Number(l.as_number() * r.as_number()),
                BinOp::Div => {
                    let d = r.as_number();
                    EvalResult::Number(if d == 0.0 { 0.0 } else { l.as_number() / d })
                }
                BinOp::Mod => {
                    let d = r.as_number();
                    EvalResult::Number(if d == 0.0 { 0.0 } else { l.as_number() % d })
                }
                BinOp::Gt => EvalResult::Bool(l.as_number() > r.as_number()),
                BinOp::Lt => EvalResult::Bool(l.as_number() < r.as_number()),
                BinOp::Ge => EvalResult::Bool(l.as_number() >= r.as_number()),
                BinOp::Le => EvalResult::Bool(l.as_number() <= r.as_number()),
                BinOp::Eq => match (&l, &r) {
                    (EvalResult::Null, EvalResult::Null) => EvalResult::Bool(true),
                    (EvalResult::Null, _) | (_, EvalResult::Null) => EvalResult::Bool(false),
                    (EvalResult::Text(a), EvalResult::Text(b)) => EvalResult::Bool(a == b),
                    _ => EvalResult::Bool(l.as_number() == r.as_number()),
                },
                BinOp::Ne => match (&l, &r) {
                    (EvalResult::Null, EvalResult::Null) => EvalResult::Bool(false),
                    (EvalResult::Null, _) | (_, EvalResult::Null) => EvalResult::Bool(true),
                    (EvalResult::Text(a), EvalResult::Text(b)) => EvalResult::Bool(a != b),
                    _ => EvalResult::Bool(l.as_number() != r.as_number()),
                },
                BinOp::And => EvalResult::Bool(l.as_bool() && r.as_bool()),
                BinOp::Or => EvalResult::Bool(l.as_bool() || r.as_bool()),
            }
        }
    }
}


pub struct RenderedNickname {
    pub before: String,
    pub truncatable: Option<String>,
    pub after: String,
}


impl RenderedNickname {
    pub fn to_truncated(&self, max_len: usize) -> String {
        let Some(truncatable) = &self.truncatable else {
            return truncate_str(&format!("{}{}", self.before, self.after), max_len);
        };

        let full = format!("{}{}{}", self.before, truncatable, self.after);
        if full.len() <= max_len {
            return full;
        }

        let budget = max_len.saturating_sub(self.before.len() + self.after.len());
        if budget == 0 {
            return truncate_str(&format!("{}{}", self.before, self.after), max_len);
        }

        format!("{}{}{}", self.before, truncate_str(truncatable, budget), self.after)
    }
}


fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len { return s.to_string() }
    let mut end = max_len;
    while !s.is_char_boundary(end) { end -= 1 }
    s[..end].trim_end().to_string()
}


pub fn render_template(template: &str, ctx: &Value) -> RenderedNickname {
    let mut output = String::new();
    let mut truncatable_segment = None;
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() && output.len() < MAX_OUTPUT_LEN {
        if chars[i] == '{' {
            let (inner, end) = extract_brace_content(&chars, i + 1);
            i = end;

            let trimmed = inner.trim();
            if trimmed.starts_with("..") {
                let expr_str = trimmed[2..].trim();
                let formatted = match tokenize(expr_str).and_then(|t| Parser::new(t).parse()) {
                    Ok(expr) => eval(&expr, ctx).to_string(),
                    Err(_) => format!("{{..{expr_str}}}"),
                };
                truncatable_segment = Some((output.len(), formatted.len()));
                output.push_str(&formatted);
                continue;
            }

            let (expr_str, format_spec) = split_format_spec(&inner);
            match tokenize(expr_str).and_then(|t| Parser::new(t).parse()) {
                Ok(expr) => {
                    let result = eval(&expr, ctx);
                    output.push_str(&match &format_spec {
                        Some(spec) => result.format(spec),
                        None => result.to_string(),
                    });
                }
                Err(_) => {
                    output.push('{');
                    output.push_str(&inner);
                    output.push('}');
                }
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    output.truncate(MAX_OUTPUT_LEN);

    match truncatable_segment {
        Some((start, len)) => {
            let end = (start + len).min(output.len());
            RenderedNickname {
                before: output[..start].to_string(),
                truncatable: Some(output[start..end].to_string()),
                after: output[end..].to_string(),
            }
        }
        None => RenderedNickname {
            before: output,
            truncatable: None,
            after: String::new(),
        },
    }
}


pub fn eval_condition(condition: &str, ctx: &Value) -> Result<bool> {
    let tokens = tokenize(condition)?;
    Ok(eval(&Parser::new(tokens).parse()?, ctx).as_bool())
}


pub fn validate_condition(condition: &str) -> Result<()> {
    Parser::new(tokenize(condition)?).parse()?;
    Ok(())
}


pub fn validate_template(template: &str) -> Result<()> {
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            let (inner, end) = extract_brace_content(&chars, i + 1);
            i = end;
            let trimmed = inner.trim();
            let expr_input = trimmed.strip_prefix("..").map(|r| r.trim()).unwrap_or(trimmed);
            let (expr_str, _) = split_format_spec(expr_input);
            Parser::new(tokenize(expr_str)?).parse()?;
        } else {
            i += 1;
        }
    }

    Ok(())
}


fn extract_brace_content(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    let mut depth = 1;
    while i < chars.len() && depth > 0 {
        match chars[i] {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth > 0 { i += 1 }
    }
    let inner: String = chars[start..i].iter().collect();
    (inner, if i < chars.len() { i + 1 } else { i })
}


fn split_format_spec(inner: &str) -> (&str, Option<String>) {
    match inner.rfind(':') {
        Some(colon) => {
            let after = inner[colon + 1..].trim();
            if after.starts_with('.') && after.ends_with('f') {
                (&inner[..colon], Some(after.to_string()))
            } else {
                (inner, None)
            }
        }
        None => (inner, None),
    }
}
