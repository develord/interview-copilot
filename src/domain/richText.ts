/**
 * Minimal, safe structure for answer text: paragraphs with inline `code`, and fenced code blocks.
 * LLM output is never rendered as HTML; components render these blocks as plain text nodes.
 * Works on partial (streaming) input: an unclosed fence yields a code block with `closed: false`.
 */

export interface InlinePart {
  code: boolean;
  text: string;
}

export type RichBlock =
  | { kind: "text"; parts: InlinePart[] }
  | { kind: "code"; lang: string; code: string; closed: boolean };

const FENCE_OPEN = /^\s*```\s*([\w+#.-]*)\s*$/;
const FENCE_CLOSE = /^\s*```\s*$/;
const INLINE_CODE = /`([^`\n]+)`/g;

export function parseRichText(input: string): RichBlock[] {
  const blocks: RichBlock[] = [];
  let textLines: string[] = [];
  let code: { lang: string; lines: string[] } | null = null;

  const flushText = () => {
    const text = textLines.join("\n").replace(/^\n+|\n+$/g, "");
    if (text.trim() !== "") blocks.push({ kind: "text", parts: splitInlineCode(text) });
    textLines = [];
  };

  for (const line of input.split("\n")) {
    if (code) {
      if (FENCE_CLOSE.test(line)) {
        blocks.push({ kind: "code", lang: code.lang, code: code.lines.join("\n"), closed: true });
        code = null;
      } else {
        code.lines.push(line);
      }
      continue;
    }
    const open = FENCE_OPEN.exec(line);
    if (open) {
      flushText();
      code = { lang: open[1] ?? "", lines: [] };
    } else {
      textLines.push(line);
    }
  }

  if (code)
    blocks.push({ kind: "code", lang: code.lang, code: code.lines.join("\n"), closed: false });
  else flushText();
  return blocks;
}

export function splitInlineCode(text: string): InlinePart[] {
  const parts: InlinePart[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE_CODE)) {
    if (match.index > last) parts.push({ code: false, text: text.slice(last, match.index) });
    parts.push({ code: true, text: match[1]! });
    last = match.index + match[0].length;
  }
  if (last < text.length) parts.push({ code: false, text: text.slice(last) });
  return parts;
}
