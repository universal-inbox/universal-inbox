const { config } = require("@swc/core/spack");

module.exports = config({
  entry: {
    web: __dirname + "/js/nango.js",
  },
  output: {
    path: "/tmp/spack",
    name: "nango.js",
  },
  module: {},
});
