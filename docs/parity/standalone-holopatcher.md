# Standalone HoloPatcher and Python bootstrap compatibility

The pinned C# `ModBuildConverter` declares `holopatcher --args/-a` near line 697
and implements `RunHolopatcherAsync` near line 4329. It discovers native or
Python HoloPatcher, runs it, prints captured streams, and returns the backend
exit status. This standalone command does not perform guide installation,
recovery, or mandatory install-log verification.

Rust exposes `patcher::standalone::run(&Options, &AtomicBool) -> Result<Output>`.
It reuses `resolve_invocation` and `Invocation::command`, including packaged
backend integrity revalidation and the existing Python isolated bootstrap.
`--allow-processes` is mandatory. Native argv and legacy argument text are
mutually exclusive; Unix legacy text uses POSIX shell-style splitting (`shlex`),
with no shell expansion or root substitution. Guide Execute/Run Arguments use the
Windows rules on every host since 2026-10-04
([execute-arguments-contract.md](execute-arguments-contract.md)). Arguments are
limited to 256 items and 64 KiB. The optional working directory must exist as an
ordinary directory. The timeout defaults to one hour and accepts 1–86,400 seconds.

Linux and macOS use an owned process group. Exit observation uses
`waitid(WNOWAIT)` so the leader's identity remains reserved until the final group
signal, followed by reaping. Every post-spawn error has an RAII cleanup owner.
An unexpected external reaper (`ECHILD`) loses that identity and must not signal
a potentially reused numeric group. This is cooperative process-group cleanup,
not a sandbox: explicitly trusted programs can deliberately escape with setsid.
Windows now launches with `CREATE_SUSPENDED` and a restricted explicit handle
inheritance list. The suspended process is assigned to a private
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` Job Object before `ResumeThread`; assignment
failure terminates the still-suspended child. Parent jobs are never changed and
no breakaway flag is requested. Startup attribute storage, duplicated standard
handles, process/thread handles, and the job have retained cleanup owners.
Every launched outcome terminates remaining descendants and waits for job
`ActiveProcesses == 0` before reading final output or releasing the engine's
recovery lock. Query failures retain the job and retry. Cleanup can wait
indefinitely and cannot be abandoned by cancellation. The unassigned suspended
process is terminated and waited independently.

Windows preserves the known invocation constructors' inherited environment plus
explicit overrides/removals, using Unicode ordinal case-insensitive keys and
ordering. Those constructors never call `env_clear`; future constructors must
preserve that contract or add an explicit environment-mode representation.
The UTF-16 command line uses CRT argument quoting and is limited to 32,767 code
units including its terminator. Environment blocks are likewise bounded to
32,767 units; no shell or parent environment mutation is used. The legacy Windows
argument string uses the shared engine Windows tokenizer. Other unimplemented
targets refuse before launch. Windows behavior remains source-only and unverified
on a native host.

Stdout and stderr each have a polled one MiB file-size ceiling and a strict
bounded final read. Polling is not a filesystem quota. Non-UTF-8 output is decoded
lossily for reporting. Child stdin is closed. Signal exits are returned as
128 plus the signal. Cancellation, deadline, admission, and output-limit failures
return errors; callers map observed cancellation to their CLI cancellation code.
No backend output or arguments are logged by the library.

## `install-python-deps`

The C# verb (declaration near line 690, implementation near line 4145) manages
Python.Included/Python.NET: it initializes that runtime, checks imports, bootstraps
pip, and installs unpinned loggerplus and ply. Its GUI build target is commented
out at `ModSync.csproj:197–213` because of an MSBuild subprocess deadlock. The
manual verb remains callable in the reference.

Rust has no Python.Included environment. Its native package and externally
managed Python fallback are explicit existing contracts. The compatibility verb
must return an actionable unsupported error, never success for a no-op. It must
not silently install pip packages into the user's system or a guessed interpreter.
A future dependency installer would need an explicit owned environment and pinned
package contract. No package mutation is implemented by this slice.

Only production source and static review were performed. No backend, pip,
process-containment probe, native build, test, or CI was run for this slice.

The execution deadline is checked again when exit is observed, including after
a blocking Windows wait interval. It is a cooperative execution deadline, not a
hard wall-clock bound on OS cleanup: destructor reaping/Windows leader waits and
kernel failure liveness can outlast it. Cleanup never converts a timeout into a
successful backend result.

Unix shared cleanup now retains the unreaped leader until live group membership
is empty, using Linux task states or macOS libproc states. This reserves the
normal group identity and permits ignoring non-writing zombies. Membership
errors retain ownership and retry; no cleanup timeout releases the engine lock.
See `engine-owned-processes.md` for the precise cooperative containment boundary.
