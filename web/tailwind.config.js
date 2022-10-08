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
      light: {
        0: "#FFFFFF",
        100: "#E6F3FF",
        200: "#CCE7FF",
        300: "#B3DBFF",
        400: "#99CFFF",
        500: "#80C4FF",
        600: "#66B8FF",
        700: "#4DACFF",
        800: "#33A0FF",
        900: "#1A94FF",
        1000: "#0088FF",
      },
      dark: {
        0: "#0088FF",
        100: "#007AE6",
        200: "#006DCC",
        300: "#005FB3",
        400: "#005299",
        500: "#004480",
        600: "#003666",
        700: "#00294C",
        800: "#001B33",
        900: "#000E19",
        1000: "#000000",
      },
      mix: {
        0: "#CCE7FF",
        100: "#99CFFF",
        200: "#66B8FF",
        300: "#33A0FF",
        400: "#0088FF",
        500: "#006DCC",
        600: "#005299",
        700: "#003666",
        800: "#001B33",
      },
    },
    extend: {},
  },
  plugins: []
};
