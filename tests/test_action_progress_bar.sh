# The action-phase progress bar renders only when stdout is a terminal: plain
# redirections and pipes must not emit it, while a pty (via `script`) must show
# the self-overwriting `[####....] done/total` frames reaching done == total.

dfm init dotfiles

for i in 1 2 3; do
    write "content $i" "file_$i.txt"
done

# Planned-task count for this exact setup (dfm also manages its own config),
# derived from the dry-run planning line so the assertion tracks reality.
TASKS="$( "$EXECUTABLE" -n -v 3 add 2>&1 | grep -oE '::copy procedure begins, [0-9]+ tasks' | grep -oE '[0-9]+' )"
[ -n "$TASKS" ] || {
    echo "Assertion failed: could not determine the planned task count"
    exit 1
}

if ! command -v script >/dev/null 2>&1; then
    echo "script(1) not available, skipping the pty part of the test"
    exit 0
fi

TTY_CAPTURE="$(mktemp)"

# TTY run for `add`: the bar must advance through every planned task and end
# on done == total.
script -qec "$EXECUTABLE add" "$TTY_CAPTURE" >/dev/null 2>&1
if ! grep -qE "\[[#-]+\] $TASKS/$TASKS" "$TTY_CAPTURE"; then
    echo "Assertion failed: action-phase progress bar not rendered on a TTY for add"
    exit 1
fi

# TTY run for `status`: Phase 1 (state entries) also drives the bar and must
# reach done == total (one entry per added file).
script -qec "$EXECUTABLE status" "$TTY_CAPTURE" >/dev/null 2>&1
if ! grep -qE "\[[#-]+\] $TASKS/$TASKS" "$TTY_CAPTURE"; then
    echo "Assertion failed: progress bar not rendered on a TTY for status"
    exit 1
fi

# Non-TTY run: the bar must not appear even when there are real tasks to run.
for i in 4 5; do
    write "content $i" "extra_$i.txt"
done
PLAIN_OUTPUT="$( "$EXECUTABLE" add 2>&1 )"
if echo "$PLAIN_OUTPUT" | grep -qE '\[[#-]*\] [0-9]+/[0-9]+'; then
    echo "Assertion failed: progress bar rendered without a TTY"
    exit 1
fi

rm -f "$TTY_CAPTURE"