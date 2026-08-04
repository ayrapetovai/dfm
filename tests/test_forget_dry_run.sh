# dry-run prevents deletion of a source file
dfm init dotfiles

write "content" file.txt
dfm add file.txt

STATE_FILE="$PWD/.local/state/dfm/state.toml"
# the sync entry key appears in state.toml exactly once while managed
assert "1" = "$(grep -c 'file.txt' "$STATE_FILE")"

# dry-run forget — nothing should be removed, state entry must survive
dfm forget --dry-run file.txt
assert_source "file.txt"
assert -f file.txt
assert "1" = "$(grep -c 'file.txt' "$STATE_FILE")"

# dry-run + force — dry-run must still win
dfm forget --dry-run --force file.txt
assert_source "file.txt"
assert -f file.txt
assert "1" = "$(grep -c 'file.txt' "$STATE_FILE")"

# actual forget removes source and state entry
dfm forget file.txt
assert_no_source "file.txt"
assert -f file.txt
assert "0" = "$(grep -c 'file.txt' "$STATE_FILE")"
