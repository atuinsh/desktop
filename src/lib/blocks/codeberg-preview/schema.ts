export const CODEBERG_PREVIEW_BLOCK_SCHEMA = {
  type: "codebergPreview",
  propSchema: {
    url: { default: "" },
    urlType: { default: "" }, // "repo" | "pr" | "issue" | "code"
    owner: { default: "" },
    repo: { default: "" },
    prNumber: { default: "" },
    issueNumber: { default: "" },
    branch: { default: "" },
    filePath: { default: "" },
    lineStart: { default: "" },
    lineEnd: { default: "" },
    cachedData: { default: "{}" },
    cachedAt: { default: "" },
  },
  content: "none",
} as const;

export type CodebergBlockProps = {
  url: string;
  urlType: string;
  owner: string;
  repo: string;
  prNumber: string;
  issueNumber: string;
  branch: string;
  filePath: string;
  lineStart: string;
  lineEnd: string;
  cachedData: string;
  cachedAt: string;
};
