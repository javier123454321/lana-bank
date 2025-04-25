import { redirect } from "next/navigation"

import { cookies } from "next/headers"

import { LoginFlow, UiNodeInputAttributes } from "@ory/client"

import { AuthTemplateCard } from "@/components/auth/auth-template-card"
import { PasskeySignIn } from "@/components/auth/webauth"
import { SerializedPublicKeyCredentialRequestOptions } from "@/lib/webauth"
import { kratosPublic } from "@/lib/kratos/sdk"
import {
  emailParserFromUiNodeLogin,
  getCsrfCookiesAsString,
  getCsrfToken,
} from "@/lib/kratos/utils"

async function TwoFactorAuthWithWebAuthPage({
  searchParams,
}: {
  searchParams: Promise<{
    flowId?: string
  }>
}) {
  const { flowId } = await searchParams
  const cookiesObj = await cookies()
  const allCookies = cookiesObj.getAll()
  let signInFlow: LoginFlow

  if (!flowId) redirect("/auth")

  try {
    signInFlow = (
      await kratosPublic().getLoginFlow({
        id: flowId,
        cookie: getCsrfCookiesAsString(allCookies),
      })
    ).data
  } catch (error) {
    redirect("/auth")
  }

  const publicKeyNode = signInFlow.ui?.nodes?.find(
    (node) =>
      node.attributes.node_type === "input" &&
      node.attributes.name === "webauthn_login_trigger",
  )

  const csrfToken = getCsrfToken(signInFlow)
  const email = emailParserFromUiNodeLogin(signInFlow.ui.nodes)

  if (!email || !csrfToken || !publicKeyNode) redirect("/auth")

  const publicKey = (
    JSON.parse(
      ((publicKeyNode.attributes as UiNodeInputAttributes).onclick as string).slice(
        26,
        -1,
      ),
    ) as {
      publicKey: SerializedPublicKeyCredentialRequestOptions
    }
  ).publicKey

  return (
    <AuthTemplateCard>
      <PasskeySignIn
        csrfToken={csrfToken}
        email={email}
        flowId={flowId}
        publicKey={publicKey}
      />
    </AuthTemplateCard>
  )
}
export default TwoFactorAuthWithWebAuthPage
