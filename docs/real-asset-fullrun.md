# Real-asset filesystem parity

Run on a trusted Linux machine with legally obtained game/mod baselines, Rust
1.95.0, Python 3.10+, the native build dependencies, and the supplied-installer
GUI driver. Use a dedicated, unprivileged account and keep the original
baselines read-only for it. `python3 tools/fullrun.py --help` describes the
driver, Xvfb, and Wine contract. The optional HoloPatcher lane runs only when
explicitly configured.

Set these in the local process environment, not in GitHub dispatch inputs:

| Variable | Value |
| --- | --- |
| `MODSYNC_FULLRUN_ROOT` | Absolute path to a runner-owned directory with mode 0700, outside the checkout, on a volume that persists after the job |
| `MODSYNC_FULLRUN_GUIDE` | Absolute path to the reviewed executable guide |
| `MODSYNC_FULLRUN_GAME` | Absolute path to the clean game baseline |
| `MODSYNC_FULLRUN_MODS` | Absolute path to the complete mod baseline |
| `MODSYNC_FULLRUN_DRIVER` | Supplied-installer GUI driver JSON, including its command and the Xvfb path |
| `MODSYNC_FULLRUN_SELECTION` | Optional JSON array of selection arguments; defaults to `["--use-file-selection"]` |
| `MODSYNC_FULLRUN_HOLOPATCHER` | Optional absolute path to the HoloPatcher executable |

To run directly from a trusted checkout: `bash tools/run_fullrun.sh`. The
command returns nonzero at the first failed stage and keeps the logs, traces,
and disposable roots in a new `run.*` directory under the configured root.
Inspect `fullrun.log` there. Size the volume for all lane copies and inventory
snapshots; set a retention policy for the run directories independently of
GitHub.

To dispatch **Full real-asset filesystem parity** from Actions, use the default
branch and configure the `modsync-real-assets` GitHub environment with a
default-branch restriction and required approval. Restrict the
`modsync-real-assets` runner group to this workflow file on that branch
(`.github/workflows/fullrun-parity.yml`); labels alone do not isolate a runner. Use an
isolated runner with the `modsync-fullrun` label and no access by PR or other
jobs. If workflow-level runner group isolation is unavailable, run the script
directly instead; do not register the asset machine as a general-purpose
runner. Actions shows only the outcome or the stage that failed. No artifacts
are uploaded.
