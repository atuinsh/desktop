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

vi.mock("@/lib/hooks/useBlockLocalState", () => ({
  useBlockLocalState: vi.fn(() => [false, vi.fn()]),
}));

vi.mock("@/tracking", () => ({
  default: vi.fn(),
}));

import { insertMarkdownRender, createLinkClickHandler } from "./index";

describe("MarkdownRender", () => {
  describe("createLinkClickHandler", () => {
    test("calls openFn with href when clicking an anchor", () => {
      const mockOpenFn = vi.fn();
      const handler = createLinkClickHandler(mockOpenFn);

      // Create a mock anchor element
      const anchor = document.createElement("a");
      anchor.href = "https://example.com";
      document.body.appendChild(anchor);

      const mockEvent = {
        target: anchor,
        preventDefault: vi.fn(),
      } as unknown as React.MouseEvent<HTMLDivElement>;

      handler(mockEvent);

      expect(mockEvent.preventDefault).toHaveBeenCalled();
      expect(mockOpenFn).toHaveBeenCalledWith("https://example.com");

      document.body.removeChild(anchor);
    });

    test("calls openFn when clicking element inside anchor", () => {
      const mockOpenFn = vi.fn();
      const handler = createLinkClickHandler(mockOpenFn);

      // Create anchor with nested span
      const anchor = document.createElement("a");
      anchor.href = "https://nested.example.com";
      const span = document.createElement("span");
      anchor.appendChild(span);
      document.body.appendChild(anchor);

      const mockEvent = {
        target: span,
        preventDefault: vi.fn(),
      } as unknown as React.MouseEvent<HTMLDivElement>;

      handler(mockEvent);

      expect(mockEvent.preventDefault).toHaveBeenCalled();
      expect(mockOpenFn).toHaveBeenCalledWith("https://nested.example.com");

      document.body.removeChild(anchor);
    });

    test("does nothing when clicking non-anchor element", () => {
      const mockOpenFn = vi.fn();
      const handler = createLinkClickHandler(mockOpenFn);

      const div = document.createElement("div");
      document.body.appendChild(div);

      const mockEvent = {
        target: div,
        preventDefault: vi.fn(),
      } as unknown as React.MouseEvent<HTMLDivElement>;

      handler(mockEvent);

      expect(mockEvent.preventDefault).not.toHaveBeenCalled();
      expect(mockOpenFn).not.toHaveBeenCalled();

      document.body.removeChild(div);
    });

    test("does nothing when anchor has no href", () => {
      const mockOpenFn = vi.fn();
      const handler = createLinkClickHandler(mockOpenFn);

      const anchor = document.createElement("a");
      // No href set
      document.body.appendChild(anchor);

      const mockEvent = {
        target: anchor,
        preventDefault: vi.fn(),
      } as unknown as React.MouseEvent<HTMLDivElement>;

      handler(mockEvent);

      expect(mockEvent.preventDefault).toHaveBeenCalled();
      expect(mockOpenFn).not.toHaveBeenCalled();

      document.body.removeChild(anchor);
    });
  });
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
