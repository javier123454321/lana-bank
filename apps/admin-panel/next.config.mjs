/** @type {import('next').NextConfig} */
import createNextIntlPlugin from "next-intl/plugin"

const withNextIntl = createNextIntlPlugin()

const nextConfig = {
  webpack: (config) => {
    config.module.rules.push({
      test: /\.svg$/i,
      use: ["@svgr/webpack"],
    })
    return config
  },
  transpilePackages: ["@lana/web"],
  output: "standalone",
  basePath: process.env.NEXT_PUBLIC_BASE_PATH,
  turbopack: {
    rules: {
      "**/*.svg": {
        loaders: ["@svgr/webpack"],
        as: "*.js",
      },
    },
  },
}

export default withNextIntl(nextConfig)
