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
        canvas: "#000000",
        card: "#1a1a1a",
        elevated: "#262626",
        soft: "#0d0d0d",
        ink: "#ffffff",
        body: "#bbbbbb",
        "body-strong": "#e6e6e6",
        muted: "#7e7e7e",
        hairline: "#3c3c3c",
        "m-blue-light": "#0066b1",
        "m-blue-dark": "#1c69d4",
        "m-red": "#e22718",
        success: "#0fa336",
        warning: "#f4b400",
      },
      fontFamily: {
        display: ["var(--font-geist-sans)", "Inter", "system-ui", "sans-serif"],
        mono: ["var(--font-geist-mono)", "ui-monospace", "monospace"],
      },
      letterSpacing: {
        label: "0.1em",
      },
    },
  },
  plugins: [],
} satisfies Config;
