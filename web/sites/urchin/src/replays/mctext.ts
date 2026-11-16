import type { ChatSegment } from "../viewer/pkg/replay_viewer.js";
import { h } from "./dom";

const PICTOGRAPHS = /[\p{Extended_Pictographic}\u{FE0F}\u{200D}]/gu;

export const withoutPictographs = (text: string) =>
  text.replace(PICTOGRAPHS, "");

export const coloured = (text: string, color: string) => {
  const span = h("span", { textContent: text });
  span.style.color = color;
  return span;
};

export const mcText = (segments: ChatSegment[]) =>
  segments.map(({ text, color, bold, italic, underlined, strikethrough }) => {
    const span = coloured(withoutPictographs(text), color);
    Object.assign(span.style, {
      fontWeight: bold ? "700" : "",
      fontStyle: italic ? "italic" : "",
      textDecorationLine: [
        underlined && "underline",
        strikethrough && "line-through",
      ]
        .filter(Boolean)
        .join(" "),
    });
    return span;
  });
