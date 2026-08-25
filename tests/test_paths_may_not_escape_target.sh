#!/usr/bin/env bash

# A PATH argument that resolves outside the managed tree is rejected with a
# uniform error instead of producing mangled output. Regression: relative
# paths are anchored at the current working directory; without the scope check
# an escaping path (a `..` climbing past the tree root, or an absolute path
# elsewhere) made status print garbage entries like `??  ./../../..<abs>`,
# while add/diff/forget each reacted differently.

ESCAPE_MSG="outside the target directory"

dfm init dotfiles

# a stray file one level ABOVE the target directory ($HOME/..)
echo stale > "$HOME/../outside.txt"

mkdir -p sub
write content file.txt

# every scoped command rejects the escaping relative path
run_fail dfm status ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

run_fail dfm add ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

run_fail dfm pull ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

run_fail dfm merge ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

run_fail dfm forget ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

run_fail dfm ignore ../outside.txt
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

# absolute paths outside the tree are rejected just the same
run_fail dfm status /etc/hostname
assert_succ grep -qF "$ESCAPE_MSG" <<<"$FAIL_OUTPUT"

# diff stays permissive: it only reports, never manages
RES=$(dfm diff /etc/hostname 2>&1)
assert_succ grep -qF "not managed" <<<"$RES"

# shell-style navigation inside the tree is fine: from sub/, ../dotfiles/file.txt
# is the source-side counterpart of the managed file
assert_succ dfm add file.txt
(cd sub && assert_succ dfm status ../dotfiles/file.txt)
