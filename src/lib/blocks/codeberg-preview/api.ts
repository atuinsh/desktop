import { fetch } from "@tauri-apps/plugin-http";
import { extensionToLanguage } from "../shared/language-detection";

export interface CodebergRepoData {
  name: string;
  full_name: string;
  description: string | null;
  html_url: string;
  stars_count: number;
  forks_count: number;
  language: string | null;
  owner: {
    login: string;
    avatar_url: string;
  };
}

export interface CodebergPRData {
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

export interface CodebergIssueData {
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

export interface CodebergCodeData {
  content: string;
  filePath: string;
  language: string;
  lineStart?: number;
  lineEnd?: number;
  html_url: string;
}

export type CodebergData = CodebergRepoData | CodebergPRData | CodebergIssueData | CodebergCodeData;

const CODEBERG_API_BASE = "https://codeberg.org/api/v1";
const RAW_CODEBERG_BASE = "https://codeberg.org";

export async function fetchCodebergRepoData(owner: string, repo: string): Promise<CodebergRepoData> {
  const response = await fetch(`${CODEBERG_API_BASE}/repos/${owner}/${repo}`, {
    headers: {
      Accept: "application/json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch repo: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  return {
    name: data.name,
    full_name: data.full_name,
    description: data.description,
    html_url: data.html_url,
    stars_count: data.stars_count,
    forks_count: data.forks_count,
    language: data.language,
    owner: {
      login: data.owner.login,
      avatar_url: data.owner.avatar_url,
    },
  };
}

export async function fetchCodebergPRData(owner: string, repo: string, prNumber: number): Promise<CodebergPRData> {
  const response = await fetch(`${CODEBERG_API_BASE}/repos/${owner}/${repo}/pulls/${prNumber}`, {
    headers: {
      Accept: "application/json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch PR: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  return {
    number: data.number,
    title: data.title,
    body: data.body,
    html_url: data.html_url,
    state: data.state,
    merged: data.merged,
    additions: data.additions || 0,
    deletions: data.deletions || 0,
    changed_files: data.changed_files || 0,
    user: {
      login: data.user.login,
      avatar_url: data.user.avatar_url,
    },
  };
}

export async function fetchCodebergIssueData(owner: string, repo: string, issueNumber: number): Promise<CodebergIssueData> {
  const response = await fetch(`${CODEBERG_API_BASE}/repos/${owner}/${repo}/issues/${issueNumber}`, {
    headers: {
      Accept: "application/json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch issue: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  return {
    number: data.number,
    title: data.title,
    body: data.body,
    html_url: data.html_url,
    state: data.state,
    comments: data.comments || 0,
    labels: (data.labels || []).map((l: any) => ({ name: l.name, color: l.color })),
    user: {
      login: data.user.login,
      avatar_url: data.user.avatar_url,
    },
  };
}

export async function fetchCodebergCodeData(
  owner: string,
  repo: string,
  branch: string,
  filePath: string,
  lineStart?: number,
  lineEnd?: number,
): Promise<CodebergCodeData> {
  const response = await fetch(`${RAW_CODEBERG_BASE}/${owner}/${repo}/raw/branch/${branch}/${filePath}`, {
    headers: {
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch code: ${response.status} ${response.statusText}`);
  }

  const fullContent = await response.text();

  let content = fullContent;
  if (lineStart !== undefined) {
    const lines = fullContent.split("\n");
    const end = lineEnd ?? lineStart;
    content = lines.slice(lineStart - 1, end).join("\n");
  }

  const extension = filePath.split(".").pop() || "";
  const language = extensionToLanguage(extension);

  let html_url = `https://codeberg.org/${owner}/${repo}/src/branch/${branch}/${filePath}`;
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

