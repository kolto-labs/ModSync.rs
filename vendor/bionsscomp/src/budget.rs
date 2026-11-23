//! Cooperative limits, active only inside the virtual API's private worker.
use crate::virtual_compile::{VirtualCompileOptions, VirtualLimits};
use std::cell::RefCell;
use std::panic::resume_unwind;
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct Stop(pub &'static str);
struct State {
    options: VirtualCompileOptions,
    work: u64,
    depth: usize,
    tokens: usize,
    diagnostics: usize,
    warnings: Vec<crate::Diagnostic>,
}
thread_local! { static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) }; }

pub(crate) fn install(options: VirtualCompileOptions) {
    ACTIVE.with(|state| {
        *state.borrow_mut() = Some(State {
            options,
            work: 0,
            depth: 0,
            tokens: 0,
            diagnostics: 0,
            warnings: Vec::new(),
        })
    });
}
fn stop(reason: &'static str) -> ! {
    resume_unwind(Box::new(Stop(reason)))
}
fn with_state(action: impl FnOnce(&mut State)) {
    ACTIVE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            action(state);
        }
    });
}
pub(crate) fn tick() {
    with_state(|state| {
        state.work += 1;
        if state.work > state.options.limits.max_work {
            stop("work limit exceeded");
        }
        if state.options.cancelled.load(Ordering::Relaxed) {
            stop("compilation cancelled");
        }
        if state
            .options
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            stop("compilation deadline exceeded");
        }
    });
}
pub(crate) struct Frame(bool);
impl Drop for Frame {
    fn drop(&mut self) {
        if self.0 {
            with_state(|state| state.depth -= 1);
        }
    }
}
pub(crate) fn enter() -> Frame {
    tick();
    let mut active = false;
    with_state(|state| {
        if state.depth >= state.options.limits.max_recursion {
            stop("compiler recursion limit exceeded");
        }
        state.depth += 1;
        active = true;
    });
    Frame(active)
}
pub(crate) fn check(value: usize, get: impl FnOnce(&VirtualLimits) -> usize, reason: &'static str) {
    tick();
    with_state(|state| {
        if value > get(&state.options.limits) {
            stop(reason);
        }
    });
}
pub(crate) fn token() {
    tick();
    with_state(|state| {
        state.tokens += 1;
        if state.tokens > state.options.limits.max_tokens {
            stop("token limit exceeded");
        }
    });
}
pub(crate) fn diagnostic() {
    tick();
    with_state(|state| {
        state.diagnostics += 1;
        if state.diagnostics > state.options.limits.max_diagnostics {
            stop("diagnostic limit exceeded");
        }
    });
}
/// Bound chains before constructing recursively-owned trees. Each syntax node
/// requires a token; four nodes/token conservatively covers wrapper nodes too.
pub(crate) fn syntax(tokens: &[crate::lexer::Token]) {
    use crate::lexer::{Punct, TokenKind};
    check(
        tokens.len().saturating_mul(4),
        |l| l.max_ast_nodes,
        "AST node upper bound exceeded",
    );
    let mut segment = 0;
    let mut nesting = 0usize;
    for token in tokens {
        tick();
        segment += 1;
        match token.kind {
            TokenKind::Punct(Punct::LParen | Punct::LBrace | Punct::LBracket) => {
                nesting += 1;
                check(
                    nesting,
                    |l| l.max_syntax_depth,
                    "syntax nesting limit exceeded",
                );
            }
            TokenKind::Punct(Punct::RParen | Punct::RBrace | Punct::RBracket) => {
                nesting = nesting.saturating_sub(1);
            }
            _ => {}
        }
        if matches!(
            token.kind,
            TokenKind::Punct(Punct::Semicolon | Punct::LBrace | Punct::RBrace)
        ) {
            segment = 0;
        }
        check(
            segment,
            |l| l.max_expression_tokens,
            "expression chain limit exceeded",
        );
    }
}

// Retain warnings even if a later stage hits an error or cooperative stop.
pub(crate) fn remember_warning(diagnostic: &crate::Diagnostic) {
    with_state(|state| state.warnings.push(diagnostic.clone()));
}
pub(crate) fn localize_warning(before: &crate::Diagnostic, after: &crate::Diagnostic) {
    if before.is_error() {
        return;
    }
    with_state(|state| {
        if let Some(stored) = state
            .warnings
            .iter_mut()
            .rev()
            .find(|stored| **stored == *before)
        {
            *stored = after.clone();
        }
    });
}
pub(crate) fn take_warnings() -> Vec<crate::Diagnostic> {
    let mut warnings = Vec::new();
    with_state(|state| warnings = std::mem::take(&mut state.warnings));
    warnings
}
