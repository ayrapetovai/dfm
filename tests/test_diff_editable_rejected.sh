# `dfm diff --editable` runs on already-differing files — editing them is its
# purpose — and the only thing that discards an edit is a non-zero tool exit:
# then neither file is written and no scratch copies are left.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
EDITED="$(uuid)"

dfm init dotfiles
write "$EDITED" edit.txt

write "$CONTENT" file.txt
dfm add file.txt

# Sides that already differ are accepted: the tool runs and its edit is saved.
write "$MODIFIED" file.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {target}"
dfm diff --editable file.txt
assert_content_eq "file.txt" "$EDITED"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# A non-zero tool exit discards the edit even on a diverged pair.
dfm config --set diff_editable_tool_command "false"
run_fail dfm diff --editable file.txt
assert_succ grep -qF "diff tool exited with status" <<<"$FAIL_OUTPUT"
assert_content_eq "file.txt" "$EDITED"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"
assert ! -e "$PWD/dotfiles/.current_diff"