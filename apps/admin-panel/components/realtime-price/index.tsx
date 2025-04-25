"use client"

import React from "react"
import { gql } from "@apollo/client"

import { useGetRealtimePriceUpdatesQuery } from "@/lib/graphql/generated"

gql`
  query GetRealtimePriceUpdates {
    realtimePrice {
      usdCentsPerBtc
    }
  }
`

const RealtimePriceUpdates = () => {
  useGetRealtimePriceUpdatesQuery({
    fetchPolicy: "network-only",
    pollInterval: 5000,
  })

  return <></>
}

export { RealtimePriceUpdates }
