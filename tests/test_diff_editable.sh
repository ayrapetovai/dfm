# `dfm diff --editable PATH` edits both sides independently: the diff tool is
# given a writable copy of the target and of the source, and every copy it
# changed is written back to its own file. The two sides are saved as the tool
# left them — they may end up differing, which is the point. The sync state is
# updated after a write.

CONTENT="$(uuid)"
EDITED="$(uuid)"
EDITED_AGAIN="$(uuid)"

dfm init dotfiles

write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# Editing only the {target} copy saves only the target: the source is untouched
# even though the pair stops being equal.
write "$EDITED" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {target}"
dfm diff --editable file.txt
assert_content_eq "file.txt" "$EDITED"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# The scratch copies are removed.
assert ! -e "$PWD/dotfiles/.current_diff"

# Editing only the {source} copy on the now-differing pair saves only the
# source (short `-e` spelling): already-diverged files are the purpose of
# `--editable`, not an error.
write "$EDITED_AGAIN" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {source}"
dfm diff -e file.txt
assert_content_eq "file.txt" "$EDITED"
assert_content_eq "$PWD/dotfiles/file.txt" "$EDITED_AGAIN"

# Editing both copies writes each back independently, even when they disagree.
cat > diverge.sh <<'TOOL'
#!/usr/bin/env bash
echo "target side" > "$1"
echo "source side" > "$2"
TOOL
chmod +x diverge.sh
dfm config --set diff_editable_tool_command "$HOME/diverge.sh {target} {source}"
dfm diff -e file.txt
assert_content_eq "file.txt" "target side"
assert_content_eq "$PWD/dotfiles/file.txt" "source side"

# A per-side edit that left the pair differing must NOT be recorded as
# synchronized: the read-only mode still treats the pair as diverged and runs
# the diff tool instead of printing "is synchronized".
dfm config --set diff_tool_command "true"
diff_out="$(dfm diff file.txt 2>/dev/null)"
assert_fail grep -qF "file.txt is synchronized" <<<"$diff_out"

# Editing both copies to the same content synchronizes the pair again, so the
# read-only `diff` mode reports it as synchronized afterwards.
cat > unify.sh <<'TOOL'
#!/usr/bin/env bash
echo "unified" > "$1"
echo "unified" > "$2"
TOOL
chmod +x unify.sh
dfm config --set diff_editable_tool_command "$HOME/unify.sh {target} {source}"
dfm diff -e file.txt
assert_content_eq "file.txt" "unified"
assert_content_eq "$PWD/dotfiles/file.txt" "unified"
assert_succ grep -qF "file.txt is synchronized" <<<$(dfm diff file.txt 2>/dev/null)

# A source path names the same pair.
write "$CONTENT" edit.txt
dfm config --set diff_editable_tool_command "cp $HOME/edit.txt {target}"
dfm diff -e dotfiles/file.txt
assert_content_eq "file.txt" "$CONTENT"
assert_content_eq "$PWD/dotfiles/file.txt" "unified"

# A tool that changes nothing leaves both sides alone and succeeds.
dfm config --set diff_editable_tool_command "true"
dfm diff -e file.txt
assert_content_eq "file.txt" "$CONTENT"
assert_content_eq "$PWD/dotfiles/file.txt" "unified"