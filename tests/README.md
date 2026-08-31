# dfm integration tests

These are **shell integration tests** for the `dfm` binary. Each file in this
directory named `test*.sh` is one test case; the harness discovers them
automatically, so adding a new `test_*.sh` file is enough to run it. There are
currently **220** test files (the launcher prints the exact count it found).

## Requirements

- **bash**: the launcher is a bash script (`#!/usr/bin/bash`) and relies on
  bashisms (`[[ ]]`, `local`, arrays, `export -f`, `foo=$(…)`), so it must
  be run with **bash**, not `sh`/`dash`. Run it as `./launcher.sh` or
  `bash tests/launcher.sh`, not `sh tests/launcher.sh`.
- **`/tmp` write access**: the harness creates its scratch space with
  `mktemp -d` in `/tmp` (noted in the launcher as mounted to a memory
  filesystem) and places every test's fresh `$HOME` under it. `/tmp` must be
  writable and must have enough free space for the suite's temporary files; the
  whole scratch tree is removed on exit.
- **`uuid`** (optional): if the `uuid` command is not installed, the launcher
  installs a stub that reads `/proc/sys/kernel/random/uuid`, so tests still
  work.

## Prerequisites

The project must be built before running the tests:

```shell
cargo build
```

The launcher uses `target/debug/dfm` (preferred) or `target/release/dfm`
(fallback). If neither exists it fails with `project is not built`.

## Running the tests

Run the whole suite with debug output for failed tests:

```shell
./launcher.sh
```

Run the whole suite quietly (no `-x` trace, failure details only):

```shell
./launcher.sh -q
```

Run a single test (full `-x` trace of that test):

```shell
./launcher.sh test_add_unchanged_file.sh
```

Run a single test quietly:

```shell
./launcher.sh -q test_add_unchanged_file.sh
```

The `-q` flag may come before the test file; everything after `--` is treated
as a path. The launcher prints one `---- <name> ✅/❌` line per test and a final
`succeeded N` / `failed N` summary, and exits non-zero if any test failed.

## How the harness runs a test (launcher.sh)

Each test is `source`d in a fresh **subshell** with:

- `set -eEu` (and `-x` unless `-q`), so any bare failing command aborts the
  test — a failing test is detected without needing explicit checks on every
  line.
- `< /dev/null` for stdin, so no test can hang waiting for prompt input.
- A brand-new temporary `$HOME` plus all four XDG vars exported into it
  (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`), so
  `dfm` writes only into the sandbox and can never touch your real home
  directory.
- The test runs with `cwd` set to the fresh `$HOME`; the source tree it manages
  lives under `$PWD/dotfiles/`.

The temporary home is removed on every exit path, including failures, so state
cannot leak between tests.

## Helper functions provided by launcher.sh

| Helper | Purpose |
| --- | --- |
| `dfm <args...>` | Run the built `dfm` binary (debug preferred). |
| `write <content> <path>` | Write `content` to `path`, creating parent directories (`mkdir -p`). |
| `assert <expr...>` | Fail unless `test <expr...>` is true. |
| `assert_succ <cmd...>` | Fail unless the command exits `0`. |
| `assert_fail <cmd...>` | Fail if the command exits `0`. |
| `run_fail <cmd...>` | Assert the command exits **non-zero**; capture its combined stdout+stderr into `$FAIL_OUTPUT` for later `grep`. Preferred way to assert expected errors. |
| `assert_source <rel>` | Assert a file exists at `$PWD/dotfiles/<rel>`. |
| `assert_no_source <rel>` | Assert a file does **not** exist at `$PWD/dotfiles/<rel>`. |
| `assert_content_eq <file> <expected>` | Assert `file`'s content equals the expected string. |
| `add_file <name> [content]` | `write`, `dfm add`, `assert_source`, then echo the content (default: a fresh uuid). |
| `assert_encrypted <target_file> <expected>` | Decrypt `$PWD/dotfiles/<target_file>.encrypted` with `dfm decrypt` and assert its content equals `expected`. Requires the test to have set `obtain_password_shell_command` and `$PASSWORD`. |
| `uuid` | A uuid generator (a stub reading `/proc/sys/kernel/random/uuid` is installed if the real `uuid` command is absent). |

All helpers are exported into the test subshell.

## Test conventions and agreements

Follow these rules when writing or reviewing tests:

- **Isolation**: each test runs in a fresh sandbox `$HOME` with all XDG vars
  pointed into it. Never run `dfm` against the real home directory — `add`,
  `pull`, `forget` act on whole trees and can damage user files. For manual
  runs (outside `launcher.sh`) you must sandbox everything yourself.

- **No stdin**: tests must never read stdin; commands that prompt would hang
  the whole suite.

- **Assertion rule — grep over captured output** (the `set -eEu` gotchas):
  - Positive match → a plain pipeline is fine: `dfm status 2>/dev/null | grep -qF "pat"`.
  - Negative match → **never** use `! cmd | grep -qF` (POSIX `-e` ignores
    `!`-inverted commands, producing a false pass). Capture first and let
    `grep` be the only command: `RES=$(...); assert_fail grep -qF "pat" <<<"$RES"`.
  - Expected failure → `run_fail dfm ...`, then `assert_succ grep -qF "msg" <<<"$FAIL_OUTPUT"`.
  - Under `set -e`, `RES=$(… dfm …)` inherits a non-zero rc — guard it with
    `|| true` when the command may legitimately fail.
  - Files starting with `-` or `+` need `grep -qF -- "-$VAR"`.
  - Prefer redirecting stderr to a file and checking it over `dfm | grep`
    pipes.

- **Environment for sub-commands**: when a helper needs to pass an env var to
  the binary use the `env K=V "$BIN"` form (positional `"$@"` are not treated
  as env assignments).

## Debugging a failing test

1. Run the failing test alone with a full trace:
   `bash tests/launcher.sh test_x.sh` (omit `-q` to see the `-x` trace).
2. Read the failing assertion and the lines immediately above it before
   touching code.
3. If the trace shows success but wrong output, add **one** `eprintln!` at the
   point the asserted value is computed, print the actual value, and compare it
   to the expectation. Remove it after.

## Summary of the whole suite

```shell
# full run (quiet)
bash tests/launcher.sh -q
# single test with trace
bash tests/launcher.sh test_diff_all.sh
```
