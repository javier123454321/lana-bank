#!/usr/bin/env bats

load "helpers"

PERSISTED_LOG_FILE="credit-facility.e2e-logs"
RUN_LOG_FILE="credit-facility.run.e2e-logs"

setup_file() {
  start_server
  login_superadmin
  reset_log_files "$PERSISTED_LOG_FILE" "$RUN_LOG_FILE"
}

teardown_file() {
  stop_server
  cp "$LOG_FILE" "$PERSISTED_LOG_FILE"
}

wait_for_customer_activation() {
  customer_id=$1

  variables=$(
    jq -n \
      --arg customerId "$customer_id" \
    '{ id: $customerId }'
  )
  exec_admin_graphql 'customer' "$variables"

  status=$(graphql_output '.data.customer.status')
  [[ "$status" == "ACTIVE" ]] || exit 1

}

wait_for_active() {
  credit_facility_id=$1

  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
    '{ id: $creditFacilityId }'
  )
  exec_admin_graphql 'find-credit-facility' "$variables"

  status=$(graphql_output '.data.creditFacility.status')
  [[ "$status" == "ACTIVE" ]] || exit 1

  disbursals=$(graphql_output '.data.creditFacility.disbursals')
  num_disbursals=$(echo $disbursals | jq -r '. | length')
  [[ "$num_disbursals" -gt "0" ]]
}

wait_for_disbursal() {
  credit_facility_id=$1

  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
    '{ id: $creditFacilityId }'
  )
  exec_admin_graphql 'find-credit-facility' "$variables"
  echo "disbursal | $i. $(graphql_output)" >> $RUN_LOG_FILE
  disbursals=$(graphql_output '.data.creditFacility.disbursals')
  num_disbursals=$(echo $disbursals | jq -r '. | length')
  [[ "$num_disbursals" -gt "1" ]]
}

wait_for_accruals() {
  expected_num_accruals=$1
  credit_facility_id=$2

  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
    '{ id: $creditFacilityId }'
  )
  exec_admin_graphql 'find-credit-facility' "$variables"
  echo "accrual | $i. $(graphql_output)" >> $RUN_LOG_FILE
  num_accruals=$(
    graphql_output '[
      .data.creditFacility.transactions[]
      | select(.__typename == "CreditFacilityInterestAccrued")
      ] | length'
  )

  [[ "$num_accruals" == "$expected_num_accruals" ]] || exit 1
}

wait_for_dashboard_disbursed() {
  before=$1
  disbursed_amount=$2

  expected_after="$(( $before + $disbursed_amount ))"

  exec_admin_graphql 'dashboard'
  after=$(graphql_output '.data.dashboard.totalDisbursed')

  [[ "$after" -eq "$expected_after" ]] || exit 1
}

wait_for_dashboard_payment() {
  before=$1
  payment_amount=$2

  expected_after="$(( $before - $payment_amount ))"

  exec_admin_graphql 'dashboard'
  after=$(graphql_output '.data.dashboard.totalDisbursed')

  [[ "$after" -eq "$expected_after" ]] || exit 1
}

ymd() {
  local date_value
  read -r date_value
  echo $date_value | cut -d 'T' -f1 | tr -d '-'
}

@test "credit-facility: can create" {
  # Setup prerequisites
  customer_id=$(create_customer)

  # retry 10 1 wait_for_customer_activation "$customer_id"

  variables=$(
    jq -n \
      --arg customerId "$customer_id" \
    '{
      id: $customerId
    }'
  )
  exec_admin_graphql 'customer' "$variables"

  deposit_account_id=$(graphql_output '.data.customer.depositAccount.depositAccountId')
  [[ "$deposit_account_id" != "null" ]] || exit 1

  facility=100000
  variables=$(
    jq -n \
    --arg customerId "$customer_id" \
    --arg disbursal_credit_account_id "$deposit_account_id" \
    --argjson facility "$facility" \
    '{
      input: {
        customerId: $customerId,
        facility: $facility,
        disbursalCreditAccountId: $disbursal_credit_account_id,
        terms: {
          annualRate: "12",
          accrualCycleInterval: "END_OF_MONTH",
          accrualInterval: "END_OF_DAY",
          oneTimeFeeRate: "5",
          duration: { period: "MONTHS", units: 3 },
          interestDueDuration: { period: "DAYS", units: 0 },
          liquidationCvl: "105",
          marginCallCvl: "125",
          initialCvl: "140"
        }
      }
    }'
  )

  exec_admin_graphql 'credit-facility-create' "$variables"
  credit_facility_id=$(graphql_output '.data.creditFacilityCreate.creditFacility.creditFacilityId')
  [[ "$credit_facility_id" != "null" ]] || exit 1

  cache_value 'credit_facility_id' "$credit_facility_id"
}

@test "credit-facility: can update collateral" {
  credit_facility_id=$(read_value 'credit_facility_id')

  variables=$(
    jq -n \
      --arg credit_facility_id "$credit_facility_id" \
    '{
      input: {
        creditFacilityId: $credit_facility_id,
        collateral: 50000000,
      }
    }'
  )
  exec_admin_graphql 'credit-facility-collateral-update' "$variables"
  credit_facility_id=$(graphql_output '.data.creditFacilityCollateralUpdate.creditFacility.creditFacilityId')
  [[ "$credit_facility_id" != "null" ]] || exit 1

  retry 10 1 wait_for_active "$credit_facility_id"
}

@test "credit-facility: can initiate disbursal" {
  credit_facility_id=$(read_value 'credit_facility_id')

  exec_admin_graphql 'dashboard'
  disbursed_before=$(graphql_output '.data.dashboard.totalDisbursed')

  amount=50000
  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
      --argjson amount "$amount" \
    '{
      input: {
        creditFacilityId: $creditFacilityId,
        amount: $amount,
      }
    }'
  )
  exec_admin_graphql 'credit-facility-disbursal-initiate' "$variables"
  disbursal_id=$(graphql_output '.data.creditFacilityDisbursalInitiate.disbursal.id')
  [[ "$disbursal_id" != "null" ]] || exit 1

  retry 10 1 wait_for_disbursal "$credit_facility_id"
  retry 10 1 wait_for_dashboard_disbursed "$disbursed_before" "$amount"
}

@test "credit-facility: records accrual" {
  credit_facility_id=$(read_value 'credit_facility_id')
  retry 30 2 wait_for_accruals 4 "$credit_facility_id"

  cat_logs | grep "interest accrual cycles completed for.*$credit_facility_id" || exit 1

  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
    '{ id: $creditFacilityId }'
  )
  exec_admin_graphql 'find-credit-facility' "$variables"
  graphql_output
  last_accrual=$(
    graphql_output '[
      .data.creditFacility.transactions[]
      | select(.__typename == "CreditFacilityInterestAccrued")
      ][0]'
  )

  amount=$(echo $last_accrual | jq -r '.cents')
  [[ "$amount" -gt "0" ]] || exit 1

  last_accrual_at=$(echo $last_accrual | jq -r '.recordedAt' | ymd)
  matures_at=$(graphql_output '.data.creditFacility.maturesAt' | ymd)
  [[ "$last_accrual_at" == "$matures_at" ]] || exit 1

  # assert_accounts_balanced
}

@test "credit-facility: record payment" {
  credit_facility_id=$(read_value 'credit_facility_id')

  exec_admin_graphql 'dashboard'
  disbursed_before=$(graphql_output '.data.dashboard.totalDisbursed')

  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
    '{ id: $creditFacilityId }'
  )
  exec_admin_graphql 'find-credit-facility' "$variables"
  interest_outstanding=$(graphql_output '.data.creditFacility.balance.interest.outstanding.usdBalance')
  total_outstanding=$(graphql_output '.data.creditFacility.balance.outstanding.usdBalance')

  disbursed_payment=25000
  amount="$(( $disbursed_payment + $interest_outstanding ))"
  variables=$(
    jq -n \
      --arg creditFacilityId "$credit_facility_id" \
      --argjson amount "$amount" \
    '{
      input: {
        creditFacilityId: $creditFacilityId,
        amount: $amount,
      }
    }'
  )
  exec_admin_graphql 'credit-facility-partial-payment' "$variables"
  balance=$(graphql_output '.data.creditFacilityPartialPayment.creditFacility.balance')

  updated_total_outstanding=$(echo $balance | jq -r '.outstanding.usdBalance')
  [[ "$updated_total_outstanding" -lt "$total_outstanding" ]] || exit 1

  updated_interest_outstanding=$(echo $balance | jq -r '.interest.outstanding.usdBalance')
  [[ "$updated_interest_outstanding" -eq "0" ]] || exit 1

  retry 10 1 wait_for_dashboard_payment "$disbursed_before" "$disbursed_payment"

  # assert_accounts_balanced
}
