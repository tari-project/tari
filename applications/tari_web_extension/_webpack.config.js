const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");
const webpack = require("webpack");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const KEY_MANAGER_PATH = "../../base_layer/key_manager/";

module.exports = {
  entry: "./rust/index.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "index.js",
  },
  plugins: [
    new HtmlWebpackPlugin(),
    new WasmPackPlugin({
      crateDirectory: path.resolve(__dirname, KEY_MANAGER_PATH),
      outDir: path.resolve(__dirname, `${KEY_MANAGER_PATH}/pkg`), // https://github.com/wasm-tool/wasm-pack-plugin/issues/93
    }),
    // Have this example work in Edge which doesn't ship `TextEncoder` or
    // `TextDecoder` at this time.
    new webpack.ProvidePlugin({
      TextDecoder: ["text-encoding", "TextDecoder"],
      TextEncoder: ["text-encoding", "TextEncoder"],
    }),
  ],
  mode: "development",
  experiments: {
    asyncWebAssembly: true,
  },
};
