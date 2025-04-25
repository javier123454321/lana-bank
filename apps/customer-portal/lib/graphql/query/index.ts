import { DocumentNode, TypedDocumentNode } from "@apollo/client/core"
import { OperationVariables } from "@apollo/client"

import { getClient } from "@/lib/apollo-client/rsc"

type QueryDocument<TData, TVariables> =
  | DocumentNode
  | TypedDocumentNode<TData, TVariables>

export async function executeQuery<
  TData,
  TVariables extends OperationVariables = OperationVariables,
>(options: {
  document: QueryDocument<TData, TVariables>
  variables: TVariables
}): Promise<TData | Error> {
  try {
    const client = await getClient()
    const response = await client.query<TData, TVariables>({
      query: options.document,
      variables: options.variables,
    })

    if (response.error) {
      throw new Error(response.error.message)
    }

    if (response.errors && response.errors.length > 0) {
      throw new Error(response.errors[0].message)
    }

    if (!response.data) {
      throw new Error("No data found")
    }

    return response.data
  } catch (error) {
    console.log("Error on apollo client => ", error)
    if (error instanceof Error) {
      console.error(
        `Query ${options.document.definitions} failed with error: ${error.message}`,
      )
      return error
    }
    console.error(
      `Query ${options.document.definitions} failed with error: Unknown error`,
    )
    return new Error("An unknown error occurred")
  }
}
