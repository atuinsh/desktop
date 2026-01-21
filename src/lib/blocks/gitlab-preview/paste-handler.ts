import { linkPreviewRegistry, type LinkPreviewHandler } from "../link-preview";
import { isGitLabUrl, parseGitLabUrl } from "./url-parser";
import type { GitLabBlockProps } from "./schema";

/**
 * GitLab link preview handler.
 * Converts GitLab URLs into rich preview blocks.
 */
export const gitlabLinkPreviewHandler: LinkPreviewHandler = {
  id: "gitlab",

  matches(url: string): boolean {
    return isGitLabUrl(url);
  },

  createBlock(url: string): { type: string; props: Record<string, string> } | null {
    const parsed = parseGitLabUrl(url);
    if (!parsed) {
      return null;
    }

    const props: Partial<GitLabBlockProps> = {
      url,
      urlType: parsed.type,
      projectPath: parsed.projectPath,
    };

    if (parsed.type === "mr" && parsed.mrNumber !== undefined) {
      props.mrNumber = parsed.mrNumber.toString();
    }

    if (parsed.type === "issue" && parsed.issueNumber !== undefined) {
      props.issueNumber = parsed.issueNumber.toString();
    }

    if (parsed.type === "code") {
      if (parsed.branch) props.branch = parsed.branch;
      if (parsed.filePath) props.filePath = parsed.filePath;
      if (parsed.lineStart !== undefined) props.lineStart = parsed.lineStart.toString();
      if (parsed.lineEnd !== undefined) props.lineEnd = parsed.lineEnd.toString();
    }

    return {
      type: "gitlabPreview",
      props: props as Record<string, string>,
    };
  },
};

// Register the GitLab handler
linkPreviewRegistry.register(gitlabLinkPreviewHandler);
