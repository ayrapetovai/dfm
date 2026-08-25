# Status: restricting the report to specific paths (absolute or relative to
# the target directory). Flags still apply within the given paths, folding
# behaves as in the full report, and the report is scoped to the request.

dfm init dotfiles

mkdir -p .config/a .config/b docs
write "x" ".config/a/f.txt"
write "y" ".config/b/g.txt"
write "z" "docs/readme.txt"
write "top" "rootfile.txt"

# Managed file, then modified on target side
dfm add .config/a/f.txt
write "modified" ".config/a/f.txt"

# Ignored directory inside the scoped region
mkdir -p .config/ignored_here
write "i" ".config/ignored_here/i.txt"
dfm ignore .config/ignored_here

# --- Full report shows everything ------------------------------------------
dfm status 2>/dev/null | grep -qF ".config/a/f.txt"
dfm status 2>/dev/null | grep -qF "rootfile.txt"
dfm status 2>/dev/null | grep -qF "docs/readme.txt"

# --- Relative dir path restricts the report --------------------------------
RES=$(dfm status .config 2>/dev/null)
assert_succ grep -qF ".config/a/f.txt" <<<"$RES"
assert_succ grep -qF ".config/b/g.txt" <<<"$RES"
assert_fail grep -qF "rootfile.txt" <<<"$RES"
assert_fail grep -qF "docs/readme.txt" <<<"$RES"

# --- Absolute path works the same ------------------------------------------
RES=$(dfm status "$PWD/docs" 2>/dev/null)
assert_succ grep -qF "docs/readme.txt" <<<"$RES"
assert_fail grep -qF "rootfile.txt" <<<"$RES"

# --- Multiple paths: union ------------------------------------------------
RES=$(dfm status .config/b docs 2>/dev/null)
assert_succ grep -qF ".config/b/g.txt" <<<"$RES"
assert_succ grep -qF "docs/readme.txt" <<<"$RES"
assert_fail grep -qF ".config/a/f.txt" <<<"$RES"

# --- Single file path ----------------------------------------------------
RES=$(dfm status .config/a/f.txt 2>/dev/null)
assert_succ grep -qF ".config/a/f.txt" <<<"$RES"
assert_fail grep -qF ".config/b/g.txt" <<<"$RES"

# --- Flags combine with paths --------------------------------------------
dfm status --porcelain .config 2>/dev/null | grep -qF "M 	.config/a/f.txt"
dfm status --short .config 2>/dev/null | grep -qF "?? .config/b/g.txt"

# --- Ignored entries are shown with --ignored within the scope -------------
dfm status --ignored .config 2>/dev/null | grep -qF "ignored_here"

# --- An explicitly named ignored path is reported, flags or not ------------
RES=$(dfm status .config/ignored_here/i.txt 2>/dev/null)
assert_succ grep -qF "!!" <<<"$RES"
assert_succ grep -qF "!!  .config/ignored_here/i.txt" <<<"$RES"
dfm status --porcelain .config/ignored_here/i.txt 2>/dev/null | grep -qF $'!!\t.config/ignored_here/i.txt'
# ...and the ignored directory itself, reached through a scoped parent
RES=$(dfm status .config 2>/dev/null)
assert_succ grep -qF ".config/ignored_here/" <<<"$RES"

# other flags keep priority: --modified hides ignored entries even when scoped
RES=$(dfm status --modified .config/ignored_here/i.txt 2>/dev/null)
assert_fail grep -qF "i.txt" <<<"$RES"

# --- Nonexistent path is an error ------------------------------------------
assert_fail dfm status no_such_path

# --- Directory collapsing still works within the scoped dir ----------------
mkdir -p scoped/dir1 scoped/dir3
write "a" "scoped/dir1/file"
write "b" "scoped/dir3/file"
RES=$(dfm status scoped 2>/dev/null)
assert_succ grep -qF "??  scoped/*" <<<"$RES"
assert_fail grep -qF "scoped/dir1/file" <<<"$RES"
rm -rf scoped

# --- No paths behaves as before (regression guard) --------------------------
dfm status 2>/dev/null | grep -qF "rootfile.txt"