/** @type {import('tailwindcss').Config} */
const colors = require('tailwindcss/colors')

module.exports = {
  content: ["index.html", "./src/**/*.rs", "./src/*.rs"],
  darkMode: "class",
  theme: {
    colors: {
      transparent: 'transparent',
      current: 'currentColor',
      black: colors.black,
      white: colors.white,
      slate: colors.slate,
      red: colors.red,
      blue: colors.blue,
      gray: colors.gray,
      light: {
        0: "#FFFFFF",
        100: "#F5F8FB",
        200: "#EAEFF3",
        300: "#DDF3FD",
        400: "#D2D5DF",
        500: "#1967DF",
      },
      dark: {
        0: "#000000",
        100: "#1A2329",
        200: "#21262E",
        300: "#2D313A",
        400: "#373D47",
        500: "#2A3F53",
        600: "#336CCB",
        700: "#58A6FE",
        800: "#D2D5DF"
      }
    },
    extend: {},
  },
  plugins: [
    require('@tailwindcss/forms')
  ]
};
