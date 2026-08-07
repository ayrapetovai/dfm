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
echo "$RES" | grep -qF ".config/a/f.txt"
echo "$RES" | grep -qF ".config/b/g.txt"
assert_fail grep -qF "rootfile.txt" <<< "$RES"
assert_fail grep -qF "docs/readme.txt" <<< "$RES"

# --- Absolute path works the same ------------------------------------------
RES=$(dfm status "$PWD/docs" 2>/dev/null)
echo "$RES" | grep -qF "docs/readme.txt"
assert_fail grep -qF "rootfile.txt" <<< "$RES"

# --- Multiple paths: union ------------------------------------------------
RES=$(dfm status .config/b docs 2>/dev/null)
echo "$RES" | grep -qF ".config/b/g.txt"
echo "$RES" | grep -qF "docs/readme.txt"
assert_fail grep -qF ".config/a/f.txt" <<< "$RES"

# --- Single file path ----------------------------------------------------
RES=$(dfm status .config/a/f.txt 2>/dev/null)
echo "$RES" | grep -qF ".config/a/f.txt"
assert_fail grep -qF ".config/b/g.txt" <<< "$RES"

# --- Flags combine with paths --------------------------------------------
dfm status --porcelain .config 2>/dev/null | grep -qF "M 	.config/a/f.txt"
dfm status --short .config 2>/dev/null | grep -qF "?? .config/b/g.txt"

# --- Ignored entries are shown with --ignored within the scope -------------
dfm status --ignored .config 2>/dev/null | grep -qF "ignored_here"

# --- Nonexistent path is an error ------------------------------------------
assert_fail dfm status no_such_path

# --- Directory collapsing still works within the scoped dir ----------------
mkdir -p scoped/dir1 scoped/dir3
write "a" "scoped/dir1/file"
write "b" "scoped/dir3/file"
dfm status scoped 2>/dev/null | grep -qF "??  scoped/*"
RES=$(dfm status scoped 2>/dev/null)
assert_fail grep -qF "scoped/dir1/file" <<< "$RES"
rm -rf scoped

# --- No paths behaves as before (regression guard) --------------------------
dfm status 2>/dev/null | grep -qF "rootfile.txt"