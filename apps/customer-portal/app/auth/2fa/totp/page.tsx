import { redirect } from "next/navigation"

import { AuthTemplateCard } from "@/components/auth/auth-template-card"
import { TotpForm } from "@/components/auth/totp-form"

async function TwoFactorAuthWithTotpPage({
  searchParams,
}: {
  searchParams: Promise<{
    flowId?: string
  }>
}) {
  const { flowId } = await searchParams
  if (!flowId) redirect("/auth")

  return (
    <AuthTemplateCard>
      <TotpForm flowId={flowId} />
    </AuthTemplateCard>
  )
}
export default TwoFactorAuthWithTotpPage
