import type { Config } from "tailwindcss";

export default {
  content: [
    "./src/pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/components/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        canvas: "#050505",
        card: "#111113",
        elevated: "#1a1a1e",
        soft: "#0a0a0b",
        ink: "#f4f4f5",
        body: "#a1a1aa",
        "body-strong": "#e4e4e7",
        muted: "#85858f",
        hairline: "#3f3f46",
        cyan: "#22d3ee",
        signal: "#34d399",
        danger: "#f87171",
        warning: "#fbbf24",
      },
      fontFamily: {
        display: [
          "IBM Plex Sans",
          "ui-sans-serif",
          "system-ui",
          "sans-serif",
        ],
        mono: [
          "IBM Plex Mono",
          "ui-monospace",
          "monospace",
        ],
      },
      letterSpacing: {
        label: "0.12em",
      },
      boxShadow: {
        none: "none",
      },
    },
  },
  plugins: [],
} satisfies Config;
