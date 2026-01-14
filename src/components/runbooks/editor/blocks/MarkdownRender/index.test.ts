/**
 * @vitest-environment jsdom
 */
import { describe, expect, test, vi } from "vitest";

// Mock dependencies that require browser environment
vi.mock("@blocknote/react", () => ({
  createReactBlockSpec: vi.fn(() => ({})),
}));

vi.mock("@/lib/hooks/useDocumentBridge", () => ({
  useBlockContext: vi.fn(() => ({ variables: {} })),
}));

vi.mock("@/lib/hooks/useKvValue", () => ({
  useBlockKvValue: vi.fn(() => [false, vi.fn()]),
}));

vi.mock("@/tracking", () => ({
  default: vi.fn(),
}));

import { insertMarkdownRender } from "./index";

describe("MarkdownRender", () => {
  describe("insertMarkdownRender", () => {
    test("returns correct menu item structure", () => {
      const mockEditor = {
        insertBlocks: vi.fn(),
        getTextCursorPosition: () => ({ block: { id: "test-id" } }),
      };

      const menuItem = insertMarkdownRender(mockEditor);

      expect(menuItem.title).toBe("Markdown Render");
      expect(menuItem.subtext).toBe("Render markdown content from a variable");
      expect(menuItem.group).toBe("Content");
      expect(menuItem.aliases).toEqual(["markdown", "md", "render", "display"]);
    });

    test("onItemClick inserts block with correct props", () => {
      const mockEditor = {
        insertBlocks: vi.fn(),
        getTextCursorPosition: () => ({ block: { id: "test-block-id" } }),
      };

      const menuItem = insertMarkdownRender(mockEditor);
      menuItem.onItemClick();

      expect(mockEditor.insertBlocks).toHaveBeenCalledWith(
        [{ type: "markdown_render", props: { variableName: "", maxLines: 12 } }],
        "test-block-id",
        "before",
      );
    });
  });
});
