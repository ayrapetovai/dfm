# Status codes: MM, M ,  M, --, NM, !?

dfm init dotfiles

write "base" "both.txt"
dfm add both.txt
write "modified_target" "both.txt"
write "modified_source" "$PWD/dotfiles/both.txt"

# BothModified (MM)
dfm status --short | grep -q "^MM both.txt$"
dfm status --porcelain | grep -q "^MM	both.txt$"
dfm status --conflicted | grep -q "both.txt"

# TargetModified (M ) — only target changed after add
write "base2" "target_only.txt"
dfm add target_only.txt
write "target_changed" "target_only.txt"
dfm status --short | grep -q "^M  target_only.txt$"

# SourceModified ( M) — only source changed after add
write "base3" "source_only.txt"
dfm add source_only.txt
write "source_modified" "$PWD/dotfiles/source_only.txt"
dfm status --short | grep -q "^ M source_only.txt$"

# NonModified (--) — up to date
write "base4" "uptodate.txt"
dfm add uptodate.txt
# Should not appear in default output
! dfm status 2>/dev/null | grep -q "uptodate.txt"
# Should appear with --all
dfm status --all 2>/dev/null | grep -qF -- "--  uptodate.txt"

# Unpulled (!?) — source exists, target deleted
write "base5" "unpulled.txt"
dfm add unpulled.txt
rm unpulled.txt
dfm status --short | grep -qF "!? unpulled.txt"
# filtered flag
dfm status --unpulled 2>/dev/null | grep -qF "!?  unpulled.txt"
