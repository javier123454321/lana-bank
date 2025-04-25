"use client"

import { gql } from "@apollo/client"
import { use } from "react"

import { CreditFacilityDisbursals } from "./list"

import { useGetCreditFacilityDisbursalsQuery } from "@/lib/graphql/generated"

gql`
  fragment DisbursalOnFacilityPage on CreditFacilityDisbursal {
    id
    disbursalId
    amount
    status
    createdAt
  }

  query GetCreditFacilityDisbursals($id: UUID!) {
    creditFacility(id: $id) {
      id
      creditFacilityId
      disbursals {
        ...DisbursalOnFacilityPage
      }
    }
  }
`
export default function CreditFacilityDisbursalsPage({
  params,
}: {
  params: Promise<{ "credit-facility-id": string }>
}) {
  const { "credit-facility-id": creditFacilityId } = use(params)
  const { data } = useGetCreditFacilityDisbursalsQuery({
    variables: { id: creditFacilityId },
  })

  if (!data?.creditFacility) return null

  return <CreditFacilityDisbursals creditFacility={data.creditFacility} />
}
