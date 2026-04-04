import tailwindcss from "@tailwindcss/vite"
import { defineConfig, fontProviders } from "astro/config"

export default defineConfig({
  site: "https://brendon-codes.github.io",
  base: "/prawjector",
  output: "static",
  trailingSlash: "ignore",
  fonts: [
    {
      name: "IBM Plex Sans",
      cssVariable: "--font_body",
      provider: fontProviders.google(),
      weights: [400, 500, 600],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["sans-serif"],
    },
    {
      name: "IBM Plex Mono",
      cssVariable: "--font_mono",
      provider: fontProviders.google(),
      weights: [400, 500, 600],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["monospace"],
    },
    {
      name: "IBM Plex Sans Condensed",
      cssVariable: "--font_display",
      provider: fontProviders.google(),
      weights: [400, 500, 600],
      styles: ["normal"],
      subsets: ["latin"],
      fallbacks: ["sans-serif"],
    },
  ],
  vite: {
    plugins: [tailwindcss()],
  },
})
