/**
 * @vitest-environment jsdom
 */
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, test, vi } from "vitest";

vi.mock("@/state/store", () => ({
  useStore: (selector: (state: { functionalColorMode: "light" | "dark" }) => unknown) =>
    selector({ functionalColorMode: "light" }),
}));

vi.mock("@tauri-apps/plugin-shell", () => ({
  open: vi.fn(),
}));

import Markdown, { normalizeMarkdownCodeLanguage } from "./Markdown";

describe("Markdown", () => {
  test("normalizes common fenced-code language aliases", () => {
    expect(normalizeMarkdownCodeLanguage("ts")).toBe("typescript");
    expect(normalizeMarkdownCodeLanguage("sh")).toBe("bash");
    expect(normalizeMarkdownCodeLanguage("yml")).toBe("yaml");
    expect(normalizeMarkdownCodeLanguage("")).toBe("");
  });

  test("renders fenced code blocks with syntax token markup", () => {
    const markup = renderToStaticMarkup(
      <Markdown content={"```ts\nconst answer: number = 42;\n```"} />,
    );

    expect(markup).toContain("markdown-code-block");
    expect(markup).toContain("token keyword");
    expect(markup).toContain("token operator");
  });

  test("preserves non-code GFM structures when rendering to React elements", () => {
    const markup = renderToStaticMarkup(
      <Markdown
        content={[
          "| Left | Right |",
          "| --- | ---: |",
          "| a | 1 |",
          "",
          "- [x] done",
          "",
          "Footnote ref[^1]",
          "",
          "[^1]: note",
        ].join("\n")}
      />,
    );

    expect(markup).toContain("<table>");
    expect(markup).toContain('align="right"');
    expect(markup).toContain('type="checkbox"');
    expect(markup).toContain('data-footnotes=""');
    expect(markup).toContain('data-footnote-ref=""');
  });
});
