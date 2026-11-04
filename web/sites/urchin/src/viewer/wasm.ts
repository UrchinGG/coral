export const [viewerWasm] = Object.values(
  import.meta.glob<string>("./pkg/*.wasm", {
    eager: true,
    query: "?url",
    import: "default",
  }),
);
