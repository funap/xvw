#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Number(i64),
    Float(f64),
    Identifier(String),
    String(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equal,
    NotEqual,
    GreaterEqual,
    LessEqual,
    Greater,
    Less,
    And,
    Or,
    Amp,
    Pipe,
    Bang,
    Shl,
    Shr,
    Caret,
    Tilde,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Comma,
    Question,
    Colon,
    ColonColon,
    #[allow(clippy::upper_case_acronyms)]
    EOF,
}

pub(super) struct Lexer<'a> {
    pub(super) input: &'a str,
    pub(super) pos: usize,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    pub(super) fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return Token::EOF;
        }

        let ch = self.current_char();
        if ch.is_ascii_digit() {
            return self.lex_number();
        }
        if ch.is_alphabetic() || ch == '_' {
            return self.lex_identifier();
        }
        if ch == '\'' || ch == '"' {
            return self.lex_string();
        }

        self.pos += ch.len_utf8();
        match ch {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '.' => Token::Dot,
            ',' => Token::Comma,
            '?' => Token::Question,
            '^' => Token::Caret,
            '~' => Token::Tilde,
            ':' => {
                if self.pos < self.input.len() && self.current_char() == ':' {
                    self.pos += 1;
                    Token::ColonColon
                } else {
                    Token::Colon
                }
            }
            '!' => {
                if self.pos < self.input.len() && self.current_char() == '=' {
                    self.pos += 1;
                    Token::NotEqual
                } else {
                    Token::Bang
                }
            }
            '=' => {
                if self.pos < self.input.len() && self.current_char() == '=' {
                    self.pos += 1;
                }
                Token::Equal
            }
            '>' => {
                if self.pos < self.input.len() && self.current_char() == '=' {
                    self.pos += 1;
                    Token::GreaterEqual
                } else if self.pos < self.input.len() && self.current_char() == '>' {
                    self.pos += 1;
                    Token::Shr
                } else {
                    Token::Greater
                }
            }
            '<' => {
                if self.pos < self.input.len() && self.current_char() == '=' {
                    self.pos += 1;
                    Token::LessEqual
                } else if self.pos < self.input.len() && self.current_char() == '<' {
                    self.pos += 1;
                    Token::Shl
                } else {
                    Token::Less
                }
            }
            '&' => {
                if self.pos < self.input.len() && self.current_char() == '&' {
                    self.pos += 1;
                    Token::And
                } else {
                    Token::Amp
                }
            }
            '|' => {
                if self.pos < self.input.len() && self.current_char() == '|' {
                    self.pos += 1;
                    Token::Or
                } else {
                    Token::Pipe
                }
            }
            _ => Token::EOF,
        }
    }

    fn current_char(&self) -> char {
        self.input[self.pos..].chars().next().unwrap_or('\0')
    }

    fn skip_whitespace(&mut self) {
        loop {
            while self.pos < self.input.len() && self.current_char().is_whitespace() {
                self.pos += self.current_char().len_utf8();
            }
            if self.pos < self.input.len() && self.current_char() == '#' {
                while self.pos < self.input.len() && self.current_char() != '\n' {
                    self.pos += self.current_char().len_utf8();
                }
            } else {
                break;
            }
        }
    }

    fn lex_number(&mut self) -> Token {
        let start = self.pos;
        if self.current_char() == '0' {
            self.pos += 1;
            if self.pos < self.input.len() {
                let next = self.current_char();
                if next == 'x' || next == 'X' {
                    self.pos += 1;
                    let hex_start = self.pos;
                    while self.pos < self.input.len() && (self.current_char().is_ascii_hexdigit() || self.current_char() == '_') {
                        self.pos += 1;
                    }
                    let hex_str: String = self.input[hex_start..self.pos].chars().filter(|c| *c != '_').collect();
                    let val = u64::from_str_radix(&hex_str, 16).map(|v| v as i64).unwrap_or(0);
                    return Token::Number(val);
                }
                if next == 'b' || next == 'B' {
                    self.pos += 1;
                    let bin_start = self.pos;
                    while self.pos < self.input.len() && (self.current_char() == '0' || self.current_char() == '1' || self.current_char() == '_') {
                        self.pos += 1;
                    }
                    let bin_str: String = self.input[bin_start..self.pos].chars().filter(|c| *c != '_').collect();
                    let val = u64::from_str_radix(&bin_str, 2).map(|v| v as i64).unwrap_or(0);
                    return Token::Number(val);
                }
                if next == 'o' || next == 'O' {
                    self.pos += 1;
                    let oct_start = self.pos;
                    while self.pos < self.input.len() && (matches!(self.current_char(), '0'..='7') || self.current_char() == '_') {
                        self.pos += 1;
                    }
                    let oct_str: String = self.input[oct_start..self.pos].chars().filter(|c| *c != '_').collect();
                    let val = u64::from_str_radix(&oct_str, 8).map(|v| v as i64).unwrap_or(0);
                    return Token::Number(val);
                }
            }
        }
        while self.pos < self.input.len() && (self.current_char().is_ascii_digit() || self.current_char() == '_') {
            self.pos += 1;
        }
        let mut is_float = false;
        // Check for fractional part
        if self.pos < self.input.len() && self.current_char() == '.' {
            let next_pos = self.pos + 1;
            if next_pos < self.input.len() && self.input[next_pos..].chars().next().is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                self.pos += 1; // consume '.'
                while self.pos < self.input.len() && (self.current_char().is_ascii_digit() || self.current_char() == '_') {
                    self.pos += 1;
                }
            }
        }
        // Check for scientific notation exponent (e.g. 1.5e3, 2e-4)
        if self.pos < self.input.len() && (self.current_char() == 'e' || self.current_char() == 'E') {
            let mut exp_pos = self.pos + 1;
            if exp_pos < self.input.len() {
                let sign_ch = self.input[exp_pos..].chars().next().unwrap_or('\0');
                if sign_ch == '+' || sign_ch == '-' {
                    exp_pos += 1;
                }
                if exp_pos < self.input.len() && self.input[exp_pos..].chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    is_float = true;
                    self.pos = exp_pos;
                    while self.pos < self.input.len() && (self.current_char().is_ascii_digit() || self.current_char() == '_') {
                        self.pos += 1;
                    }
                }
            }
        }
        let clean_str: String = self.input[start..self.pos].chars().filter(|c| *c != '_').collect();
        if is_float {
            let val: f64 = clean_str.parse().unwrap_or(0.0);
            return Token::Float(val);
        }
        let val = clean_str.parse::<i64>().or_else(|_| clean_str.parse::<u64>().map(|v| v as i64)).unwrap_or(0);
        Token::Number(val)
    }

    fn lex_identifier(&mut self) -> Token {
        let start = self.pos;
        while self.pos < self.input.len() && (self.current_char().is_alphanumeric() || self.current_char() == '_') {
            self.pos += self.current_char().len_utf8();
        }
        Token::Identifier(self.input[start..self.pos].to_string())
    }

    fn lex_string(&mut self) -> Token {
        let quote = self.current_char();
        self.pos += quote.len_utf8();
        if quote == '\'' {
            let start = self.pos;
            while self.pos < self.input.len() && self.current_char() != quote {
                self.pos += self.current_char().len_utf8();
            }
            let s = self.input[start..self.pos].to_string();
            if self.pos < self.input.len() {
                self.pos += quote.len_utf8();
            }
            return Token::String(s);
        }

        let mut s = String::new();
        while self.pos < self.input.len() {
            let ch = self.current_char();
            if ch == quote {
                self.pos += ch.len_utf8();
                break;
            }
            if ch == '\\' {
                self.pos += 1;
                if self.pos >= self.input.len() {
                    break;
                }
                let esc = self.current_char();
                self.pos += esc.len_utf8();
                match esc {
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    '\\' => s.push('\\'),
                    '"' => s.push('"'),
                    '\'' => s.push('\''),
                    'a' => s.push('\x07'),
                    'b' => s.push('\x08'),
                    'v' => s.push('\x0b'),
                    'f' => s.push('\x0c'),
                    'e' => s.push('\x1b'),
                    'x' => {
                        let hex_start = self.pos;
                        let mut count = 0;
                        while count < 2 && self.pos < self.input.len() && self.current_char().is_ascii_hexdigit() {
                            self.pos += 1;
                            count += 1;
                        }
                        if let Ok(code) = u32::from_str_radix(&self.input[hex_start..self.pos], 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            s.push(c);
                        }
                    }
                    'u' => {
                        let hex_start = self.pos;
                        let mut count = 0;
                        while count < 4 && self.pos < self.input.len() && self.current_char().is_ascii_hexdigit() {
                            self.pos += 1;
                            count += 1;
                        }
                        if let Ok(code) = u32::from_str_radix(&self.input[hex_start..self.pos], 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            s.push(c);
                        }
                    }
                    '0'..='7' => {
                        let mut oct_str = String::from(esc);
                        let mut count = 1;
                        while count < 3 && self.pos < self.input.len() && matches!(self.current_char(), '0'..='7') {
                            oct_str.push(self.current_char());
                            self.pos += 1;
                            count += 1;
                        }
                        if let Ok(code) = u32::from_str_radix(&oct_str, 8)
                            && let Some(c) = char::from_u32(code)
                        {
                            s.push(c);
                        }
                    }
                    other => s.push(other),
                }
            } else {
                s.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Token::String(s)
    }
}
