export const GITHUB_PREVIEW_BLOCK_SCHEMA = {
  type: "githubPreview",
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
    cachedData: { default: "{}" }, // JSON-stringified preview data
    cachedAt: { default: "" }, // ISO timestamp for cache invalidation
  },
  content: "none",
} as const;

export type GitHubBlockProps = {
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
