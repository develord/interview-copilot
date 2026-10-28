import { describe, expect, it } from "vitest";
import { parseRichText, splitInlineCode } from "./richText";

describe("parseRichText", () => {
  it("returns a single text block for plain prose", () => {
    expect(parseRichText("Hello world")).toEqual([
      { kind: "text", parts: [{ code: false, text: "Hello world" }] },
    ]);
  });

  it("splits prose and fenced code blocks, keeping the language", () => {
    const blocks = parseRichText("Approach:\n```ts\nconst a = 1;\n```\nO(1).");
    expect(blocks).toEqual([
      { kind: "text", parts: [{ code: false, text: "Approach:" }] },
      { kind: "code", lang: "ts", code: "const a = 1;", closed: true },
      { kind: "text", parts: [{ code: false, text: "O(1)." }] },
    ]);
  });

  it("marks an unclosed fence as an open code block while streaming", () => {
    const blocks = parseRichText("Code:\n```js\nfunction f() {");
    expect(blocks.at(-1)).toEqual({
      kind: "code",
      lang: "js",
      code: "function f() {",
      closed: false,
    });
  });

  it("keeps HTML-looking content as literal text", () => {
    const blocks = parseRichText("<img src=x onerror=alert(1)>");
    expect(blocks).toEqual([
      { kind: "text", parts: [{ code: false, text: "<img src=x onerror=alert(1)>" }] },
    ]);
  });

  it("returns no blocks for empty or whitespace input", () => {
    expect(parseRichText("")).toEqual([]);
    expect(parseRichText("  \n ")).toEqual([]);
  });
});

describe("splitInlineCode", () => {
  it("extracts inline code spans", () => {
    expect(splitInlineCode("Use `Promise.all` or `Promise.allSettled`.")).toEqual([
      { code: false, text: "Use " },
      { code: true, text: "Promise.all" },
      { code: false, text: " or " },
      { code: true, text: "Promise.allSettled" },
      { code: false, text: "." },
    ]);
  });

  it("leaves an unmatched backtick as text (partial stream)", () => {
    expect(splitInlineCode("Use `Promise")).toEqual([{ code: false, text: "Use `Promise" }]);
  });
});
