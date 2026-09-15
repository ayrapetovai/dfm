# `dfm decrypt FILE -` streamed the plaintext to stdout: bit-identical to the
# original, no extra output mixed in, and no file named `-` is created.
# A second positional other than `-` and `-` combined with `--output` both fail.

PASSWORD="$(uuid)"
CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "printf '%s\n' '$PASSWORD'"

write "$CONTENT" secret.txt
dfm encrypt secret.txt -o secret.txt.encrypted

dfm decrypt secret.txt.encrypted - >restored.txt
assert_content_eq "restored.txt" "$CONTENT"

# the decrypted bytes are exactly the original content, with no extra message
cmp -s restored.txt secret.txt

# no file literally named `-` is created in the current directory
assert_fail test -f ./-

# a second positional that is not `-` is rejected
run_fail dfm decrypt secret.txt.encrypted out.txt
assert_succ grep -qF "the only second positional accepted by decrypt is '-'" <<<"$FAIL_OUTPUT"

# `-` is mutually exclusive with `--output`
run_fail dfm decrypt secret.txt.encrypted -o out.txt -
assert_succ grep -qF -- "--output and '-' (stdout) are mutually exclusive" <<<"$FAIL_OUTPUT"

