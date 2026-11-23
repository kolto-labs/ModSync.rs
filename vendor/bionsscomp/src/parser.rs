//! Turns tokens into a [`Script`].
//!
//! # Where the grammar came from
//!
//! Recursive descent over Torlack's yacc grammar (`_NscLib/NscParser.ypp` in
//! the BSD-licensed NWNTools sources), which is the direct ancestor of the
//! KOTOR compiler. Precedence, associativity and every statement production
//! follow it. Where the KOTOR build's observed behaviour differs, the KOTOR
//! build wins and the difference is marked `KOTOR-DIVERGENCE`.
//!
//! # Productions worth knowing about
//!
//! These are the ones that surprise people who assume C:
//!
//! - **`for` takes expressions, not declarations.** `for (int i = 0; ...)` is a
//!   syntax error. There is no production for it.
//! - **`case` and `default` are statements**, not parts of a `switch` node. A
//!   `switch` takes one statement, and the labels live inside it wherever a
//!   statement may go — including, legally, outside any switch, which sema
//!   then rejects.
//! - **`[1.0, 2.0, 3.0]` is a vector constant** and the components must be
//!   float literals. `[]`, `[1.0]` and `[1.0, 2.0]` are all valid and pad with
//!   zero. There is no vector expression syntax over variables.
//! - **A call must be spelled `identifier(...)`.** `s.fn()` does not parse.
//! - **A parameter list may end with a comma**: `void f(int a,)` is accepted.
//! - **The left side of an assignment must be a unary expression.** `-a = b`
//!   parses; `a + b = c` is a syntax error, not a sema error.
//! - **Unary `+` exists.** BioWare's compiler has no such operator.
//! - **`const` may be written anywhere a type may.** The parser records it and
//!   sema rejects the places it is not allowed, because that is where the
//!   reference compiler rejects it.
//!
//! # About the reference compiler's two passes
//!
//! `NscCompileScript` parses each file twice: once to collect declarations,
//! once to generate code, and it stops after the first pass if that pass
//! reported an error. So no syntax error is ever reported twice, and the only
//! externally visible effect of the split is which warnings belong to which
//! pass. A tree-building parser needs one syntactic pass to get the same
//! result, and this one emits every diagnostic exactly once.
//!
//! What pass one really bought was the answer to "does this file define an
//! entry point?", which decides between compiling and reporting
//! `File is an include file, ignored`. [`Parsed::index`] carries that answer so
//! the back end can make the same decision at the same point.

use crate::ast::{
    AssignOp, BinaryOp, Block, Declaration, DeclarationIndex, EngineType, Expr, Field,
    FunctionDecl, Parameter, PostfixOp, Script, Stmt, StructDecl, Type, UnaryOp, VarDecl, MAIN,
    STARTING_CONDITIONAL,
};
use crate::error::{CompileError, Diagnostic, DiagnosticSink, Position, Severity, Span, Stage};
use crate::lexer::{Keyword, Punct, Token, TokenKind};

/// What one parse produced.
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    /// The tree.
    pub script: Script,
    /// What the file declared at file scope.
    pub index: DeclarationIndex,
    /// Every problem found. Parsing does not stop at the first one.
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
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

/// Builds a [`Script`] from a token stream.
#[derive(Debug, Clone)]
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
    sink: DiagnosticSink,
    aborted: bool,
}

/// Signals that the parser gave up on the construct it was reading.
///
/// The caller recovers; it does not stop the whole parse unless the error limit
/// was reached.
struct Failed;

type Parse<T> = Result<T, Failed>;

impl Parser {
    /// Builds a parser over `tokens`.
    pub fn new(tokens: Vec<Token>) -> Self {
        let _budget_frame = crate::budget::enter();
        Parser {
            tokens,
            position: 0,
            sink: DiagnosticSink::new(),
            aborted: false,
        }
    }

    /// Returns the tokens the parser was given.
    pub fn tokens(&self) -> &[Token] {
        let _budget_frame = crate::budget::enter();
        &self.tokens
    }

    /// Returns the index of the next token to read.
    pub fn position(&self) -> usize {
        let _budget_frame = crate::budget::enter();
        self.position
    }

    /// Returns the next token without consuming it.
    pub fn peek(&self) -> Option<&Token> {
        let _budget_frame = crate::budget::enter();
        self.tokens.get(self.position)
    }

    /// Parses the whole token stream, keeping warnings as well as errors.
    pub fn parse_detailed(&mut self) -> Parsed {
        let _budget_frame = crate::budget::enter();
        crate::budget::syntax(&self.tokens);
        let mut script = Script::default();

        while !self.at_eof() && !self.aborted {
            crate::budget::tick();
            let before = self.position;
            match self.external_declaration() {
                Ok(mut declarations) => script.declarations.append(&mut declarations),
                Err(Failed) => self.recover(),
            }
            if self.position == before {
                // Recovery made no progress. Drop a token so the loop ends.
                self.advance();
            }
        }

        let index = index_of(&script);
        Parsed {
            script,
            index,
            diagnostics: std::mem::take(&mut self.sink).into_diagnostics(),
        }
    }

    /// Parses the whole token stream.
    ///
    /// # Errors
    ///
    /// Returns every syntax error found, not just the first.
    pub fn parse(&mut self) -> Result<Script, CompileError> {
        let _budget_frame = crate::budget::enter();
        let parsed = self.parse_detailed();
        if parsed.has_errors() {
            return Err(CompileError::Diagnostics(parsed.diagnostics));
        }
        Ok(parsed.script)
    }

    // ---- file scope ------------------------------------------------------

    fn external_declaration(&mut self) -> Parse<Vec<Declaration>> {
        let _budget_frame = crate::budget::enter();
        let start = self.current_span();
        let is_const = self.eat_keyword(Keyword::Const);

        // `struct Name {` opens a type declaration. `struct Name x;` is a
        // variable of that type. Only the lookahead past the name separates
        // them, and `const struct Name { ... };` is not a production at all.
        if !is_const && self.at_keyword(Keyword::Struct) && self.looks_like_struct_definition() {
            return Ok(vec![self.struct_definition(start)?]);
        }

        let ty = self.type_specifier()?;
        let name = self.identifier()?;

        if self.at_punct(Punct::LParen) {
            return Ok(vec![self.function(ty, name, is_const, start)?]);
        }

        let declarations = self.init_declarator_list(ty, name, is_const, start)?;
        Ok(declarations.into_iter().map(Declaration::Global).collect())
    }

    fn looks_like_struct_definition(&self) -> bool {
        let _budget_frame = crate::budget::enter();
        matches!(
            self.kind_at(1),
            Some(TokenKind::Identifier(_)) | Some(TokenKind::EngineType { .. })
        ) && self.kind_at(2) == Some(&TokenKind::Punct(Punct::LBrace))
    }

    fn struct_definition(&mut self, start: Span) -> Parse<Declaration> {
        let _budget_frame = crate::budget::enter();
        self.advance(); // struct
        let name = self.identifier()?;
        self.expect(Punct::LBrace)?;

        let mut fields = Vec::new();
        while !self.at_punct(Punct::RBrace) && !self.at_eof() {
            crate::budget::tick();
            let field_start = self.current_span();
            let is_const = self.eat_keyword(Keyword::Const);
            let ty = self.type_specifier()?;

            loop {
                crate::budget::tick();
                let field_name = self.identifier()?;
                fields.push(Field {
                    ty: ty.clone(),
                    name: field_name,
                    is_const,
                    span: field_start.join(self.previous_span()),
                });
                if !self.eat_punct(Punct::Comma) {
                    break;
                }
            }
            self.expect(Punct::Semicolon)?;
        }

        self.expect(Punct::RBrace)?;
        self.expect(Punct::Semicolon)?;

        Ok(Declaration::Struct(StructDecl {
            name,
            fields,
            span: start.join(self.previous_span()),
        }))
    }

    fn function(
        &mut self,
        return_type: Type,
        name: String,
        is_const: bool,
        start: Span,
    ) -> Parse<Declaration> {
        let _budget_frame = crate::budget::enter();
        self.expect(Punct::LParen)?;
        let mut parameters = Vec::new();

        while !self.at_punct(Punct::RParen) && !self.at_eof() {
            crate::budget::tick();
            parameters.push(self.parameter()?);
            if !self.eat_punct(Punct::Comma) {
                break;
            }
            // A trailing comma before `)` is allowed by the grammar.
        }
        self.expect(Punct::RParen)?;

        let body = if self.at_punct(Punct::LBrace) {
            Some(self.block()?)
        } else {
            self.expect(Punct::Semicolon)?;
            None
        };

        Ok(Declaration::Function(FunctionDecl {
            return_type,
            name,
            parameters,
            body,
            is_const,
            span: start.join(self.previous_span()),
        }))
    }

    fn parameter(&mut self) -> Parse<Parameter> {
        let _budget_frame = crate::budget::enter();
        let start = self.current_span();
        let is_const = self.eat_keyword(Keyword::Const);
        let ty = self.type_specifier()?;
        let name = self.identifier()?;
        let default = if self.eat_punct(Punct::Assign) {
            Some(self.assignment_expression()?)
        } else {
            None
        };

        Ok(Parameter {
            ty,
            name,
            default,
            is_const,
            span: start.join(self.previous_span()),
        })
    }

    /// Parses `name [= init] {, name [= init]} ;` for an already-parsed type.
    fn init_declarator_list(
        &mut self,
        ty: Type,
        first: String,
        is_const: bool,
        start: Span,
    ) -> Parse<Vec<VarDecl>> {
        let _budget_frame = crate::budget::enter();
        let mut declarations = Vec::new();
        let mut name = first;

        loop {
            crate::budget::tick();
            let init = if self.eat_punct(Punct::Assign) {
                Some(self.assignment_expression()?)
            } else {
                None
            };
            declarations.push(VarDecl {
                ty: ty.clone(),
                name,
                init,
                is_const,
                span: start.join(self.previous_span()),
            });

            if !self.eat_punct(Punct::Comma) {
                break;
            }
            name = self.identifier()?;
        }

        self.expect(Punct::Semicolon)?;
        Ok(declarations)
    }

    // ---- types -----------------------------------------------------------

    fn type_specifier(&mut self) -> Parse<Type> {
        let _budget_frame = crate::budget::enter();
        let ty = match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Keyword(Keyword::Void)) => Type::Void,
            Some(TokenKind::Keyword(Keyword::Int)) => Type::Int,
            Some(TokenKind::Keyword(Keyword::Float)) => Type::Float,
            Some(TokenKind::Keyword(Keyword::Object)) => Type::Object,
            Some(TokenKind::Keyword(Keyword::Str)) => Type::Str,
            Some(TokenKind::Keyword(Keyword::Action)) => Type::Action,
            Some(TokenKind::Keyword(Keyword::Vector)) => Type::Vector,
            Some(TokenKind::EngineType { index, name }) => {
                Type::Engine(EngineType::new(*index, name.clone()))
            }
            Some(TokenKind::Keyword(Keyword::Struct)) => {
                self.advance();
                let name = self.identifier()?;
                return Ok(Type::Struct(name));
            }
            _ => return self.syntax_error(),
        };
        self.advance();
        Ok(ty)
    }

    fn starts_type(&self) -> bool {
        let _budget_frame = crate::budget::enter();
        match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Keyword(keyword)) => {
                keyword.starts_type() || *keyword == Keyword::Const
            }
            Some(TokenKind::EngineType { .. }) => true,
            _ => false,
        }
    }

    // ---- statements ------------------------------------------------------

    fn block(&mut self) -> Parse<Block> {
        let _budget_frame = crate::budget::enter();
        let start = self.current_span();
        self.expect(Punct::LBrace)?;

        let mut statements = Vec::new();
        while !self.at_punct(Punct::RBrace) && !self.at_eof() && !self.aborted {
            crate::budget::tick();
            let before = self.position;
            match self.statement() {
                Ok(statement) => statements.push(statement),
                Err(Failed) => self.recover(),
            }
            if self.position == before {
                self.advance();
            }
        }
        self.expect(Punct::RBrace)?;

        Ok(Block {
            statements,
            span: start.join(self.previous_span()),
        })
    }

    fn statement(&mut self) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        let start = self.current_span();

        if self.eat_punct(Punct::Semicolon) {
            return Ok(Stmt::Empty(start));
        }
        if self.at_punct(Punct::LBrace) {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.starts_type() {
            return self.declaration_statement(start);
        }

        let Some(TokenKind::Keyword(keyword)) = self.peek().map(|token| &token.kind).cloned()
        else {
            return self.expression_statement(start);
        };

        match keyword {
            Keyword::If => self.if_statement(start),
            Keyword::Switch => self.switch_statement(start),
            Keyword::While => self.while_statement(start),
            Keyword::Do => self.do_statement(start),
            Keyword::For => self.for_statement(start),
            Keyword::Case => {
                self.advance();
                let label = self.conditional_expression()?;
                self.expect(Punct::Colon)?;
                Ok(Stmt::Case {
                    label: Some(label),
                    span: start.join(self.previous_span()),
                })
            }
            Keyword::Default => {
                self.advance();
                self.expect(Punct::Colon)?;
                Ok(Stmt::Case {
                    label: None,
                    span: start.join(self.previous_span()),
                })
            }
            Keyword::Break => {
                self.advance();
                self.expect(Punct::Semicolon)?;
                Ok(Stmt::Break(start.join(self.previous_span())))
            }
            Keyword::Continue => {
                self.advance();
                self.expect(Punct::Semicolon)?;
                Ok(Stmt::Continue(start.join(self.previous_span())))
            }
            Keyword::Return => {
                self.advance();
                let value = if self.at_punct(Punct::Semicolon) {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(Punct::Semicolon)?;
                Ok(Stmt::Return {
                    value,
                    span: start.join(self.previous_span()),
                })
            }
            _ => self.expression_statement(start),
        }
    }

    fn declaration_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        let is_const = self.eat_keyword(Keyword::Const);
        let ty = self.type_specifier()?;
        let name = self.identifier()?;
        let mut declarations = self.init_declarator_list(ty, name, is_const, start)?;

        // A declarator list is several declarations, and they belong to the
        // scope around them. Wrapping them in a block would give them one of
        // their own, and the names would go out of scope on the next line.
        if declarations.len() == 1 {
            return Ok(Stmt::VarDecl(declarations.remove(0)));
        }
        Ok(Stmt::VarDecls {
            declarations,
            span: start.join(self.previous_span()),
        })
    }

    fn expression_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        let expr = self.expression()?;
        self.expect(Punct::Semicolon)?;
        Ok(Stmt::Expr {
            expr,
            span: start.join(self.previous_span()),
        })
    }

    fn if_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        self.expect(Punct::LParen)?;
        let condition = self.expression()?;
        self.expect(Punct::RParen)?;

        let then_branch = self.branch_statement()?;
        let else_branch = if self.eat_keyword(Keyword::Else) {
            Some(Box::new(self.branch_statement()?))
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_branch: Box::new(then_branch),
            else_branch,
            span: start.join(self.previous_span()),
        })
    }

    /// Parses the body of an `if` or an `else`, warning about `if (x);`.
    ///
    /// The grammar gives these arms their own production purely so it can raise
    /// this warning. Loop bodies use the plain statement production and stay
    /// silent.
    fn branch_statement(&mut self) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        let span = self.current_span();
        let statement = self.statement()?;
        if matches!(statement, Stmt::Empty(_)) {
            self.warning(
                span,
                "\"if\" or \"else\" statement followed by a blank statement. (i.e. if (x);)",
            );
        }
        Ok(statement)
    }

    fn switch_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        self.expect(Punct::LParen)?;
        let subject = self.expression()?;
        self.expect(Punct::RParen)?;
        let body = self.statement()?;

        Ok(Stmt::Switch {
            subject,
            body: Box::new(body),
            span: start.join(self.previous_span()),
        })
    }

    fn while_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        self.expect(Punct::LParen)?;
        let condition = self.expression()?;
        self.expect(Punct::RParen)?;
        let body = self.statement()?;

        Ok(Stmt::While {
            condition,
            body: Box::new(body),
            span: start.join(self.previous_span()),
        })
    }

    fn do_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        let body = self.statement()?;
        self.expect_keyword(Keyword::While)?;
        self.expect(Punct::LParen)?;
        let condition = self.expression()?;
        self.expect(Punct::RParen)?;
        self.expect(Punct::Semicolon)?;

        Ok(Stmt::DoWhile {
            body: Box::new(body),
            condition,
            span: start.join(self.previous_span()),
        })
    }

    fn for_statement(&mut self, start: Span) -> Parse<Stmt> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        self.expect(Punct::LParen)?;

        let init = self.optional_expression(Punct::Semicolon)?;
        self.expect(Punct::Semicolon)?;
        let condition = self.optional_expression(Punct::Semicolon)?;
        self.expect(Punct::Semicolon)?;
        let step = self.optional_expression(Punct::RParen)?;
        self.expect(Punct::RParen)?;

        let body = self.statement()?;
        Ok(Stmt::For {
            init,
            condition,
            step,
            body: Box::new(body),
            span: start.join(self.previous_span()),
        })
    }

    fn optional_expression(&mut self, terminator: Punct) -> Parse<Option<Expr>> {
        let _budget_frame = crate::budget::enter();
        if self.at_punct(terminator) {
            return Ok(None);
        }
        self.expression().map(Some)
    }

    // ---- expressions -----------------------------------------------------

    fn expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        self.assignment_expression()
    }

    fn assignment_expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let left = self.conditional_expression()?;
        let Some(op) = self.peek_assign_op() else {
            return Ok(left);
        };

        // The grammar's left side is a unary_expression, so `a + b = c` is a
        // syntax error rather than something sema complains about.
        if !is_unary_expression(&left) {
            return self.syntax_error();
        }

        let span = left.span();
        self.advance();
        let value = self.assignment_expression()?;
        Ok(Expr::Assign {
            op,
            target: Box::new(left),
            value: Box::new(value),
            span: span.join(self.previous_span()),
        })
    }

    fn conditional_expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let condition = self.binary_expression(0)?;
        if !self.eat_punct(Punct::Question) {
            return Ok(condition);
        }

        let then_value = self.expression()?;
        self.expect(Punct::Colon)?;
        let else_value = self.conditional_expression()?;
        let span = condition.span().join(else_value.span());

        Ok(Expr::Conditional {
            condition: Box::new(condition),
            then_value: Box::new(then_value),
            else_value: Box::new(else_value),
            span,
        })
    }

    /// Precedence climbing preserves left associativity without descending
    /// through every unused precedence level for each parenthesized expression.
    /// Only an actual higher-precedence right operand adds a recursive frame.
    fn binary_expression(&mut self, minimum_level: usize) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let mut left = self.unary_expression()?;
        loop {
            crate::budget::tick();
            let operator = BINARY_LEVELS
                .iter()
                .enumerate()
                .skip(minimum_level)
                .find_map(|(level, operators)| {
                    operators
                        .iter()
                        .find(|(punct, _)| self.at_punct(*punct))
                        .map(|&(_, op)| (level, op))
                });
            let Some((level, op)) = operator else {
                return Ok(left);
            };
            self.advance();
            let right = self.binary_expression(level + 1)?;
            let span = left.span().join(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
    }

    fn unary_expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let start = self.current_span();
        let op = match self.peek().map(|token| &token.kind) {
            Some(TokenKind::Punct(Punct::Increment)) => UnaryOp::PreIncrement,
            Some(TokenKind::Punct(Punct::Decrement)) => UnaryOp::PreDecrement,
            Some(TokenKind::Punct(Punct::Plus)) => UnaryOp::Plus,
            Some(TokenKind::Punct(Punct::Minus)) => UnaryOp::Negate,
            Some(TokenKind::Punct(Punct::Tilde)) => UnaryOp::BitNot,
            Some(TokenKind::Punct(Punct::Not)) => UnaryOp::Not,
            _ => return self.postfix_expression(),
        };

        self.advance();
        let operand = self.unary_expression()?;
        Ok(Expr::Unary {
            op,
            span: start.join(operand.span()),
            operand: Box::new(operand),
        })
    }

    fn postfix_expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let mut expr = self.primary_expression()?;

        loop {
            crate::budget::tick();
            if self.eat_punct(Punct::Dot) {
                let field = self.identifier()?;
                let span = expr.span().join(self.previous_span());
                expr = Expr::Member {
                    value: Box::new(expr),
                    field,
                    span,
                };
                continue;
            }

            let op = if self.at_punct(Punct::Increment) {
                PostfixOp::Increment
            } else if self.at_punct(Punct::Decrement) {
                PostfixOp::Decrement
            } else {
                return Ok(expr);
            };
            self.advance();
            let span = expr.span().join(self.previous_span());
            expr = Expr::Postfix {
                op,
                operand: Box::new(expr),
                span,
            };
        }
    }

    fn primary_expression(&mut self) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        let span = self.current_span();
        let Some(kind) = self.peek().map(|token| token.kind.clone()) else {
            return self.syntax_error();
        };

        match kind {
            TokenKind::Int(value) => {
                self.advance();
                Ok(Expr::IntLit { value, span })
            }
            TokenKind::Float(value) => {
                self.advance();
                Ok(Expr::FloatLit { value, span })
            }
            TokenKind::Str(value) => {
                self.advance();
                Ok(Expr::StrLit { value, span })
            }
            TokenKind::Keyword(Keyword::ObjectSelf) => {
                self.advance();
                Ok(Expr::ObjectLit { value: 0, span })
            }
            TokenKind::Keyword(Keyword::ObjectInvalid) => {
                self.advance();
                Ok(Expr::ObjectLit { value: 1, span })
            }
            TokenKind::Punct(Punct::LParen) => {
                self.advance();
                let inner = self.expression()?;
                self.expect(Punct::RParen)?;
                Ok(inner)
            }
            TokenKind::Punct(Punct::LBracket) => self.vector_constant(span),
            TokenKind::Identifier(name) => {
                self.advance();
                if !self.at_punct(Punct::LParen) {
                    return Ok(Expr::Name { name, span });
                }
                self.advance();

                let mut arguments = Vec::new();
                while !self.at_punct(Punct::RParen) && !self.at_eof() {
                    crate::budget::tick();
                    arguments.push(self.assignment_expression()?);
                    if !self.eat_punct(Punct::Comma) {
                        break;
                    }
                }
                self.expect(Punct::RParen)?;

                let call_span = span.join(self.previous_span());
                Ok(Expr::Call {
                    callee: Box::new(Expr::Name { name, span }),
                    arguments,
                    span: call_span,
                })
            }
            _ => self.syntax_error(),
        }
    }

    /// Parses `[ ]`, `[ f ]`, `[ f, f ]` or `[ f, f, f ]`.
    ///
    /// Components must be float literals. Missing ones are zero.
    fn vector_constant(&mut self, span: Span) -> Parse<Expr> {
        let _budget_frame = crate::budget::enter();
        self.advance();
        let mut components = [0.0f32; 3];

        for (slot, component) in components.iter_mut().enumerate() {
            crate::budget::tick();
            if self.at_punct(Punct::RBracket) {
                break;
            }
            if slot > 0 && !self.eat_punct(Punct::Comma) {
                break;
            }
            let Some(TokenKind::Float(value)) = self.peek().map(|token| &token.kind) else {
                return self.syntax_error();
            };
            *component = *value;
            self.advance();
        }
        self.expect(Punct::RBracket)?;

        Ok(Expr::VectorLit {
            x: components[0],
            y: components[1],
            z: components[2],
            span: span.join(self.previous_span()),
        })
    }

    fn peek_assign_op(&self) -> Option<AssignOp> {
        let _budget_frame = crate::budget::enter();
        let TokenKind::Punct(punct) = self.peek()?.kind else {
            return None;
        };
        Some(match punct {
            Punct::Assign => AssignOp::Assign,
            Punct::PlusAssign => AssignOp::Add,
            Punct::MinusAssign => AssignOp::Sub,
            Punct::StarAssign => AssignOp::Mul,
            Punct::SlashAssign => AssignOp::Div,
            Punct::PercentAssign => AssignOp::Mod,
            Punct::AmpAssign => AssignOp::BitAnd,
            Punct::PipeAssign => AssignOp::BitOr,
            Punct::CaretAssign => AssignOp::BitXor,
            Punct::ShlAssign => AssignOp::Shl,
            Punct::ShrAssign => AssignOp::Shr,
            Punct::UShrAssign => AssignOp::UShr,
            _ => return None,
        })
    }

    // ---- token plumbing --------------------------------------------------

    fn kind_at(&self, ahead: usize) -> Option<&TokenKind> {
        let _budget_frame = crate::budget::enter();
        self.tokens.get(self.position + ahead).map(|t| &t.kind)
    }

    fn at_eof(&self) -> bool {
        let _budget_frame = crate::budget::enter();
        match self.peek() {
            None => true,
            Some(token) => token.is_eof(),
        }
    }

    fn at_punct(&self, punct: Punct) -> bool {
        let _budget_frame = crate::budget::enter();
        self.peek().is_some_and(|token| token.is_punct(punct))
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        let _budget_frame = crate::budget::enter();
        self.peek().is_some_and(|token| token.is_keyword(keyword))
    }

    fn advance(&mut self) {
        let _budget_frame = crate::budget::enter();
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn eat_punct(&mut self, punct: Punct) -> bool {
        let _budget_frame = crate::budget::enter();
        let found = self.at_punct(punct);
        if found {
            self.advance();
        }
        found
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        let _budget_frame = crate::budget::enter();
        let found = self.at_keyword(keyword);
        if found {
            self.advance();
        }
        found
    }

    fn expect(&mut self, punct: Punct) -> Parse<()> {
        let _budget_frame = crate::budget::enter();
        if self.eat_punct(punct) {
            return Ok(());
        }
        self.syntax_error()
    }

    fn expect_keyword(&mut self, keyword: Keyword) -> Parse<()> {
        let _budget_frame = crate::budget::enter();
        if self.eat_keyword(keyword) {
            return Ok(());
        }
        self.syntax_error()
    }

    fn identifier(&mut self) -> Parse<String> {
        let _budget_frame = crate::budget::enter();
        // An engine structure name is an identifier that a `#define` claimed.
        // Using one as a variable name is a name clash, not a syntax error, so
        // it is let through here and rejected by sema.
        match self.peek().map(|token| token.kind.clone()) {
            Some(TokenKind::Identifier(name)) | Some(TokenKind::EngineType { name, .. }) => {
                self.advance();
                Ok(name)
            }
            _ => self.syntax_error(),
        }
    }

    fn current_span(&self) -> Span {
        let _budget_frame = crate::budget::enter();
        self.peek()
            .map(|token| token.span)
            .unwrap_or_else(|| self.previous_span())
    }

    fn previous_span(&self) -> Span {
        let _budget_frame = crate::budget::enter();
        self.position
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map(|token| token.span)
            .unwrap_or_else(|| Span::point(Position::START))
    }

    /// Reports `Syntax error at "%s"` for the current token.
    ///
    /// Once the error limit is hit the reference compiler aborts the parse
    /// outright, so nothing is reported after that.
    fn syntax_error<T>(&mut self) -> Parse<T> {
        let _budget_frame = crate::budget::enter();
        if self.aborted {
            return Err(Failed);
        }

        let (span, message) = match self.peek() {
            None => (self.previous_span(), "Unexpected end of file".to_string()),
            Some(token) if token.is_eof() => (token.span, "Unexpected end of file".to_string()),
            Some(token) => (
                token.span,
                format!("Syntax error at \"{}\"", token.kind.describe()),
            ),
        };

        self.sink
            .push(Diagnostic::error(Stage::Parser, span, message));

        if self.sink.is_exhausted() {
            self.sink.push(Diagnostic::error(
                Stage::Parser,
                span,
                "Compiler has reached the limit of 100 errors, aborting",
            ));
            self.aborted = true;
        }
        Err(Failed)
    }

    fn warning(&mut self, span: Span, message: impl Into<String>) {
        let _budget_frame = crate::budget::enter();
        self.sink
            .push(Diagnostic::warning(Stage::Parser, span, message));
    }

    /// Skips to the next place a statement could start again.
    ///
    /// The yacc grammar's error rule scans forward to end of file, `;`, `{` or
    /// `}`, then drops a `;` if that is where it stopped.
    fn recover(&mut self) {
        let _budget_frame = crate::budget::enter();
        while !self.at_eof()
            && !self.at_punct(Punct::Semicolon)
            && !self.at_punct(Punct::LBrace)
            && !self.at_punct(Punct::RBrace)
        {
            crate::budget::tick();
            self.advance();
        }
        self.eat_punct(Punct::Semicolon);
    }
}

/// The left-associative binary levels, loosest first.
///
/// Straight from the grammar's expression cascade:
/// `||`, `&&`, `|`, `^`, `&`, equality, relational, shift, additive,
/// multiplicative.
type BinaryLevel = &'static [(Punct, BinaryOp)];

const BINARY_LEVELS: &[BinaryLevel] = &[
    &[(Punct::OrOr, BinaryOp::LogicalOr)],
    &[(Punct::AndAnd, BinaryOp::LogicalAnd)],
    &[(Punct::Pipe, BinaryOp::BitOr)],
    &[(Punct::Caret, BinaryOp::BitXor)],
    &[(Punct::Amp, BinaryOp::BitAnd)],
    &[(Punct::Eq, BinaryOp::Eq), (Punct::NotEq, BinaryOp::NotEq)],
    &[
        (Punct::Less, BinaryOp::Less),
        (Punct::Greater, BinaryOp::Greater),
        (Punct::LessEq, BinaryOp::LessEq),
        (Punct::GreaterEq, BinaryOp::GreaterEq),
    ],
    &[
        (Punct::Shl, BinaryOp::Shl),
        (Punct::Shr, BinaryOp::Shr),
        (Punct::UShr, BinaryOp::UShr),
    ],
    &[(Punct::Plus, BinaryOp::Add), (Punct::Minus, BinaryOp::Sub)],
    &[
        (Punct::Star, BinaryOp::Mul),
        (Punct::Slash, BinaryOp::Div),
        (Punct::Percent, BinaryOp::Mod),
    ],
];

/// Returns true when `expr` could have been produced by `unary_expression`.
fn is_unary_expression(expr: &Expr) -> bool {
    let _budget_frame = crate::budget::enter();
    !matches!(
        expr,
        Expr::Binary { .. } | Expr::Assign { .. } | Expr::Conditional { .. }
    )
}

/// Collects what the file declared at file scope.
fn index_of(script: &Script) -> DeclarationIndex {
    let _budget_frame = crate::budget::enter();
    let mut index = DeclarationIndex::default();

    for declaration in &script.declarations {
        crate::budget::tick();
        match declaration {
            Declaration::Struct(declaration) => index.struct_names.push(declaration.name.clone()),
            Declaration::Global(declaration) => index.global_names.push(declaration.name.clone()),
            Declaration::Function(declaration) => {
                if !index.function_names.contains(&declaration.name) {
                    index.function_names.push(declaration.name.clone());
                }
                if declaration.body.is_some() {
                    if declaration.name == MAIN {
                        index.has_main = true;
                    } else if declaration.name == STARTING_CONDITIONAL {
                        index.has_starting_conditional = true;
                    }
                }
            }
        }
    }

    index
}

/// Parses a token stream in one call, keeping warnings as well as errors.
pub fn parse_detailed(tokens: Vec<Token>) -> Parsed {
    let _budget_frame = crate::budget::enter();
    Parser::new(tokens).parse_detailed()
}

/// Parses a token stream in one call.
///
/// # Errors
///
/// Returns every syntax error found, not just the first.
pub fn parse(tokens: Vec<Token>) -> Result<Script, CompileError> {
    let _budget_frame = crate::budget::enter();
    Parser::new(tokens).parse()
}
