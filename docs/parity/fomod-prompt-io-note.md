# Cancellation-aware Fetch terminal input

Read-only feasibility audit by an independent subagent; implementation guidance for
approved FOMOD S1, not implementation or platform acceptance. No builds, tests,
Rust harnesses, network calls or mutations were performed by the auditor.

Existing `prompt_choices` blocks on `BufRead::read_line`; the signal handler only
sets an AtomicBool. Local Tokio1.53.1 `io/stdin.rs:16-20` explicitly documents an
uncancellable blocking worker and possible runtime shutdown wait until Enter.
Neither checking the flag between lines nor selecting against Tokio stdin meets
the approved cancellation contract.

Use one internal prompt core with a typed next-line callback: Line, EOF, signal
cancellation, or I/O error. Keep the candidate clone until validation completes.
The existing standalone wrapper keeps its signature and supplies its old BufRead
behavior, including existing EOF/q error mapping. Fetch supplies cancellable input
for both configure confirmation and choices, represents refusal separately, and
checks cancellation again immediately before committing a completed session.

## Unix candidate

Target-specific direct libc0.2.189 is already present in Cargo.lock. Local bindings
supply poll/read/fcntl/pollfd. Fetch must exclusively consume stdin for the prompt
sequence; do not mix buffered std stdin and raw reads. Save flags, add O_NONBLOCK,
and restore exact original flags on every exit. dup alone shares status flags and
does not isolate this change.

Maintain a byte buffer across prompts. Check cancellation before consuming a
complete buffered line, before a bounded poll (for example50ms), and after reads.
Retry EINTR; send EAGAIN back through poll. Leave canonical terminal editing/echo
and signal processing unchanged. Accumulate partial pipe/file bytes until newline;
drain pending bytes on hangup, return a final unterminated nonempty line once,
then EOF. Validate UTF-8 only after assembling a line. Signal cancellation discards
partial input and uncommitted selection; no reader thread is needed.

## Windows candidate — runtime proof still required

Target-specific direct windows-sys0.61.2 is already locked. Minimal feature groups:
Win32_Foundation, Win32_System_Console, Win32_System_IO,
Win32_Storage_FileSystem. GetConsoleMode selects ReadConsoleW for a cooked console
or ReadFile for redirected stdin. Preserve console mode; convert completed UTF-16
input and retain partial-line semantics for pipes/files.

An owned synchronous native-read worker is supervised while Fetch observes its
signal flag. Once cancelled, repeatedly call CancelSynchronousIo on the real
worker thread handle until completion, then join before returning. A single call
can miss a read submitted immediately afterward (ERROR_NOT_FOUND), so it is not
sufficient. Check cancellation before each read and after it returns; cancellation
wins over a concurrent complete line. Do not use Rust's buffered stdin inside this
boundary, drop an unfinished JoinHandle, terminate a thread, or report completion
while a read worker remains alive.

Confidence is medium until supported Windows console and ordinary redirected-pipe
routes are exercised. Local bindings prove availability, not cancellation support
for every device driver. Unexpected cancellation errors remain visible; detached
readers are not a fallback. Cross-compilation alone cannot establish this behavior.

## Ownership and proof

S1 may need narrowly scoped Cargo target-dependency declarations plus the adapter
module; reusing locked versions avoids an unrelated dependency upgrade. Record
this exact ownership before implementation. Existing frozen Rust test sources
remain untouched; the freeze does not justify an unsafe detached-reader design.

Production receipts must send SIGINT while awaiting confirmation and selection,
with no next line; repeat with partial TTY input and a live pipe writer that keeps
the stream open without newline. Cover canonical editing/EOF, pipe EOF with/without
residual bytes, original flag restoration, no uncommitted configuration and no
surviving reader. The approved plan's source SPEC and QUALITY gates still apply.
