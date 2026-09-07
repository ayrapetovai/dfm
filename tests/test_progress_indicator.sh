# Progress indicator (ActionBar) honors non-interactive output.
#
# The action bar renders on stdout only when stdout is a terminal. All tests
# here run with stdout redirected (or captured), i.e. never a TTY, so the bar
# must never emit its text into the stream at ANY verbosity level — program
# output stays byte-clean for pipes, scripts, and the log-capture harness.
# The actual bar rendering/format is covered by Rust unit tests; these
# assertions guard the "never pollute non-TTY output" contract.

dfm init dotfiles

# Capture helper: run dfm capturing stdout+stderr into a var.
# $EXECUTABLE (the binary) is used directly instead of the `dfm` shell
# function so the `set -x` shell-trace lines are not captured into the file.
capture() {
    local out; out="$(mktemp)"
    "$EXECUTABLE" "$@" >"$out" 2>&1
    cat "$out"
    rm -f "$out"
}

# Create a batch large enough that the old per-file analysis loop would have
# fired progress heartbeats (100 files / 500 walk entries).
# Plain redirections (not `write`): the commands are bulk add/pull/sync over
# many files, so per-file sleep/mkdir would only slow the loop down.
for i in $(seq 1 600); do
    echo "content $i" > "file_$i.txt"
done

# The reading/processing bar text must not appear in captured output at any
# verbosity level (-v 0, default -v 1, verbose -v 2/3).
for V in 0 1 2 3; do
    OUTPUT="$(capture -v $V add)"
    if echo "$OUTPUT" | grep -qE "reading |processing |processed |traversing\.\.\."; then
        echo "Assertion failed: progress text leaked into captured add output at -v $V"
        exit 1
    fi
done

# pull and sync must be equally clean on non-TTY stdout.
for CMD in pull sync; do
    OUTPUT="$(capture -v 0 $CMD)"
    if echo "$OUTPUT" | grep -qE "reading |processing |processed |traversing\.\.\."; then
        echo "Assertion failed: progress text leaked into captured $CMD output"
        exit 1
    fi
done

# status must be clean at -v 0 (default) across all output modes; the bar is
# only ever omitted for --porcelain on a TTY, and never visible when piped.
for MODE in "" "--porcelain" "--short" "--all"; do
    OUTPUT="$(capture -v 0 status $MODE)"
    if echo "$OUTPUT" | grep -qE "reading |processing |processed |traversing\.\.\."; then
        echo "Assertion failed: progress text leaked into status $MODE output"
        exit 1
    fi
done

# Nothing extra is emitted to a captured stdout at -v 0 for add (single line
# model: the bar would render zero lines because there is no TTY).
OUTPUT="$(capture -v 0 add)"
if [ -n "$OUTPUT" ]; then
    echo "Assertion failed: captured add at -v 0 produced unexpected output"
    exit 1
fi