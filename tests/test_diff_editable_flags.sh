# `--editable` needs an explicit PATH, cannot be combined with `--all`, and
# never writes under `--dry-run`.

CONTENT="$(uuid)"
EDITED="$(uuid)"

dfm init dotfiles
write "$EDITED" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {target}"

write "$CONTENT" file.txt
dfm add file.txt

# No PATH: a CLI error, nothing is run.
run_fail dfm diff --editable
assert_succ grep -qF "PATH" <<<"$FAIL_OUTPUT"

# `--all` is the default batch mode, `--editable` requires an explicit PATH.
run_fail dfm diff --editable --all file.txt
assert_succ grep -qF "cannot be used with" <<<"$FAIL_OUTPUT"

# Dry run: the tool never runs and both sides keep their content.
dfm --dry-run diff --editable file.txt
assert_content_eq "file.txt" "$CONTENT"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"
assert ! -e "$PWD/dotfiles/.current_diff"
