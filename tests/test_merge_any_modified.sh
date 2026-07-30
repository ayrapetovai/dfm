# dfm merge PATH runs the merge tool even when only one side was modified.
# Without a path, only BothModified entries are merged (existing behavior).

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

original="$(uuid)"
target_mod="$(uuid)"
source_mod="$(uuid)"
both_mod="$(uuid)"

# === Case 1: TargetModified + explicit path → merge runs ===
write "$original" file.txt
dfm add file.txt
write "$target_mod" file.txt
dfm merge file.txt
assert_content_eq "file.txt" "$target_mod"
assert_content_eq "$PWD/dotfiles/file.txt" "$target_mod"
assert_source "file.txt"

# === Case 2: SourceModified + explicit path → merge runs ===
dfm add file.txt
write "$source_mod" "$PWD/dotfiles/file.txt"
write "$original" file.txt  # revert target
dfm merge file.txt
# merge tool "cp {target} {result}" keeps target version
assert_content_eq "file.txt" "$original"
assert_content_eq "$PWD/dotfiles/file.txt" "$original"
assert_source "file.txt"

# === Case 3: TargetModified + no path → should skip ===
dfm add file.txt
write "$target_mod" file.txt
# both target and source exist, but only target was modified
# merge (no path) should skip non-BothModified
dfm merge 2>&1
# file should still be target_mod (unchanged by merge)
assert_content_eq "file.txt" "$target_mod"

# === Case 4: SourceModified + no path → should skip ===
dfm add file.txt
write "$source_mod" "$PWD/dotfiles/file.txt"
write "$original" file.txt
dfm merge 2>&1
assert_content_eq "file.txt" "$original"

# === Case 5: BothModified + explicit path → merge runs ===
dfm add file.txt
write "$both_mod" file.txt
write "$source_mod" "$PWD/dotfiles/file.txt"
dfm merge file.txt
assert_content_eq "file.txt" "$both_mod"
assert_content_eq "$PWD/dotfiles/file.txt" "$both_mod"

# === Case 6: BothModified + no path → merge runs ===
dfm add file.txt
write "$both_mod" file.txt
write "$source_mod" "$PWD/dotfiles/file.txt"
dfm merge
assert_content_eq "file.txt" "$both_mod"
assert_content_eq "$PWD/dotfiles/file.txt" "$both_mod"
