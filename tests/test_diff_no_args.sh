# `dfm diff` with no paths must do nothing: no output, exit 0 — even when
# the program is not initialized yet.

dfm diff >out.txt 2>/dev/null
assert ! -s out.txt

dfm init dotfiles
dfm diff >out.txt 2>/dev/null
assert ! -s out.txt
