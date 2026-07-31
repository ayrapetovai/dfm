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

# Create a batch large enough to trigger the bulk-progress heartbeats
# (analysis loop fires every 100 files, traversal every 500 entries).
for i in $(seq 1 600); do
    write "content $i" "file_$i.txt"
done

# progress is visible at every verbosity level, including -v 0 and the default
for V in 0 1 2; do
    OUTPUT="$(capture -v $V add)"

    # analysis-loop heartbeat fires periodically
    if ! echo "$OUTPUT" | grep -q "processed 100/"; then
        echo "Assertion failed: no analysis-loop progress heartbeat at -v $V"
        exit 1
    fi

    # traversal heartbeat fires once 500 entries are visited
    if ! echo "$OUTPUT" | grep -q "traversing... 500 entries visited"; then
        echo "Assertion failed: no traversal progress heartbeat at -v $V"
        exit 1
    fi
done

# at -v 0 nothing else is printed, so progress must render on ONE line
# (updates joined by \r, never \n) and that line must be erased when done.
OUTPUT="$(capture -v 0 add)"

if [ "$(printf '%s' "$OUTPUT" | wc -l)" -ne 0 ]; then
    echo "Assertion failed: progress emitted multiple lines at -v 0"
    exit 1
fi

# emulate a terminal (\r returns to column 0 and overwrites); after the
# operation the final visible line must be empty (erased).
RENDERED="$(printf '%s' "$OUTPUT" | awk 'BEGIN{line=""; cr=sprintf("%c",13)} {for(i=1;i<=length($0);i++){c=substr($0,i,1); if(c==cr) line=""; else line=line c}} END{gsub(/[ \t]+$/,"",line); print line}')"
if [ -n "$RENDERED" ]; then
    echo "Assertion failed: progress line left residue on screen: '$RENDERED'"
    exit 1
fi

# small operations (< 100 files) stay quiet — no noise on tiny batches
mkdir -p small
for i in $(seq 1 5); do
    write "content $i" "small/file_$i.txt"
done
SMALL_OUTPUT="$(capture add small)"
if echo "$SMALL_OUTPUT" | grep -q "processed"; then
    echo "Assertion failed: progress heartbeat fired for a tiny batch"
    exit 1
fi
