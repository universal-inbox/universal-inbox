module.exports = {
  entry: {
    web: __dirname + "/js/index.js",
  },
  experiments: {
    outputModule: true,
  },
  output: {
    path: "/tmp/rspack",
    filename: "index.js",
    library: {
      type: "module",
    },
  },
  mode: "production",
  module: {},
};
