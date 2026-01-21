import { fetch } from "@tauri-apps/plugin-http";
import { extensionToLanguage } from "../shared/language-detection";

export interface RepoData {
  name: string;
  full_name: string;
  description: string | null;
  html_url: string;
  stargazers_count: number;
  forks_count: number;
  language: string | null;
  owner: {
    login: string;
    avatar_url: string;
  };
}

export interface PRData {
  number: number;
  title: string;
  body: string | null;
  html_url: string;
  state: "open" | "closed";
  merged: boolean;
  additions: number;
  deletions: number;
  changed_files: number;
  user: {
    login: string;
    avatar_url: string;
  };
}

export interface IssueData {
  number: number;
  title: string;
  body: string | null;
  html_url: string;
  state: "open" | "closed";
  comments: number;
  labels: Array<{
    name: string;
    color: string;
  }>;
  user: {
    login: string;
    avatar_url: string;
  };
}

export interface CodeData {
  content: string;
  filePath: string;
  language: string;
  lineStart?: number;
  lineEnd?: number;
  html_url: string;
}

export type GitHubData = RepoData | PRData | IssueData | CodeData;

const GITHUB_API_BASE = "https://api.github.com";
const RAW_GITHUB_BASE = "https://raw.githubusercontent.com";

export async function fetchRepoData(owner: string, repo: string): Promise<RepoData> {
  const response = await fetch(`${GITHUB_API_BASE}/repos/${owner}/${repo}`, {
    headers: {
      Accept: "application/vnd.github.v3+json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch repo: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

export async function fetchPRData(owner: string, repo: string, prNumber: number): Promise<PRData> {
  const response = await fetch(`${GITHUB_API_BASE}/repos/${owner}/${repo}/pulls/${prNumber}`, {
    headers: {
      Accept: "application/vnd.github.v3+json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch PR: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

export async function fetchIssueData(owner: string, repo: string, issueNumber: number): Promise<IssueData> {
  const response = await fetch(`${GITHUB_API_BASE}/repos/${owner}/${repo}/issues/${issueNumber}`, {
    headers: {
      Accept: "application/vnd.github.v3+json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch issue: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

export async function fetchCodeData(
  owner: string,
  repo: string,
  branch: string,
  filePath: string,
  lineStart?: number,
  lineEnd?: number,
): Promise<CodeData> {
  const response = await fetch(`${RAW_GITHUB_BASE}/${owner}/${repo}/${branch}/${filePath}`, {
    headers: {
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch code: ${response.status} ${response.statusText}`);
  }

  const fullContent = await response.text();

  // Extract lines if specified
  let content = fullContent;
  if (lineStart !== undefined) {
    const lines = fullContent.split("\n");
    const end = lineEnd ?? lineStart;
    content = lines.slice(lineStart - 1, end).join("\n");
  }

  // Detect language from file extension
  const extension = filePath.split(".").pop() || "";
  const language = extensionToLanguage(extension);

  // Build GitHub URL with line references
  let html_url = `https://github.com/${owner}/${repo}/blob/${branch}/${filePath}`;
  if (lineStart !== undefined) {
    html_url += `#L${lineStart}`;
    if (lineEnd !== undefined && lineEnd !== lineStart) {
      html_url += `-L${lineEnd}`;
    }
  }

  return {
    content,
    filePath,
    language,
    lineStart,
    lineEnd,
    html_url,
  };
}

