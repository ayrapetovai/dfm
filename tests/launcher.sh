#!/usr/bin/bash

set -euP

QUIET=""
while [ $# -gt 0 ]; do
    case "$1" in
        -q) QUIET="1"; shift ;;
        --) shift; break ;;
        *) break ;;
    esac
done

TEST_FILE_TO_RUN="${1:-}"

PROGRAMM_NAME_IN_SHELL="dfm"
EXECUTABLE_NAME="dfm"

TESTS_DIR=`readlink -f "$(dirname "$0")"`
PROJECT_ROOT=$(readlink -f "$TESTS_DIR/../")
DEBUG_EXECUTABLE="$PROJECT_ROOT/target/debug/$EXECUTABLE_NAME"
RELEASE_EXECUTABLE="$PROJECT_ROOT/target/release/$EXECUTABLE_NAME"

EXECUTABLE=
if [[ -f "$DEBUG_EXECUTABLE" ]]; then
    EXECUTABLE="$DEBUG_EXECUTABLE"
elif [[ -f "$RELEASE_EXECUTABLE" ]]; then
    EXECUTABLE="$RELEASE_EXECUTABLE"
else
    echo "project is not built"
    exit 1
fi
export EXECUTABLE

eval 'function '$PROGRAMM_NAME_IN_SHELL'() { "$EXECUTABLE" "$@" && sleep 0.002s; }'
export "$PROGRAMM_NAME_IN_SHELL"

function write() {
    mkdir -p "$(dirname "$2")" && echo "$1" > "$2" && sleep 0.002
}
export write;

function assert() {
    if ! test "$@"; then
        echo "Assertion failed"
        exit 1
    fi
    return 0
}
export assert

function assert_succ() {
    if ! "$@"; then
        echo "Assertion failed"
        exit 1
    fi
}
export assert_succ

function assert_fail() {
    if "$@"; then
        echo "Assertion failed"
        exit 1
    fi
}
export assert_fail

# Assert that a file exists in the source directory ($PWD/dotfiles/).
# Retries for up to ~1s to handle CI filesystem latency.
function assert_source() {
    local file="$PWD/dotfiles/$1"
    [ -f "$file" ] && return 0
    echo "Assertion failed: source file $file not found after 1s"
    exit 1
}
export -f assert_source

# Assert that a file does NOT exist in the source directory.
function assert_no_source() {
    assert_fail test -f "$PWD/dotfiles/$1"
}
export -f assert_no_source

# Assert that a file's content matches the expected string.
# Retries for up to ~1s to handle CI filesystem latency.
function assert_content_eq() {
    local file="$1"
    local expected="$2"
    local actual
    actual="$(cat "$file" 2>/dev/null)"
    if [ "$actual" = "$expected" ]; then
        return 0
    fi
    echo "Assertion failed: file $file content mismatch"
    echo "  expected: $expected"
    echo "  actual:   $(cat "$file" 2>/dev/null || echo '<unreadable>')"
    exit 1
}
export -f assert_content_eq

# Stub for uuid command for CI
if ! command -v uuid > /dev/null 2>&1; then
  function uuid() {
    cat /proc/sys/kernel/random/uuid
  }
fi

# Create a file with optional content, add it under management,
# verify it landed in source, and echo the content for later use.
function add_file() {
    local name="$1"
    local content="${2:-$(uuid)}"
    write "$content" "$name"
    dfm add "$name"
    assert_source "$name"
    echo "$content"
}
export -f add_file

# Decrypt $PWD/dotfiles/<target>.encrypted with $PASSWORD and assert its
# content matches the expected value.
# Usage: assert_encrypted <target_file> <expected_content>
# Requires $PASSWORD to be set in the calling test.
function assert_encrypted() {
    local target_file="$1"
    local expected="$2"

    if command -v 7z > /dev/null 2>&1; then
        rm -f "$target_file"
        7z -p"$PASSWORD" x -y "${PWD}/dotfiles/${target_file}.encrypted" > /dev/null 2>&1
        assert_content_eq "$target_file" "$expected"
    else
      return 0
    fi
}
export -f assert_encrypted

# mktemp creates directory in the /tmp wich is mounted to memory filesystem
readonly TMP_HOME=$(mktemp -d)
trap 'rm -rf -- "$TMP_HOME"' EXIT

TEST_FILE_TO_RUN_ABS=
if [ -n "$TEST_FILE_TO_RUN" ]; then
  TEST_FILE_TO_RUN_ABS=$(readlink -f "$TEST_FILE_TO_RUN")
fi

export HOME="$TMP_HOME"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_CACHE_HOME="$HOME/.cache"
export XDG_STATE_HOME="$HOME/.local/state"
cd $HOME

TEST_CASES=$(find "$TESTS_DIR" -type f -name 'test*.sh' -printf "%p\n")
TEST_COUNT=$(echo "$TEST_CASES" | wc -l)
if [ -n "$TEST_FILE_TO_RUN_ABS" ]; then
    echo "running 1 test (of $TEST_COUNT)"
else
    echo "running $TEST_COUNT tests"
fi

SUCCED_COUNTER=0
FAILED_COUNTER=0

run_test() {
    local test_file="$1"
    if [ -n "$QUIET" ]; then
        ( set -eEu; source "$test_file" ) > /dev/null 2>&1
    else
        local tmp; tmp=$(mktemp)
        if ( set -eEu -x; source "$test_file" ) >"$tmp" 2>&1; then
            rm -f "$tmp"
            return 0
        else
            cat "$tmp"
            rm -f "$tmp"
            return 1
        fi
    fi
}

if [ -n "$TEST_FILE_TO_RUN_ABS" ]; then
    test_name="$(basename $TEST_FILE_TO_RUN_ABS)"
    if run_test "$TEST_FILE_TO_RUN_ABS"; then
        echo "---- $test_name ✅"
        SUCCED_COUNTER=$((SUCCED_COUNTER + 1))
    else
        echo "---- $test_name ❌"
        FAILED_COUNTER=$((FAILED_COUNTER + 1))
    fi
else
    for test_case in $TEST_CASES; do
        test_name="$(basename $test_case)"

        # launch test
        if run_test "$test_case"; then
            echo "---- $test_name ✅"
            SUCCED_COUNTER=$((SUCCED_COUNTER + 1))
        else
            echo "---- $test_name ❌"
            FAILED_COUNTER=$((FAILED_COUNTER + 1))
        fi

        # remove all garbade generated by the test
        find "$TMP_HOME" -mindepth 1 -exec rm -rf {} \; 2>/dev/null || true
    done;
fi

echo "succed $SUCCED_COUNTER"
echo "failed $FAILED_COUNTER"

test $FAILED_COUNTER -eq 0
exit $?
