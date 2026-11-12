export const bulletLines = (notes: string) =>
  notes
    .split(/\r?\n/)
    .map((line) => line.match(/^\s*[-*]\s+(.*)$/)?.[1].trim())
    .filter((line): line is string => Boolean(line));

const internal = /^(refactor|test|chore|style|docs|ci|build)\b/;
const commitType = /^\w+(\(.+?\))?!?:\s*/;

export const releaseLines = (notes: string) =>
  bulletLines(notes)
    .filter((line) => !internal.test(line))
    .map((line) => line.replace(commitType, ""))
    .map((line) => line.charAt(0).toUpperCase() + line.slice(1));
