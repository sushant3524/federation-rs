let denoFsPlugin = {
  name: "fs",
  setup(build) {
    // Intercept require("fs") and replace it with shims
    build.onResolve({ filter: /^fs$/ }, (args) => ({
      path: args.path,
      namespace: "deno-fs",
    }));

    build.onLoad({ filter: /.*/, namespace: "deno-fs" }, () => ({
      contents: `
        module.exports = {
          existsSync: Deno.ensureFileSync,
          writeFileSync: Deno.writeTextFile,
          readFileSync: (...args) => {
            return Buffer.from(Deno.core.ops.op_read_file(...args));
          }
        }
      `,
      loader: "js",
    }));
  },
};

let denoUtilPlugin = {
  name: "util",
  setup(build) {
    build.onResolve({ filter: /^util$/ }, (args) => ({
      path: args.path,
      namespace: "deno-util",
    }));

    build.onLoad({ filter: /.*/, namespace: "deno-util" }, () => ({
      contents: `module.exports = {
        TextDecoder: globalThis.TextDecoder,
        TextEncoder: globalThis.TextEncoder,
      }`,
      loader: "js",
    }));
  },
};

// all paths are relative to package.json when run with `npm run build`
require("esbuild")
  .build({
    entryPoints: ["./js-dist/index.js"],
    bundle: true,
    minify: true,
    sourcemap: true,
    target: "es2020",
    globalName: "composition_bridge",
    outfile: "./bundled/composition_bridge.js",
    format: "iife",
    plugins: [denoFsPlugin, denoUtilPlugin],
    define: { Buffer: "buffer_shim" },
    inject: ["./esbuild/buffer_shim.js", "./esbuild/url_shim.js"],
  })
  .catch(() => process.exit(1));
