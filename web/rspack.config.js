const path = require("path");

module.exports = {
  entry: {
    web: __dirname + "/js/index.js",
  },
  experiments: {
    outputModule: true,
  },
  output: {
    path: path.resolve(__dirname, "public/js"),
    filename: "index.js",
    library: {
      type: "module",
    },
  },
  mode: "production",
  module: {},
};
