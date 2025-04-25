"use client"

import { gql } from "@apollo/client"
import { use } from "react"

import { CreditFacilityTransactions } from "./transaction"

import { useGetCreditFacilityTransactionsQuery } from "@/lib/graphql/generated"

gql`
  fragment CreditFacilityTransactionsFragment on CreditFacility {
    id
    creditFacilityId
    transactions {
      ... on CreditFacilityIncrementalPayment {
        cents
        recordedAt
        txId
      }
      ... on CreditFacilityCollateralUpdated {
        satoshis
        recordedAt
        action
        txId
      }
      ... on CreditFacilityOrigination {
        cents
        recordedAt
        txId
      }
      ... on CreditFacilityCollateralizationUpdated {
        state
        collateral
        outstandingInterest
        outstandingDisbursal
        recordedAt
        price
      }
      ... on CreditFacilityDisbursalExecuted {
        cents
        recordedAt
        txId
      }
      ... on CreditFacilityInterestAccrued {
        cents
        recordedAt
        txId
        days
      }
    }
  }

  query GetCreditFacilityTransactions($id: UUID!) {
    creditFacility(id: $id) {
      ...CreditFacilityTransactionsFragment
    }
  }
`

interface CreditFacilityTransactionsPageProps {
  params: Promise<{
    "credit-facility-id": string
  }>
}

export default function CreditFacilityTransactionsPage({
  params,
}: CreditFacilityTransactionsPageProps) {
  const { "credit-facility-id": creditFacilityId } = use(params)
  const { data } = useGetCreditFacilityTransactionsQuery({
    variables: { id: creditFacilityId },
    fetchPolicy: "cache-and-network",
  })

  if (!data?.creditFacility) return null

  return <CreditFacilityTransactions creditFacility={data.creditFacility} />
}
