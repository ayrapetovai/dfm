# C1 — ignore a non-existent path (canonicalize fails)
dfm init dotfiles

assert_fail dfm ignore /nonexistent/path
