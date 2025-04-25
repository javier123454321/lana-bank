import { redirect } from "next/navigation"

import { AuthTemplateCard } from "@/components/auth/auth-template-card"
import { OtpForm, OtpParams } from "@/components/auth/otp-form"

async function Otp({ searchParams }: { searchParams?: Promise<OtpParams> }) {
  if (!searchParams) redirect("/")

  const params = await searchParams
  const { flowId, type } = params
  if (!flowId || !type || !["login", "register"].includes(type)) redirect("/")

  return (
    <AuthTemplateCard>
      <OtpForm flowId={flowId} type={type} />
    </AuthTemplateCard>
  )
}

export default Otp
