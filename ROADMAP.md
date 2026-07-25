# `dfm status` — Specification

## Output format (default)

Categorized list grouped by action, modelled after `git status`. Only actionable items shown by default.

```
$ dfm status
Source: ~/dotfiles  (branch: main, up to date)
Target: ~

Changes to merge:        2
  MM  .bashrc
  MM  .config/git/config

Changes to add:          1
  M   .ssh/config

Changes to pull:         1
   M dot-bashrc

Unmanaged files:         3
  ??  .vimrc
  ??  .config/foo/bar.conf
  ??  .local/bin/script.sh

Ignored:                 4
  !!  .cache/
  !!  node_modules/
  !!  .npm/
  !!  .config/baz/token

Unused ignore patterns:  1
  !P  /tmp/.*
```

### Two-letter status codes (`--short` / `--porcelain`)

Used by both `--short` (human-readable short) and `--porcelain` (stable machine-readable).

| Code | Meaning |
|---|---|---|
| `MM` | BothModified — target and source changed independently |
| `M ` | TargetModified — only target changed (can `dfm add`) |
| ` M` | SourceModified — only source changed (can `dfm pull`) |
| `--` | NonModified — up to date |
| `LL` | Managed symlink — target symlink points to managed pointee |
| `NM` | NeverSynchronized — no sync record exists |
| `??` | Unmanaged — exists in target only, untracked |
| `?L` | Unmanaged symlink — symlink not tracked by dfm |
| `!!` | Ignored — matches an ignore pattern |
| `!L` | Ignored symlink — symlink matching an ignore pattern |
| `!P` | Unused pattern — ignore regex that matches no file |
| `!?` | Unpulled — exists in source only, not yet pulled |

`--short` may add color or alignment whitespace. `--porcelain` is stable ASCII-only, tab-separated: `CODE\tPATH`.

### Git integration

If the source directory is a git repository, show helpful context in the header:

- Branch name
- Ahead / behind counts vs upstream
- Dirty / clean worktree
- Last commit summary

This gives a quick answer to "is my dotfiles repo in a good state?" without a separate `git status` call.

## Flags

| Flag | Short | Default | Description |
|---|---|---|---|
| *(none)* | | active | Show conflicts + modifications + unmanaged + unused patterns |
| `--all` | `-a` | — | Include up-to-date (`--`) and all ignored (`!!`) entries |
| `--short` | `-s` | — | One line per file, status code + path (human-readable short) |
| `--porcelain` | | — | Stable machine-readable output, tab-separated `CODE\tPATH` |
| `--conflicted` | `-c` | — | Only `MM` entries |
| `--modified` | `-m` | — | Only entries where target or source is modified (any `M` variant) |
| `--unmanaged` | `-U` | — | Only `??` entries |
| `--unpulled` | `-p` | — | Only `!?` entries (source-only) |
| `--ignored` | `-i` | — | Only `!!` entries |
| `--ignored-patterns` | `-l` | — | List all active ignore patterns |
| `--unused-patterns` | `-u` | — | List stale patterns (`!P`) |

## Pager

- Use a pager when output exceeds the terminal height.
- Respect `$PAGER` env var. If unset, default to `less -FRSX` (like git).
- If `$PAGER` is empty or no terminal is detected, print directly without paging.
- `--porcelain` output must never be paged (pipe-safe).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Nothing to do — all files up to date or all requested filters matched nothing |
| 1 | Conflicts exist (`MM` entries) that need `dfm merge` |
| 2 | Unpulled or unmanaged files exist (`!?`, `??`) |
| 3 | Unused ignore patterns exist (`!P`) |

Multiple conditions OR the codes (e.g. conflicts + unpulled = 3). This allows scripting:

```bash
dfm status --conflicted && exit 0  # nothing to merge
dfm merge                           # resolve conflicts
```

## Implementation status

| Priority | Feature | Status |
|---|---|---|
| **P0** | Categorized list grouped by action (merge / add / pull / unmanaged / ignored) | ✅ Done |
| **P0** | Detect and show unpulled source files (`!?`) | ✅ Done |
| **P0** | Detect and show unmanaged target files (`??`) | ✅ Done |
| **P0** | Per-file sync status: BothModified / SourceModified / TargetModified / NonModified / NeverSynchronized | ✅ Done |
| **P0** | Show ignored files with the matching regex pattern | ✅ Done |
| **P1** | Unused (stale) ignore pattern detection | ✅ Done |
| **P1** | `--short` / `-s` | ✅ Done |
| **P1** | `--porcelain` stable output | ✅ Done |
| **P1** | Exit code differentiation (0/1/2/4 OR-combined) | ✅ Done |
| **P2** | Pager (`$PAGER` / default `less -FRSX`, screen-height detection) | ✅ Done |
| **P2** | Filter flags (`--conflicted`, `--modified`, `--unmanaged`, `--unpulled`, `--ignored`) | ✅ Done |
| **P2** | `--all` flag | ✅ Done |
| **P2** | `--ignored-patterns` / `--unused-patterns` | ✅ Done |
| **P2** | Symlink status codes (`?L`, `!L`, `LL`) | ✅ Done |
| **P3** | Git integration (branch, dirty/clean in header) | ✅ Done |
| **P3** | Ahead / behind counts vs upstream | ⏳ Not yet |
| **P3** | Last commit summary | ⏳ Not yet |

