#!/usr/bin/bash

set -eux

TESTS_DIR=`readlink -f "$(dirname "$0")"`

PROJECT_ROOT=$(readlink -f "$TESTS_DIR/../")
# echo "PROJECT_ROOT=$PROJECT_ROOT"
# echo "TESTS_DIR=$TESTS_DIR"

DEBUG_EXECUTABLE="$PROJECT_ROOT/target/debug/dfm"
RELEASE_EXECUTABLE="$PROJECT_ROOT/target/release/dfm"

EXECUTABLE=""
if [[ -f "$DEBUG_EXECUTABLE" ]]; then
    EXECUTABLE="$DEBUG_EXECUTABLE"
elif [[ -f "$RELEASE_EXECUTABLE" ]]; then
    EXECUTABLE="$RELEASE_EXECUTABLE"
else
    echo "project is not built"
    exit 1
fi

export EXECUTABLE

function dfm() {
    "$EXECUTABLE" "$@"
}

export dfm

TMP_HOME=$(mktemp -d)
export HOME="$TMP_HOME"
cd $HOME

TEST_CASES=$(find $TESTS_DIR -name 'test*.sh')

echo "running tests"

for test_case in "$TEST_CASES" ; do
    test_name="$(basename $test_case)"
    if ( source "$test_case" ) ; then
        echo "---- $test_name ✅"
    else
        echo "---- $test_name ❌"
    fi
done

