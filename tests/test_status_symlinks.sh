# Status symlink codes: ?L, !L, LL
# Also verifies header shows both paths and source dir files are excluded.

dfm init dotfiles

write "a" "real_target.txt"
dfm add real_target.txt

# LL: managed symlink — add an existing symlink to a managed file
ln -s real_target.txt linked.txt
dfm add linked.txt

# LL should appear with --all (hidden by default like --)
dfm status --all 2>/dev/null | grep -qF "LL  linked.txt"
dfm status --short --all 2>/dev/null | grep -q "^LL linked.txt$"
dfm status --porcelain --all 2>/dev/null | grep -q $'^LL\tlinked.txt$'

# LL should NOT appear in default status (just like --)
! dfm status 2>/dev/null | grep -q "linked.txt"

# ?L: unmanaged symlink — symlink to file not in state
ln -s /nonexistent broken_link.txt
dfm status --short 2>/dev/null | grep -q "^?L broken_link.txt$"
dfm status --porcelain 2>/dev/null | grep -q $'^\\?L\tbroken_link.txt$'

# ?L should also appear in default output (like ??)
dfm status 2>/dev/null | grep -qF "?L  broken_link.txt"

# --unmanaged should include ?L
dfm status --unmanaged 2>/dev/null | grep -qF "?L  broken_link.txt"

# !L: ignored symlink — symlink matching an ignore pattern
ln -s /nonexistent ignore_this_link.txt
dfm ignore -p "ignore_this_link.txt"
# !L visible only with --all or --ignored
! dfm status 2>/dev/null | grep -q "ignore_this_link.txt"
dfm status --all 2>/dev/null | grep -qF "!L  ignore_this_link.txt"
dfm status --ignored 2>/dev/null | grep -qF "!L  ignore_this_link.txt"

# --short --all should show !L
dfm status --short --all 2>/dev/null | grep -q "^!L ignore_this_link.txt$"

# Source dir files must NOT appear in status

# Verify that internal source directory files (.dfm_root, etc.) are NOT in status
! dfm status --short --all 2>/dev/null | grep -q "^\?\? \.dfm_root$"
! dfm status --short --all 2>/dev/null | grep -q "^\?\? \.git$"
