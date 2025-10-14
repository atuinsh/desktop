import { expect, test, vi } from "vitest";
import { getCurrentDirectory, findAllParentsOfType, findFirstParentOfType } from "./exec";

// Mock the template and KV store modules
vi.mock("@/state/templates", () => ({
  templateString: vi.fn((_blockId: string, template: string) => Promise.resolve(template)),
}));

vi.mock("@/state/kv", () => ({
  KVStore: {
    open_default: vi.fn(() =>
      Promise.resolve({
        get: vi.fn((key: string) => {
          if (key === "block.local-dir-1.path") return Promise.resolve("/stored/path");
          if (key === "block.local-dir-2.path") return Promise.resolve("./relative/stored");
          return Promise.resolve(null);
        }),
      }),
    ),
  },
}));

test("findFirstParentOfType returns null when no parent found", () => {
  const editor = {
    document: [{ id: "target", type: "run" }],
  };

  const result = findFirstParentOfType(editor, "target", "directory");
  expect(result).toBeNull();
});

test("findFirstParentOfType returns last matching parent", () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/first" } },
      { id: "dir2", type: "directory", props: { path: "/second" } },
      { id: "target", type: "run" },
    ],
  };

  const result = findFirstParentOfType(editor, "target", "directory");
  expect(result.id).toBe("dir2");
});

test("findFirstParentOfType works with multiple types", () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/first" } },
      { id: "local1", type: "local-directory" },
      { id: "target", type: "run" },
    ],
  };

  const result = findFirstParentOfType(editor, "target", ["directory", "local-directory"]);
  expect(result.id).toBe("local1");
});

test("findAllParentsOfType returns all matching parents", () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/first" } },
      { id: "other", type: "text" },
      { id: "dir2", type: "directory", props: { path: "/second" } },
      { id: "target", type: "run" },
    ],
  };

  const result = findAllParentsOfType(editor, "target", "directory");
  expect(result).toHaveLength(2);
  expect(result[0].id).toBe("dir1");
  expect(result[1].id).toBe("dir2");
});

test("getCurrentDirectory returns ~ when no directory blocks", async () => {
  const editor = {
    document: [{ id: "target", type: "run" }],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("~");
});

test("getCurrentDirectory returns absolute path from directory block", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/absolute/path" } },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("/absolute/path");
});

test("getCurrentDirectory returns path from local-directory block", async () => {
  const editor = {
    document: [
      { id: "local-dir-1", type: "local-directory" },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("/stored/path");
});

test("getCurrentDirectory handles tilde paths as absolute", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "~/home/path" } },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("~/home/path");
});

test("getCurrentDirectory combines relative paths", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/base" } },
      { id: "dir2", type: "directory", props: { path: "./subdir" } },
      { id: "dir3", type: "directory", props: { path: "./nested" } },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("/base/./subdir/./nested");
});

test("getCurrentDirectory stops at first absolute path when walking backwards", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/should-not-include" } },
      { id: "dir2", type: "directory", props: { path: "/base" } },
      { id: "dir3", type: "directory", props: { path: "./relative" } },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("/base/./relative");
});

test("getCurrentDirectory handles all relative paths", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "./first" } },
      { id: "dir2", type: "directory", props: { path: "./second" } },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("~/./first/./second");
});

test("getCurrentDirectory mixes directory and local-directory blocks", async () => {
  const editor = {
    document: [
      { id: "dir1", type: "directory", props: { path: "/base" } },
      { id: "local-dir-2", type: "local-directory" },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("/base/./relative/stored");
});

test("getCurrentDirectory handles local-directory without stored path", async () => {
  const editor = {
    document: [
      { id: "local-dir-unknown", type: "local-directory" },
      { id: "target", type: "run" },
    ],
  };

  const result = await getCurrentDirectory(editor, "target", null);
  expect(result).toBe("~");
});
