import { linkPreviewRegistry, type LinkPreviewHandler } from "../link-preview";
import { isGitHubUrl, parseGitHubUrl } from "./url-parser";
import type { GitHubBlockProps } from "./schema";

/**
 * GitHub link preview handler.
 * Converts GitHub URLs into rich preview blocks.
 */
export const githubLinkPreviewHandler: LinkPreviewHandler = {
  id: "github",

  matches(url: string): boolean {
    return isGitHubUrl(url);
  },

  createBlock(url: string): { type: string; props: Record<string, string> } | null {
    const parsed = parseGitHubUrl(url);
    if (!parsed) {
      return null;
    }

    const props: Partial<GitHubBlockProps> = {
      url,
      urlType: parsed.type,
      owner: parsed.owner,
      repo: parsed.repo,
    };

    if (parsed.type === "pr" && parsed.prNumber !== undefined) {
      props.prNumber = parsed.prNumber.toString();
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
      type: "githubPreview",
      props: props as Record<string, string>,
    };
  },
};

// Register the GitHub handler
linkPreviewRegistry.register(githubLinkPreviewHandler);
