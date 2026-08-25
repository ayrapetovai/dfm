#!/usr/bin/env bash

# Relative PATH arguments follow normal shell semantics: they are anchored at
# the current working directory and then normalized lexically. A command run
# from a subdirectory addresses files relative to that subdirectory (`../name`
# reaches the parent), and every resolved path must still land inside the
# managed tree (target or source directory) or it is rejected.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set diff_tool_command "diff -u {target} {source}"

write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify only the target
write "$MODIFIED" file.txt

mkdir -p abc
write other abc/other.txt
dfm ignore abc

# From the nested cwd the parent's managed file is addressed as ../file.txt.
(cd "$HOME/abc" && dfm status --porcelain ../file.txt) >status.out 2>/dev/null
assert_succ grep -qF "M 	file.txt" status.out

(cd "$HOME/abc" && dfm diff ../file.txt) >diff.out 2>/dev/null
assert_succ grep -qF -- "-$MODIFIED" diff.out
assert_succ grep -qF -- "+$CONTENT" diff.out

# The bare name is cwd-relative: inside abc/ there is no file.txt.
RES=$( (cd "$HOME/abc" && dfm status --porcelain file.txt) 2>&1 || true )
assert_succ grep -qF "does not exist" <<<"$RES"

# add and forget use the same anchoring from the nested directory
write newcontent newfile.txt
(cd "$HOME/abc" && dfm add ../newfile.txt)
assert_source "newfile.txt"

(cd "$HOME/abc" && dfm forget ../newfile.txt)
assert_no_source "newfile.txt"

# source-side paths are reached through the same shell-style navigation
# (the newer target copy is intentionally kept)
(cd "$HOME/abc" && assert_succ dfm pull --force ../dotfiles/file.txt)

# deeper nesting composes the same way: two levels up plus into sub/
write nested sub/nested.txt
RES=$( (cd "$HOME/abc" && dfm status --porcelain ../sub/nested.txt) 2>/dev/null )
assert_succ grep -qF $'??\tsub/nested.txt' <<<"$RES"
