//! Turns NWScript source text into tokens.
//!
//! # Rules that are not C
//!
//! Every one of these is what the reference compiler does. Do not "fix" any of
//! them; existing mod source depends on all of them.
//!
//! - **`\n` is the only escape sequence in a string.** Any other backslash is
//!   dropped and the character after it is processed normally, so `\t` is the
//!   letter `t` and — the one that surprises people — `\"` *terminates* the
//!   string rather than embedding a quote.
//! - **A string cannot span a line.** Hitting the end of the line is
//!   `Unterminated string`, and the partial text is still returned as a token.
//! - **Only bytes `0x21..=0x7e` are source characters.** Everything else,
//!   including every byte of a UTF-8 or Windows-1252 accented character, is
//!   whitespace outside a string literal. Inside one it passes through.
//! - **Floats have no exponent and no leading dot.** The scanner takes digits
//!   with at most one `.` and an optional trailing `f`. So `1e3` is the integer
//!   `1` followed by the identifier `e3`, `.5` is `.` followed by `5`, `1.` is
//!   the float `1.0`, and `5f` is the float `5.0`.
//! - **`INT` is `int` and `OBJECT_ID` is `object`.** They are keywords, not
//!   typedefs, and they are case sensitive: `Int` is an identifier.
//! - **Engine structure names are not keywords.** `effect` and friends are
//!   whatever `#define ENGINE_STRUCTURE_<n>` declared, which is why scanning
//!   takes an [`EngineStructures`] table.
//! - **An unrecognised character is a warning, not an error.** `#` produces
//!   `Invalid character '#' (0x23) found in source, ignored` and scanning
//!   continues, which is how a leftover TSLPatcher `#2DAMEMORY5` token turns
//!   into a syntax error about `DAMEMORY5`.

use crate::error::{CompileError, Diagnostic, Position, Severity, Span, Stage};
use crate::preprocessor::EngineStructures;

/// A reserved word.
///
/// The reference compiler keeps these in the same table as the engine structure
/// names, which is why an engine structure may not be named after one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Keyword {
    /// `int`, or its synonym `INT`.
    Int,
    Float,
    /// `object`, or its synonym `OBJECT_ID`.
    Object,
    /// `string`.
    Str,
    Struct,
    Void,
    Vector,
    Action,
    Const,
    Break,
    Case,
    Continue,
    Default,
    Do,
    Else,
    For,
    If,
    Return,
    Switch,
    While,
    ObjectSelf,
    ObjectInvalid,
}

impl Keyword {
    /// Returns the keyword's canonical spelling, for diagnostics.
    ///
    /// A source that wrote `INT` reports as `int`, matching the reference
    /// compiler's `Syntax error at "int"`.
    pub fn as_str(self) -> &'static str {
        let _budget_frame = crate::budget::enter();
        match self {
            Keyword::Int => "int",
            Keyword::Float => "float",
            Keyword::Object => "object",
            Keyword::Str => "string",
            Keyword::Struct => "struct",
            Keyword::Void => "void",
            Keyword::Vector => "vector",
            Keyword::Action => "action",
            Keyword::Const => "const",
            Keyword::Break => "break",
            Keyword::Case => "case",
            Keyword::Continue => "continue",
            Keyword::Default => "default",
            Keyword::Do => "do",
            Keyword::Else => "else",
            Keyword::For => "for",
            Keyword::If => "if",
            Keyword::Return => "return",
            Keyword::Switch => "switch",
            Keyword::While => "while",
            Keyword::ObjectSelf => "OBJECT_SELF",
            Keyword::ObjectInvalid => "OBJECT_INVALID",
        }
    }

    /// Returns true when the keyword can start a type.
    pub fn starts_type(self) -> bool {
        let _budget_frame = crate::budget::enter();
        matches!(
            self,
            Keyword::Int
                | Keyword::Float
                | Keyword::Object
                | Keyword::Str
                | Keyword::Struct
                | Keyword::Void
                | Keyword::Vector
                | Keyword::Action
        )
    }

    /// Looks a word up in the reserved word table.
    ///
    /// Returns `None` for anything the table does not hold, including engine
    /// structure names, which live in [`EngineStructures`].
    pub fn lookup(word: &str) -> Option<Keyword> {
        let _budget_frame = crate::budget::enter();
        Some(match word {
            "int" | "INT" => Keyword::Int,
            "float" => Keyword::Float,
            "object" | "OBJECT_ID" => Keyword::Object,
            "string" => Keyword::Str,
            "struct" => Keyword::Struct,
            "void" => Keyword::Void,
            "vector" => Keyword::Vector,
            "action" => Keyword::Action,
            "const" => Keyword::Const,
            "break" => Keyword::Break,
            "case" => Keyword::Case,
            "continue" => Keyword::Continue,
            "default" => Keyword::Default,
            "do" => Keyword::Do,
            "else" => Keyword::Else,
            "for" => Keyword::For,
            "if" => Keyword::If,
            "return" => Keyword::Return,
            "switch" => Keyword::Switch,
            "while" => Keyword::While,
            "OBJECT_SELF" => Keyword::ObjectSelf,
            "OBJECT_INVALID" => Keyword::ObjectInvalid,
            _ => return None,
        })
    }
}

/// What kind of token this is.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An identifier that is not a reserved word.
    Identifier(String),
    /// A reserved word.
    Keyword(Keyword),
    /// An engine structure type name, with the index its `#define` gave it.
    EngineType {
        /// The `<n>` from `ENGINE_STRUCTURE_<n>`.
        index: u8,
        /// The name as written.
        name: String,
    },
    /// An integer literal, already converted.
    Int(i32),
    /// A float literal, already converted.
    Float(f32),
    /// A string literal with the quotes removed and `\n` already applied.
    ///
    /// The bytes are the source's own bytes, not UTF-8. `CONSTS` stores a
    /// length in bytes and the game reads Windows-1252, so `"café"` has to
    /// reach the byte stream as four bytes ending in `0xe9`. Decoding it to
    /// text on the way through would make it five, and lose the original
    /// bytes for anything the decoder could not name.
    Str(Vec<u8>),
    /// Punctuation or an operator.
    Punct(Punct),
    /// The end of the token stream.
    Eof,
}

impl TokenKind {
    /// Returns what `Syntax error at "%s"` should print for this token.
    ///
    /// Identifiers print their own text. Literals print a placeholder, because
    /// the reference compiler's parse stack does not keep their source text.
    pub fn describe(&self) -> &str {
        let _budget_frame = crate::budget::enter();
        match self {
            TokenKind::Identifier(name) => name,
            TokenKind::Keyword(keyword) => keyword.as_str(),
            TokenKind::EngineType { name, .. } => name,
            TokenKind::Int(_) => "integer constant",
            TokenKind::Float(_) => "float constant",
            TokenKind::Str(_) => "string constant",
            TokenKind::Punct(punct) => punct.as_str(),
            TokenKind::Eof => "Unknown token",
        }
    }
}

/// Punctuation and operators, in the set NWScript actually uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Punct {
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Question,
    Colon,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    Increment,
    Decrement,
    Eq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    AndAnd,
    OrOr,
    Not,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    /// `>>>`, unsigned right shift. NWScript has no C equivalent for it.
    UShr,
    AmpAssign,
    PipeAssign,
    CaretAssign,
    ShlAssign,
    ShrAssign,
    UShrAssign,
}

impl Punct {
    /// Returns the operator as it is written.
    pub fn as_str(self) -> &'static str {
        let _budget_frame = crate::budget::enter();
        match self {
            Punct::LParen => "(",
            Punct::RParen => ")",
            Punct::LBrace => "{",
            Punct::RBrace => "}",
            Punct::LBracket => "[",
            Punct::RBracket => "]",
            Punct::Semicolon => ";",
            Punct::Comma => ",",
            Punct::Dot => ".",
            Punct::Question => "?",
            Punct::Colon => ":",
            Punct::Assign => "=",
            Punct::Plus => "+",
            Punct::Minus => "-",
            Punct::Star => "*",
            Punct::Slash => "/",
            Punct::Percent => "%",
            Punct::PlusAssign => "+=",
            Punct::MinusAssign => "-=",
            Punct::StarAssign => "*=",
            Punct::SlashAssign => "/=",
            Punct::PercentAssign => "%=",
            Punct::Increment => "++",
            Punct::Decrement => "--",
            Punct::Eq => "==",
            Punct::NotEq => "!=",
            Punct::Less => "<",
            Punct::LessEq => "<=",
            Punct::Greater => ">",
            Punct::GreaterEq => ">=",
            Punct::AndAnd => "&&",
            Punct::OrOr => "||",
            Punct::Not => "!",
            Punct::Amp => "&",
            Punct::Pipe => "|",
            Punct::Caret => "^",
            Punct::Tilde => "~",
            Punct::Shl => "<<",
            Punct::Shr => ">>",
            Punct::UShr => ">>>",
            Punct::AmpAssign => "&=",
            Punct::PipeAssign => "|=",
            Punct::CaretAssign => "^=",
            Punct::ShlAssign => "<<=",
            Punct::ShrAssign => ">>=",
            Punct::UShrAssign => ">>>=",
        }
    }
}

/// One token plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Where the token sits in the source text.
    pub span: Span,
}

impl Token {
    /// Builds a token.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        let _budget_frame = crate::budget::enter();
        crate::budget::token();
        Token { kind, span }
    }

    /// Returns true when this token ends the stream.
    pub fn is_eof(&self) -> bool {
        let _budget_frame = crate::budget::enter();
        matches!(self.kind, TokenKind::Eof)
    }

    /// Returns true when this token is `punct`.
    pub fn is_punct(&self, punct: Punct) -> bool {
        let _budget_frame = crate::budget::enter();
        self.kind == TokenKind::Punct(punct)
    }

    /// Returns true when this token is `keyword`.
    pub fn is_keyword(&self, keyword: Keyword) -> bool {
        let _budget_frame = crate::budget::enter();
        self.kind == TokenKind::Keyword(keyword)
    }
}

/// What one scan produced.
#[derive(Debug, Clone, Default)]
pub struct Lexed {
    /// The tokens, always ending with [`TokenKind::Eof`].
    pub tokens: Vec<Token>,
    /// Every problem found. Scanning does not stop at the first one.
    pub diagnostics: Vec<Diagnostic>,
}

impl Lexed {
    /// Returns true when at least one diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        let _budget_frame = crate::budget::enter();
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }

    /// Returns only the warnings.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        let _budget_frame = crate::budget::enter();
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Warning)
    }
}

/// Scans source text into [`Token`]s.
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    engine: EngineStructures,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    /// Builds a lexer over `source` with no engine structure types declared.
    pub fn new(source: &'a str) -> Self {
        let _budget_frame = crate::budget::enter();
        Lexer::with_engine_structures(source, EngineStructures::new())
    }

    /// Builds a lexer that knows the engine structure types `#define` declared.
    pub fn with_engine_structures(source: &'a str, engine: EngineStructures) -> Self {
        let _budget_frame = crate::budget::enter();
        Lexer {
            source,
            offset: 0,
            line: 1,
            column: 1,
            engine,
            diagnostics: Vec::new(),
        }
    }

    /// Returns the full source text.
    pub fn source(&self) -> &'a str {
        let _budget_frame = crate::budget::enter();
        self.source
    }

    /// Returns the byte offset the lexer has reached.
    pub fn offset(&self) -> usize {
        let _budget_frame = crate::budget::enter();
        self.offset
    }

    /// Returns the text the lexer has not consumed.
    pub fn remaining(&self) -> &'a str {
        let _budget_frame = crate::budget::enter();
        &self.source[self.offset.min(self.source.len())..]
    }

    /// Scans the whole input, collecting problems rather than stopping.
    ///
    /// The returned token list always ends with [`TokenKind::Eof`].
    pub fn scan(&mut self) -> Lexed {
        let _budget_frame = crate::budget::enter();
        let mut tokens = Vec::new();
        loop {
            crate::budget::tick();
            let token = self.next_token();
            let done = token.is_eof();
            tokens.push(token);
            if done {
                break;
            }
        }
        Lexed {
            tokens,
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    /// Scans the whole input.
    ///
    /// # Errors
    ///
    /// Returns every lexing error found, not just the first. Warnings are
    /// dropped; use [`Lexer::scan`] to keep them.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, CompileError> {
        let _budget_frame = crate::budget::enter();
        let lexed = self.scan();
        if lexed.has_errors() {
            return Err(CompileError::Diagnostics(lexed.diagnostics));
        }
        Ok(lexed.tokens)
    }

    fn next_token(&mut self) -> Token {
        let _budget_frame = crate::budget::enter();
        loop {
            crate::budget::tick();
            self.skip_trivia();
            let start = self.position();
            let Some(byte) = self.peek() else {
                return Token::new(TokenKind::Eof, Span::point(start));
            };

            if byte.is_ascii_alphabetic() || byte == b'_' {
                return self.scan_word(start);
            }
            if byte.is_ascii_digit() {
                return self.scan_number(start);
            }
            if byte == b'"' {
                self.bump();
                return self.scan_string(start);
            }
            if let Some(punct) = self.scan_punct() {
                return Token::new(TokenKind::Punct(punct), self.span_from(start));
            }

            // Either a comment was consumed, or the character is not part of
            // the language. Either way, go around again.
        }
    }

    /// Skips whitespace and comments.
    fn skip_trivia(&mut self) {
        let _budget_frame = crate::budget::enter();
        loop {
            crate::budget::tick();
            match self.peek() {
                Some(byte) if !is_source_character(byte) => {
                    self.bump();
                }
                _ => return,
            }
        }
    }

    fn scan_word(&mut self, start: Position) -> Token {
        let _budget_frame = crate::budget::enter();
        let begin = self.offset;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_') {
            crate::budget::tick();
            self.bump();
        }
        let word = &self.source[begin..self.offset];
        let span = self.span_from(start);

        if let Some(keyword) = Keyword::lookup(word) {
            return Token::new(TokenKind::Keyword(keyword), span);
        }
        if let Some(index) = self.engine.lookup(word) {
            return Token::new(
                TokenKind::EngineType {
                    index,
                    name: word.to_string(),
                },
                span,
            );
        }
        Token::new(TokenKind::Identifier(word.to_string()), span)
    }

    fn scan_number(&mut self, start: Position) -> Token {
        let _budget_frame = crate::budget::enter();
        if self.peek() == Some(b'0') && matches!(self.peek_at(1), Some(b'x') | Some(b'X')) {
            self.bump();
            self.bump();
            let mut value: i32 = 0;
            while let Some(digit) = self.peek().and_then(hex_digit) {
                crate::budget::tick();
                value = value.wrapping_mul(16).wrapping_add(digit as i32);
                self.bump();
            }
            return Token::new(TokenKind::Int(value), self.span_from(start));
        }

        let begin = self.offset;
        let mut has_decimal = false;
        loop {
            crate::budget::tick();
            match self.peek() {
                Some(byte) if byte.is_ascii_digit() => self.bump(),
                Some(b'.') if !has_decimal => {
                    has_decimal = true;
                    self.bump()
                }
                _ => break,
            };
        }

        // The `f` suffix is not part of the text that gets converted, but it
        // does make an otherwise integral literal a float. `5f` is `5.0`.
        let text = &self.source[begin..self.offset];
        if matches!(self.peek(), Some(b'f') | Some(b'F')) {
            has_decimal = true;
            self.bump();
        }

        let kind = if has_decimal {
            // `(float) atof (psz)`: convert to double, then round once to
            // float. Going straight to `f32` rounds twice and can land a ULP
            // out on a value like `3.67`.
            TokenKind::Float(text.parse::<f64>().unwrap_or(0.0) as f32)
        } else {
            TokenKind::Int(parse_decimal(text))
        };
        Token::new(kind, self.span_from(start))
    }

    /// Scans a string literal. The opening quote is already consumed.
    fn scan_string(&mut self, start: Position) -> Token {
        let _budget_frame = crate::budget::enter();
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            crate::budget::tick();
            let Some(byte) = self.peek() else {
                return self.unterminated_string(start, bytes);
            };
            if byte == b'\n' || byte == b'\r' {
                return self.unterminated_string(start, bytes);
            }
            self.bump();

            match byte {
                b'"' => break,
                b'\\' => {
                    // The only escape. Every other backslash is dropped and the
                    // character after it goes around the loop untouched, so
                    // `\"` closes the string.
                    if self.peek() == Some(b'n') {
                        self.bump();
                        bytes.push(b'\n');
                    }
                }
                other => bytes.push(other),
            }
        }

        Token::new(TokenKind::Str(source_bytes(&bytes)), self.span_from(start))
    }

    fn unterminated_string(&mut self, start: Position, bytes: Vec<u8>) -> Token {
        let _budget_frame = crate::budget::enter();
        let span = self.span_from(start);
        self.error(span, "Unterminated string");
        Token::new(TokenKind::Str(source_bytes(&bytes)), span)
    }

    /// Scans one operator, or consumes a comment and returns `None`.
    fn scan_punct(&mut self) -> Option<Punct> {
        let _budget_frame = crate::budget::enter();
        let start = self.position();
        let byte = self.peek()?;
        self.bump();

        let punct = match byte {
            b'/' => match self.peek() {
                Some(b'*') => {
                    self.bump();
                    self.skip_block_comment(start);
                    return None;
                }
                Some(b'/') => {
                    self.skip_line_comment();
                    return None;
                }
                Some(b'=') => {
                    self.bump();
                    Punct::SlashAssign
                }
                _ => Punct::Slash,
            },
            b'{' => Punct::LBrace,
            b'}' => Punct::RBrace,
            b'[' => Punct::LBracket,
            b']' => Punct::RBracket,
            b'(' => Punct::LParen,
            b')' => Punct::RParen,
            b';' => Punct::Semicolon,
            b':' => Punct::Colon,
            b'?' => Punct::Question,
            b',' => Punct::Comma,
            b'~' => Punct::Tilde,
            b'.' => Punct::Dot,
            b'+' => self.pick(
                &[(b'=', Punct::PlusAssign), (b'+', Punct::Increment)],
                Punct::Plus,
            ),
            b'-' => self.pick(
                &[(b'=', Punct::MinusAssign), (b'-', Punct::Decrement)],
                Punct::Minus,
            ),
            b'*' => self.pick(&[(b'=', Punct::StarAssign)], Punct::Star),
            b'%' => self.pick(&[(b'=', Punct::PercentAssign)], Punct::Percent),
            b'^' => self.pick(&[(b'=', Punct::CaretAssign)], Punct::Caret),
            b'&' => self.pick(
                &[(b'=', Punct::AmpAssign), (b'&', Punct::AndAnd)],
                Punct::Amp,
            ),
            b'|' => self.pick(
                &[(b'=', Punct::PipeAssign), (b'|', Punct::OrOr)],
                Punct::Pipe,
            ),
            b'!' => self.pick(&[(b'=', Punct::NotEq)], Punct::Not),
            b'=' => self.pick(&[(b'=', Punct::Eq)], Punct::Assign),
            b'<' => match self.peek() {
                Some(b'=') => {
                    self.bump();
                    Punct::LessEq
                }
                Some(b'<') => {
                    self.bump();
                    self.pick(&[(b'=', Punct::ShlAssign)], Punct::Shl)
                }
                _ => Punct::Less,
            },
            b'>' => match self.peek() {
                Some(b'=') => {
                    self.bump();
                    Punct::GreaterEq
                }
                Some(b'>') => {
                    self.bump();
                    match self.peek() {
                        Some(b'=') => {
                            self.bump();
                            Punct::ShrAssign
                        }
                        Some(b'>') => {
                            self.bump();
                            self.pick(&[(b'=', Punct::UShrAssign)], Punct::UShr)
                        }
                        _ => Punct::Shr,
                    }
                }
                _ => Punct::Greater,
            },
            other => {
                let span = self.span_from(start);
                self.warning(
                    span,
                    format!(
                        "Invalid character '{}' (0x{other:02X}) found in source, ignored",
                        other as char
                    ),
                );
                return None;
            }
        };
        Some(punct)
    }

    /// Consumes the next byte when it matches one of `choices`.
    fn pick(&mut self, choices: &[(u8, Punct)], fallback: Punct) -> Punct {
        let _budget_frame = crate::budget::enter();
        let Some(byte) = self.peek() else {
            return fallback;
        };
        for (candidate, punct) in choices {
            crate::budget::tick();
            if byte == *candidate {
                self.bump();
                return *punct;
            }
        }
        fallback
    }

    fn skip_line_comment(&mut self) {
        let _budget_frame = crate::budget::enter();
        while let Some(byte) = self.peek() {
            crate::budget::tick();
            if byte == b'\n' {
                return;
            }
            self.bump();
        }
    }

    fn skip_block_comment(&mut self, start: Position) {
        let _budget_frame = crate::budget::enter();
        loop {
            crate::budget::tick();
            match self.peek() {
                None => {
                    let span = self.span_from(start);
                    self.warning(span, "End of file reached while processing comment");
                    return;
                }
                Some(b'*') if self.peek_at(1) == Some(b'/') => {
                    self.bump();
                    self.bump();
                    return;
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        let _budget_frame = crate::budget::enter();
        self.source.as_bytes().get(self.offset).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        let _budget_frame = crate::budget::enter();
        self.source.as_bytes().get(self.offset + ahead).copied()
    }

    fn bump(&mut self) {
        let _budget_frame = crate::budget::enter();
        let Some(byte) = self.peek() else {
            return;
        };
        self.offset += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else if !is_utf8_continuation(byte) {
            self.column += 1;
        }
    }

    fn position(&self) -> Position {
        let _budget_frame = crate::budget::enter();
        Position::new(self.offset, self.line, self.column)
    }

    fn span_from(&self, start: Position) -> Span {
        let _budget_frame = crate::budget::enter();
        Span::new(start, self.position())
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        let _budget_frame = crate::budget::enter();
        self.diagnostics
            .push(Diagnostic::error(Stage::Lexer, span, message));
    }

    fn warning(&mut self, span: Span, message: impl Into<String>) {
        let _budget_frame = crate::budget::enter();
        self.diagnostics
            .push(Diagnostic::warning(Stage::Lexer, span, message));
    }
}

/// Returns true when a byte is part of the language rather than whitespace.
///
/// The reference compiler treats every byte outside `0x21..=0x7e` as
/// whitespace, which includes DEL and every high byte.
pub fn is_source_character(byte: u8) -> bool {
    let _budget_frame = crate::budget::enter();
    (0x21..=0x7e).contains(&byte)
}

fn is_utf8_continuation(byte: u8) -> bool {
    let _budget_frame = crate::budget::enter();
    (0x80..0xc0).contains(&byte)
}

/// Turns the UTF-8 the scanner walked back into the source file's own bytes.
///
/// The scanner reads a `&str` that [`crate::preprocessor::decode_source`]
/// produced, so a file byte of `0xe9` sits in it as the two UTF-8 bytes of
/// U+00E9. A string literal has to reach `CONSTS` as the `0xe9`, because the
/// length field counts bytes and the game reads Windows-1252.
fn source_bytes(utf8: &[u8]) -> Vec<u8> {
    let _budget_frame = crate::budget::enter();
    // The slice is always a whole run of characters: none of `"`, `\`, `\n`
    // or `\r` can appear inside a UTF-8 sequence, so the scanner never cuts
    // one in half. `from_utf8_lossy` therefore never replaces anything.
    crate::preprocessor::encode_source(&String::from_utf8_lossy(utf8))
}

fn hex_digit(byte: u8) -> Option<u8> {
    let _budget_frame = crate::budget::enter();
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Converts a decimal literal, wrapping on overflow rather than saturating.
///
/// Not what `atol` does. The reference compiler accumulates digit by digit and
/// lets the result wrap, so `2147483648` is `-2147483648`, `4294967296` is `0`,
/// and `99999999999` is `1215752191`. Saturating to `i32::MAX` instead would
/// compile all three to the same wrong number, silently. Measured against the
/// real compiler; the hex path already worked this way.
fn parse_decimal(text: &str) -> i32 {
    let _budget_frame = crate::budget::enter();
    let mut value: i32 = 0;
    for byte in text.bytes() {
        crate::budget::tick();
        if byte.is_ascii_digit() {
            value = value.wrapping_mul(10).wrapping_add((byte - b'0') as i32);
        }
    }
    value
}

/// Scans `source`, keeping warnings as well as errors.
pub fn scan(source: &str, engine: &EngineStructures) -> Lexed {
    let _budget_frame = crate::budget::enter();
    Lexer::with_engine_structures(source, engine.clone()).scan()
}

/// Scans `source` in one call.
///
/// # Errors
///
/// Returns every lexing error found, not just the first.
pub fn tokenize(source: &str) -> Result<Vec<Token>, CompileError> {
    let _budget_frame = crate::budget::enter();
    Lexer::new(source).tokenize()
}
