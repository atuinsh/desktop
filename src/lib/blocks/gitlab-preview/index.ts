// GitLab Preview Block - link preview for GitLab URLs
export { default as GitLabPreviewBlockSpec } from "./spec";
export { GITLAB_PREVIEW_BLOCK_SCHEMA, type GitLabBlockProps } from "./schema";
export { isGitLabUrl, parseGitLabUrl } from "./url-parser";
export type { ParsedGitLabUrl, GitLabUrlType } from "./url-parser";
