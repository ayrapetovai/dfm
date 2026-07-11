#!/usr/bin/bash

set -euP

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

eval 'function '$PROGRAMM_NAME_IN_SHELL'() { "$EXECUTABLE" "$@"; }'
export "$PROGRAMM_NAME_IN_SHELL"

readonly TMP_HOME=$(mktemp -d)
trap 'rm -rf -- "$TMP_HOME"' EXIT

export HOME="$TMP_HOME"
cd $HOME

TEST_CASES=$(find "$TESTS_DIR" -type f -name 'test*.sh' -printf "%p\n")
echo "running $(echo "$TEST_CASES" | wc -l) tests"

SUCCED_COUNTER=0
FAILED_COUNTER=0

for test_case in $TEST_CASES; do
    test_name="$(basename $test_case)"
    if ( set -eEux; source "$test_case" ) ; then
        echo "---- $test_name ✅"
        SUCCED_COUNTER=$((SUCCED_COUNTER + 1))
    else
        echo "---- $test_name ❌"
        FAILED_COUNTER=$((FAILED_COUNTER + 1))
    fi
done

echo "succed $SUCCED_COUNTER"
echo "failed $FAILED_COUNTER"

test $FAILED_COUNTER -eq 0
exit $?
