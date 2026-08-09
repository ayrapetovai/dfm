# `status --porcelain` must be deterministic: the same set of files always
# produces the same line order across runs (Phase 1 iterates a HashMap).

dfm init dotfiles

for f in zeta gamma alpha delta; do
  echo "$f" >"$f.txt"
  dfm add "$f.txt"
done

out1="$(dfm status --porcelain --all 2>/dev/null)"
out2="$(dfm status --porcelain --all 2>/dev/null)"

# two consecutive runs must be byte-identical
assert "$out1" = "$out2"

# and the output must be sorted by path
sorted=$(printf "%s\n" "$out1" | sort)
assert "$out1" = "$sorted"

