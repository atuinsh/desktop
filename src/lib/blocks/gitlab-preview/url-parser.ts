export type GitLabUrlType = "repo" | "mr" | "issue" | "code";

export interface ParsedGitLabUrl {
  type: GitLabUrlType;
  projectPath: string; // Can be nested: group/subgroup/project
  mrNumber?: number;
  issueNumber?: number;
  branch?: string;
  filePath?: string;
  lineStart?: number;
  lineEnd?: number;
}

const GITLAB_URL_REGEX = /^https?:\/\/(www\.)?gitlab\.com\/(.+?)(?:\/-\/(.*))?$/;

export function isGitLabUrl(text: string): boolean {
  return GITLAB_URL_REGEX.test(text.trim());
}

export function parseGitLabUrl(url: string): ParsedGitLabUrl | null {
  const trimmedUrl = url.trim();
  const match = trimmedUrl.match(GITLAB_URL_REGEX);

  if (!match) {
    return null;
  }

  let projectPath = match[2];
  const rest = match[3] || "";

  // Remove trailing slash from project path
  projectPath = projectPath.replace(/\/$/, "");

  // If rest is empty, it's a repo URL
  if (!rest) {
    return {
      type: "repo",
      projectPath,
    };
  }

  // Check for MR URL: merge_requests/{number}
  const mrMatch = rest.match(/^merge_requests\/(\d+)/);
  if (mrMatch) {
    return {
      type: "mr",
      projectPath,
      mrNumber: parseInt(mrMatch[1], 10),
    };
  }

  // Check for issue URL: issues/{number}
  const issueMatch = rest.match(/^issues\/(\d+)/);
  if (issueMatch) {
    return {
      type: "issue",
      projectPath,
      issueNumber: parseInt(issueMatch[1], 10),
    };
  }

  // Check for code URL: blob/{branch}/{path}#L{start}-{end}
  // GitLab uses #L10-20 format (not #L10-L20)
  const codeMatch = rest.match(/^blob\/([^/]+)\/(.+?)(?:#L(\d+)(?:-(\d+))?)?$/);
  if (codeMatch) {
    const result: ParsedGitLabUrl = {
      type: "code",
      projectPath,
      branch: codeMatch[1],
      filePath: codeMatch[2],
    };

    if (codeMatch[3]) {
      result.lineStart = parseInt(codeMatch[3], 10);
    }
    if (codeMatch[4]) {
      result.lineEnd = parseInt(codeMatch[4], 10);
    }

    return result;
  }

  // Default to repo for other paths
  return {
    type: "repo",
    projectPath,
  };
}
