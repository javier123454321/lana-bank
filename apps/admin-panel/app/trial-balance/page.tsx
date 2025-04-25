"use client"
import React, { useState } from "react"
import { gql } from "@apollo/client"

import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@lana/web/ui/table"

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@lana/web/ui/card"

import { Skeleton } from "@lana/web/ui/skeleton"

import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import {
  TrialBalanceCurrencySelection,
  TrialBalanceLayerSelection,
  TrialBalanceLayers,
} from "./trial-balance-currency-selector"

import { GetTrialBalanceQuery, useGetTrialBalanceQuery } from "@/lib/graphql/generated"
import Balance, { Currency } from "@/components/balance/balance"
import {
  DateRange,
  DateRangeSelector,
  getInitialDateRange,
} from "@/components/date-range-picker"
import PaginatedTable, { Column, PaginatedData } from "@/components/paginated-table"

const DEFAULT_PAGESIZE = 15

gql`
  query GetTrialBalance(
    $from: Timestamp!
    $until: Timestamp!
    $first: Int!
    $after: String
  ) {
    trialBalance(from: $from, until: $until) {
      name
      total {
        usd {
          ...UsdLedgerBalanceRangeFragment
        }
        btc {
          ...BtcLedgerBalanceRangeFragment
        }
      }
      accounts(first: $first, after: $after) {
        edges {
          cursor
          node {
            id
            code
            name
            balanceRange {
              __typename
              ...UsdLedgerBalanceRangeFragment
              ...BtcLedgerBalanceRangeFragment
            }
            children {
              id
              code
              name
              balanceRange {
                __typename
                ...UsdLedgerBalanceRangeFragment
                ...BtcLedgerBalanceRangeFragment
              }
            }
          }
        }
        pageInfo {
          endCursor
          startCursor
          hasNextPage
          hasPreviousPage
        }
      }
    }
  }

  fragment UsdBalanceFragment on UsdLedgerAccountBalance {
    settled
    pending
    encumbrance
  }

  fragment BtcBalanceFragment on BtcLedgerAccountBalance {
    settled
    pending
    encumbrance
  }

  fragment UsdLedgerBalanceRangeFragment on UsdLedgerAccountBalanceRange {
    usdStart: start {
      ...UsdBalanceFragment
    }
    usdDiff: diff {
      ...UsdBalanceFragment
    }
    usdEnd: end {
      ...UsdBalanceFragment
    }
  }

  fragment BtcLedgerBalanceRangeFragment on BtcLedgerAccountBalanceRange {
    btcStart: start {
      ...BtcBalanceFragment
    }
    btcDiff: diff {
      ...BtcBalanceFragment
    }
    btcEnd: end {
      ...BtcBalanceFragment
    }
  }
`

const LoadingSkeleton = () => {
  return (
    <div className="space-y-4" data-testid="loading-skeleton">
      <div className="space-y-4">
        <Skeleton className="h-10 w-72" />
        <Skeleton className="h-10 w-96" />
      </div>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>
              <Skeleton className="h-5 w-32" />
            </TableHead>
            <TableHead>
              <Skeleton className="h-5 w-24 ml-auto" />
            </TableHead>
            <TableHead>
              <Skeleton className="h-5 w-24 ml-auto" />
            </TableHead>
            <TableHead>
              <Skeleton className="h-5 w-24 ml-auto" />
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {[1, 2, 3, 4, 5].map((i) => (
            <TableRow key={i}>
              <TableCell>
                <Skeleton className="h-5 w-full" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-24 ml-auto" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-24 ml-auto" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-24 ml-auto" />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

type TrialBalanceAccountNode = NonNullable<
  NonNullable<GetTrialBalanceQuery["trialBalance"]>["accounts"]
>["edges"][0]["node"]

type BalanceRange = NonNullable<TrialBalanceAccountNode["balanceRange"]>

function isUsdLedgerBalanceRange(balanceRange: BalanceRange | null | undefined) {
  return !!balanceRange && balanceRange.__typename === "UsdLedgerAccountBalanceRange"
}

function isBtcLedgerBalanceRange(balanceRange: BalanceRange | null | undefined) {
  return !!balanceRange && balanceRange.__typename === "BtcLedgerAccountBalanceRange"
}

const hasNonZeroBalanceRangeData = (balanceRange: BalanceRange) => {
  if (
    balanceRange.__typename === "UsdLedgerAccountBalanceRange" &&
    (balanceRange.usdStart.pending ||
      balanceRange.usdStart.settled ||
      balanceRange.usdStart.encumbrance ||
      balanceRange.usdDiff.pending ||
      balanceRange.usdDiff.settled ||
      balanceRange.usdDiff.encumbrance ||
      balanceRange.usdEnd.pending ||
      balanceRange.usdEnd.settled ||
      balanceRange.usdEnd.encumbrance)
  )
    return true
  if (
    balanceRange.__typename === "BtcLedgerAccountBalanceRange" &&
    (balanceRange.btcStart.pending ||
      balanceRange.btcStart.settled ||
      balanceRange.btcStart.encumbrance ||
      balanceRange.btcDiff.pending ||
      balanceRange.btcDiff.settled ||
      balanceRange.btcDiff.encumbrance ||
      balanceRange.btcEnd.pending ||
      balanceRange.btcEnd.settled ||
      balanceRange.btcEnd.encumbrance)
  )
    return true
  return false
}

function TrialBalancePage() {
  const t = useTranslations("TrialBalance")
  const [dateRange, setDateRange] = useState<DateRange>(getInitialDateRange())

  const { data, loading, error, fetchMore } = useGetTrialBalanceQuery({
    variables: {
      from: dateRange.from,
      until: dateRange.until,
      first: DEFAULT_PAGESIZE,
    },
  })

  const [currency, setCurrency] = React.useState<Currency>("usd")
  const [layer, setLayer] = React.useState<TrialBalanceLayers>("settled")

  const router = useRouter()

  const total = data?.trialBalance.total
  const accounts = data?.trialBalance.accounts

  if (error) return <div className="text-destructive">{error.message}</div>
  if (loading && !data) {
    return <LoadingSkeleton />
  }
  if (!total) return <div>{t("noAccountsPresent")}</div>

  const columns: Column<TrialBalanceAccountNode>[] = [
    {
      key: "code",
      label: t("table.headers.accountCode"),
      render: (code: string) => (
        <div className="font-mono text-xs text-gray-500">{code}</div>
      ),
    },
    {
      key: "name",
      label: t("table.headers.accountName"),
    },
    {
      key: "balanceRange",
      label: t("table.headers.openingBalance"),
      labelClassName: "!justify-end",
      render: (_, node) => {
        if (currency === "usd" && isUsdLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.usdStart[layer]}
            />
          )
        } else if (currency === "btc" && isBtcLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.btcStart[layer]}
            />
          )
        }
        return null
      },
    },
    {
      key: "balanceRange",
      label: t("table.headers.periodActivity"),
      labelClassName: "!justify-end",
      render: (_, node) => {
        if (currency === "usd" && isUsdLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.usdDiff[layer]}
            />
          )
        } else if (currency === "btc" && isBtcLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.btcDiff[layer]}
            />
          )
        }
        return null
      },
    },
    {
      key: "balanceRange",
      label: t("table.headers.closingBalance"),
      labelClassName: "!justify-end",
      render: (_, node) => {
        if (currency === "usd" && isUsdLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.usdEnd[layer]}
            />
          )
        } else if (currency === "btc" && isBtcLedgerBalanceRange(node.balanceRange)) {
          return (
            <Balance
              align="end"
              currency={currency}
              amount={node.balanceRange.btcEnd[layer]}
            />
          )
        }
        return null
      },
    },
  ]

  const Footer = (
    <TableFooter className="border-t-4">
      <TableRow>
        <TableCell className="font-bold">{t("totals")}</TableCell>
        <TableCell />
        <TableCell className="w-48">
          <Balance
            align="end"
            currency={currency}
            amount={
              currency === "usd" ? total.usd.usdStart[layer] : total.btc.btcStart[layer]
            }
          />
        </TableCell>
        <TableCell className="w-48">
          <Balance
            align="end"
            currency={currency}
            amount={
              currency === "usd" ? total.usd.usdDiff[layer] : total.btc.btcDiff[layer]
            }
          />
        </TableCell>
        <TableCell className="w-48">
          <Balance
            align="end"
            currency={currency}
            amount={
              currency === "usd" ? total.usd.usdEnd[layer] : total.btc.btcEnd[layer]
            }
          />
        </TableCell>
      </TableRow>
    </TableFooter>
  )

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("title")}</CardTitle>
        <CardDescription>{t("description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex gap-6 items-center">
          <div>{t("dateRange")}:</div>
          <DateRangeSelector initialDateRange={dateRange} onDateChange={setDateRange} />
        </div>
        <div>
          <TrialBalanceCurrencySelection currency={currency} setCurrency={setCurrency} />
          <TrialBalanceLayerSelection layer={layer} setLayer={setLayer} />
        </div>
        <PaginatedTable<TrialBalanceAccountNode>
          columns={columns}
          data={accounts as PaginatedData<TrialBalanceAccountNode>}
          loading={loading}
          pageSize={DEFAULT_PAGESIZE}
          fetchMore={async (cursor) =>
            fetchMore({
              variables: {
                after: cursor,
                from: dateRange.from,
                until: dateRange.until,
                first: DEFAULT_PAGESIZE,
              },
            })
          }
          customFooter={Footer}
          style="compact"
          onClick={(account) => router.push(`/ledger-account/${account.code}`)}
          noDataText={t("noAccountsPresent")}
          subRows={(account) =>
            account.children.filter((child) =>
              hasNonZeroBalanceRangeData(child.balanceRange),
            ) as TrialBalanceAccountNode[]
          }
        />
        <div className="mt-4" />
      </CardContent>
    </Card>
  )
}

export default TrialBalancePage
