import { getRequestConfig } from "next-intl/server"
import { cookies, headers } from "next/headers"

const locales = ["en", "es"]

export default getRequestConfig(async () => {
  const cookieStore = await cookies()
  const localeCookie = cookieStore.get("NEXT_LOCALE")

  if (localeCookie?.value) {
    const locale = localeCookie.value
    return {
      locale,
      messages: (await import(`../messages/${locale}.json`)).default,
    }
  }

  const headersList = await headers()
  const acceptLanguage = headersList.get("accept-language") || ""

  const userLocales = acceptLanguage
    .split(",")
    .map((item) => {
      const [locale, priority] = item.trim().split(";q=")
      return {
        locale: locale.split("-")[0],
        priority: priority ? parseFloat(priority) : 1.0,
      }
    })
    .sort((a, b) => b.priority - a.priority)
    .map((item) => item.locale)

  const detectedLocale = userLocales.find((locale) => locales.includes(locale)) || "en"
  return {
    locale: detectedLocale,
    messages: (await import(`../messages/${detectedLocale}.json`)).default,
  }
})
