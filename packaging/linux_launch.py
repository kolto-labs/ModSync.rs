#!/usr/bin/env python3
"""One entry point for ordinary desktop activation and a single protocol URL."""
import os
from pathlib import Path
import sys

if len(sys.argv) > 2:
    raise SystemExit("ModSync launcher accepts at most one URL")
program = Path(__file__).resolve().parent / "modsync"
arguments = [str(program), "gui"]
if len(sys.argv) == 2:
    arguments += ["--protocol", sys.argv[1]]
os.execv(program, arguments)
