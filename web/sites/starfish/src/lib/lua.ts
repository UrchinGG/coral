const BUILTIN_TYPES = new Set([
  "any",
  "boolean",
  "function",
  "integer",
  "nil",
  "number",
  "string",
  "table",
]);

// Reference signatures read `name(a, b?) -> Type`; shown as LuaLS
// annotations they are real Lua, so they highlight exactly as in an editor.
// Required parameters need no annotation: the function line already names them.
export const luaSignature = (signature: string) => {
  const [head, returns = ""] = signature.split(" -> ");
  const call = head.match(/^([\w.:]+)\((.*)\)$/);
  if (!call) return head;

  const [, name, list] = call;
  const params = list ? list.split(", ") : [];
  return [
    ...params
      .filter((param) => param.endsWith("?"))
      .map((param) => `---@param ${param} any`),
    ...returnAnnotations(returns),
    `function ${name}(${params.map((param) => param.replace(/\?$/, "")).join(", ")}) end`,
  ].join("\n");
};

const returnAnnotations = (returns: string) => {
  if (returns === "" || returns === "nil") return [];
  if (returns === "...") return ["---@return any ..."];
  const tuple = returns.match(/^\((.*)\)$/);
  if (tuple) {
    return tuple[1].split(", ").map((value) => `---@return any ${value}`);
  }
  if (/^[a-z]\w*$/.test(returns) && !BUILTIN_TYPES.has(returns)) {
    return [`---@return any ${returns}`];
  }
  return [`---@return ${luaType(returns)}`];
};

const luaType = (type: string) =>
  type
    .replace(/\{([^}]*)\}/g, (_, fields: string) => {
      const typed = fields
        .split(", ")
        .map((field) =>
          field.endsWith("?") ? `${field.slice(0, -1)}?: any` : `${field}: any`,
        );
      return `{ ${typed.join(", ")} }`;
    })
    .replaceAll(" | ", "|");
