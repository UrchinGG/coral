import { el } from "@/lib/dom";
import {
  highlightExpression,
  highlightTemplate,
  type ExprSegment,
} from "@/lib/expr";

type Mode = "template" | "condition";

export interface Editor {
  element: HTMLElement;
  area: HTMLTextAreaElement;
}

export function highlighted(text: string, mode: Mode = "condition") {
  return el("code", { className: "expr" }, ...segments(text, mode));
}

export function expressionEditor(options: {
  label: string;
  value: string;
  mode: Mode;
  placeholder?: string;
  suggestions: string[];
  onInput: (value: string) => void;
}): Editor {
  const shadow = el("pre", { className: "expr-shadow", ariaHidden: "true" });
  const area = el("textarea", {
    className: "expr-input",
    ariaLabel: options.label,
    spellcheck: false,
    placeholder: options.placeholder ?? "",
    value: options.value,
  });
  const paint = () => {
    shadow.replaceChildren(...segments(area.value, options.mode), "\n");
    options.onInput(area.value);
  };
  area.addEventListener("input", paint);
  area.addEventListener("scroll", () => (shadow.scrollTop = area.scrollTop));
  const insert = (text: string) => {
    const { selectionStart, selectionEnd } = area;
    area.setRangeText(text, selectionStart, selectionEnd, "end");
    area.focus();
    paint();
  };
  const fields = options.suggestions.map((path) => {
    const chip = el("button", {
      type: "button",
      className: "chip mono",
      textContent: path,
    });
    chip.addEventListener("click", () => insert(path));
    return chip;
  });
  shadow.replaceChildren(...segments(area.value, options.mode), "\n");
  const element = el(
    "div",
    { className: "expr-editor" },
    el("div", { className: "expr-box" }, shadow, area),
    ...(fields.length
      ? [
          el(
            "div",
            { className: "chips" },
            el("span", { className: "muted" }, "Insert field:"),
            ...fields,
          ),
        ]
      : []),
  );
  return { element, area };
}

function segments(text: string, mode: Mode) {
  const parts: ExprSegment[] =
    mode === "template" ? highlightTemplate(text) : highlightExpression(text);
  return parts.map((part) =>
    el("span", { className: `expr-${part.kind}`, textContent: part.text }),
  );
}
