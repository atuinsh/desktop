import { linkPreviewRegistry, type LinkPreviewHandler } from "../link-preview";
import { isCodebergUrl, parseCodebergUrl } from "./url-parser";
import type { CodebergBlockProps } from "./schema";

/**
 * Codeberg link preview handler.
 * Converts Codeberg URLs into rich preview blocks.
 */
export const codebergLinkPreviewHandler: LinkPreviewHandler = {
  id: "codeberg",

  matches(url: string): boolean {
    return isCodebergUrl(url);
  },

  createBlock(url: string): { type: string; props: Record<string, string> } | null {
    const parsed = parseCodebergUrl(url);
    if (!parsed) {
      return null;
    }

    const props: Partial<CodebergBlockProps> = {
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
      type: "codebergPreview",
      props: props as Record<string, string>,
    };
  },
};

// Register the Codeberg handler
linkPreviewRegistry.register(codebergLinkPreviewHandler);
