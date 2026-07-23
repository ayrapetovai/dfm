# H1 — nothing to do: empty patterns with no paths
dfm init dotfiles

# dfm ignore -p with no pattern values and no paths
# patterns = Some([]), paths = None → traversed_paths = &vec![]
# All three collections empty → "nothing to do"
dfm ignore -p
