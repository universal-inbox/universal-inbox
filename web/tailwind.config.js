/** @type {import('tailwindcss').Config} */
const colors = require('tailwindcss/colors')

module.exports = {
  content: ["index.html", "./src/**/*.rs", "./src/*.rs"],
  darkMode: "class",
  plugins: [
    require('@tailwindcss/forms'),
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
    themes: ["light", "dark"],
    darkTheme: "dark",
  },
};
