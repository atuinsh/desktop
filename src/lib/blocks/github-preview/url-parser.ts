export type GitHubUrlType = "repo" | "pr" | "issue" | "code";

export interface ParsedGitHubUrl {
  type: GitHubUrlType;
  owner: string;
  repo: string;
  prNumber?: number;
  issueNumber?: number;
  branch?: string;
  filePath?: string;
  lineStart?: number;
  lineEnd?: number;
}

const GITHUB_URL_REGEX = /^https?:\/\/(www\.)?github\.com\/([^/]+)\/([^/]+)(\/.*)?$/;

export function isGitHubUrl(text: string): boolean {
  return GITHUB_URL_REGEX.test(text.trim());
}

export function parseGitHubUrl(url: string): ParsedGitHubUrl | null {
  const trimmedUrl = url.trim();
  const match = trimmedUrl.match(GITHUB_URL_REGEX);

  if (!match) {
    return null;
  }

  const owner = match[2];
  const repo = match[3];
  const rest = match[4] || "";

  // Check for PR URL: /pull/{number}
  const prMatch = rest.match(/^\/pull\/(\d+)/);
  if (prMatch) {
    return {
      type: "pr",
      owner,
      repo,
      prNumber: parseInt(prMatch[1], 10),
    };
  }

  // Check for issue URL: /issues/{number}
  const issueMatch = rest.match(/^\/issues\/(\d+)/);
  if (issueMatch) {
    return {
      type: "issue",
      owner,
      repo,
      issueNumber: parseInt(issueMatch[1], 10),
    };
  }

  // Check for code URL: /blob/{branch}/{path}#L{start}-L{end}
  const codeMatch = rest.match(/^\/blob\/([^/]+)\/(.+?)(?:#L(\d+)(?:-L(\d+))?)?$/);
  if (codeMatch) {
    const result: ParsedGitHubUrl = {
      type: "code",
      owner,
      repo,
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

  // Default to repo for any other path (tree, commits, branches, etc.)
  return {
    type: "repo",
    owner,
    repo,
  };
}
