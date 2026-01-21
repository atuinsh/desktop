export const GITLAB_PREVIEW_BLOCK_SCHEMA = {
  type: "gitlabPreview",
  propSchema: {
    url: { default: "" },
    urlType: { default: "" }, // "repo" | "mr" | "issue" | "code"
    projectPath: { default: "" }, // GitLab uses nested paths: group/subgroup/project
    mrNumber: { default: "" },
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

export type GitLabBlockProps = {
  url: string;
  urlType: string;
  projectPath: string;
  mrNumber: string;
  issueNumber: string;
  branch: string;
  filePath: string;
  lineStart: string;
  lineEnd: string;
  cachedData: string;
  cachedAt: string;
};
