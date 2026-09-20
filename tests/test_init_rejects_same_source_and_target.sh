# init must reject a source directory equal to the target directory: with
# equal paths, later "remove the source" commands (purge, forget --force)
# would wipe the whole target directory.

# same directory given for both arguments
run_fail dfm init dotfiles dotfiles
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

# a rejected init must not have created the state file
assert_fail test -f "$XDG_STATE_HOME/dfm/state.toml"

# source "." (== $HOME) with the default target $HOME
run_fail dfm init .
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

# dry-run must reject as well
run_fail dfm init -n . .
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

