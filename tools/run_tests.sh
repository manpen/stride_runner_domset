#!/bin/bash
source tools/assert.sh

TESTDIR="stride-logs/testing"
BIN="target/debug/runner"

prepare_env() {
  cargo build --all
  if [ ! -f .stride/metadata.db ]; then
      $BIN update
  fi

  rm -rf $TESTDIR
  mkdir -p $TESTDIR
}

assert_success() {
  $BIN $@ 2>&1 > /dev/null 2>/dev/null
  assert_eq 0 $? "Command should succeed"
}

assert_failed() {
  $BIN $@ > /dev/null 2>/dev/null
  assert_not_eq 0 $? "Command should fail"
}

lines_with_single_number_in() {
  cat $1 | grep -c '^[[:space:]]*[0-9][0-9]*[[:space:]]*$'
}

lines_with_edges() {
  cat $1 | grep -c '^[[:space:]]*[0-9][0-9]*[[:space:]][[:space:]]*[0-9][0-9]*[[:space:]]*$'
}

run_cargo_test() {
  cargo test 
}

test_export_instance() {
    echo "Run export instance test"
    local OUTPUT="$TESTDIR/476.gr"
    local ARGS="export-instance -i 476 -o $OUTPUT"

    rm -f $OUTPUT
    assert_success $ARGS
    local edges=$(lines_with_edges $OUTPUT)
    assert_eq 11 $edges "Exported instance should have 4 edges"
    
    # file exists; should not fail
    assert_failed $ARGS
    assert_success $ARGS -f # force overwrite
}


test_export_solution() {
  echo "Run export solution test"
  local OUTPUT="$TESTDIR/549.sol"
  local ARGS="export-solution -i 549 -s 02f17fd6-65a8-442b-b23e-c45709833614 -r 4d377e8d-9666-4d30-b4d3-a6be86ca847f -o $OUTPUT"

  rm -f $OUTPUT
  assert_success $ARGS

  # file exists; should not fail
  echo "ILLEGAL" > $OUTPUT
  assert_failed $ARGS

  # file exists; force overwrite should work
  assert_success $ARGS -f

  # file should have 3 numbers
  SIZE=$(lines_with_single_number_in "$OUTPUT")
  assert_eq "3" "$SIZE" "Exported solution should have score 2 and 3 numbers"
}



#####

if [ ! -f Cargo.toml ]; then
  echo "Please run this script from the root of the project"
  exit 1
fi


prepare_env
run_cargo_test
test_export_instance
test_export_solution