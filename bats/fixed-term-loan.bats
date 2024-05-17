#!/usr/bin/env bats

load "helpers"

setup_file() {
  start_server
}

teardown_file() {
  stop_server
  stop_rust_example
}

@test "fixed-term-loan: can create a loan" {

  variables=$(
    jq -n \
    '{
      input: {
        bitfinexUserName: "bitfinexUserName",
      }
    }'
  )
  exec_graphql 'fixed-term-loan-create' "$variables"
  id=$(graphql_output '.data.fixedTermLoanCreate.loan.loanId')
  [[ "$id" != null ]] || exit 1;
}
