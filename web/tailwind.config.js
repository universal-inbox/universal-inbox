/** @type {import('tailwindcss').Config} */
const colors = require('tailwindcss/colors')

module.exports = {
  content: ["index.html", "./src/**/*.rs", "./src/*.rs"],
  darkMode: "class",
  plugins: [
    require('@tailwindcss/forms'),
    require('@tailwindcss/typography'),
    require("daisyui")
  ],

  daisyui: {
    styled: true,
    themes: true,
    base: true,
    utils: true,
    logs: true,
    rtl: false,
    prefix: "",
    themes: [
      {
        "uilight": {
          "primary": "#388FEF",
          "secondary": "#f000b8",
          "accent": "#1dcdbc",
          "neutral": "#2b3440",
          "base-100": "#ffffff",
          "info": "#3abff8",
          "success": "#36d399",
          "warning": "#fbbd23",
          "error": "#f87272",
        }
      },
      {
        "uidark": {
          "primary": "#388FEF",
          "secondary": "#D926AA",
          "accent": "#1FB2A5",
          "neutral": "#191D24",
          "base-100": "#040e29",
          "base-200": "#0C2875",
          "base-300": "#2F4275",
          "base-content": "#FFFFFF",
          "info": "#3ABFF8",
          "success": "#36D399",
          "warning": "#FBBD23",
          "error": "#F87272",
        }
      }
    ],
    darkTheme: "uidark",
  },
};
