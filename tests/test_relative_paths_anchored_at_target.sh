#!/usr/bin/env bash

# Relative PATH arguments must be resolved against the target directory, not
# against the current working directory. Regression: running
# `dfm diff file.txt` from inside an ignored subdirectory resolved the name to
# <subdir>/file.txt and wrongly reported it as ignored instead of diffing the
# managed target file ~/file.txt.

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

# From inside the ignored directory the bare name must still mean ~/file.txt.
(cd "$HOME/abc" && dfm status --porcelain) >status.out 2>/dev/null
assert_succ grep -qF "M 	file.txt" status.out

(cd "$HOME/abc" && dfm diff file.txt) >diff.out 2>/dev/null
assert_succ grep -qF -- "-$MODIFIED" diff.out
assert_succ grep -qF -- "+$CONTENT" diff.out

# add and forget relative names are anchored at the target dir as well
write newcontent newfile.txt
(cd "$HOME/abc" && dfm add newfile.txt)
assert_source "newfile.txt"

(cd "$HOME/abc" && dfm forget newfile.txt)
assert_no_source "newfile.txt"

# a source-side relative path still resolves through its location under the
# target directory (the newer target copy is intentionally kept)
(cd "$HOME/abc" && assert_succ dfm pull --force dotfiles/file.txt)
