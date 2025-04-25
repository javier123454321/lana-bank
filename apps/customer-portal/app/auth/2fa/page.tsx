import { redirect } from "next/navigation"

import Link from "next/link"

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@lana/web/ui/card"
import { Button } from "@lana/web/ui/button"

import { AuthTemplateCard } from "@/components/auth/auth-template-card"

async function TwoFactorAuthPage({
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
      <Card className="md:w-2/5 my-2">
        <CardHeader>
          <CardTitle>Continue with two-factor authentication.</CardTitle>
          <CardDescription className="text-textColor-secondary">
            Select Method to Continue your two-factor authentication.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2 w-full">
          <Link href={`/auth/2fa/webauth?flowId=${flowId}`}>
            <Button className="align-middle w-30 items-center w-full">
              Continue with Passkey
            </Button>
          </Link>
          <Link href={`/auth/2fa/totp?flowId=${flowId}`}>
            <Button className="align-middle w-30 items-center min-h-max w-full">
              Continue with Authenticator
            </Button>
          </Link>
        </CardContent>
      </Card>
    </AuthTemplateCard>
  )
}
export default TwoFactorAuthPage
