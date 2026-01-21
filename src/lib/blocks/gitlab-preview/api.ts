import { fetch } from "@tauri-apps/plugin-http";
import { extensionToLanguage } from "../shared/language-detection";

export interface GitLabRepoData {
  name: string;
  path_with_namespace: string;
  description: string | null;
  web_url: string;
  star_count: number;
  forks_count: number;
  // GitLab doesn't return primary language in project API
  owner: {
    username: string;
    avatar_url: string;
  };
}

export interface GitLabMRData {
  iid: number;
  title: string;
  description: string | null;
  web_url: string;
  state: "opened" | "closed" | "merged";
  author: {
    username: string;
    avatar_url: string;
  };
  // These require additional API call, may be null
  changes_count?: string;
}

export interface GitLabIssueData {
  iid: number;
  title: string;
  description: string | null;
  web_url: string;
  state: "opened" | "closed";
  user_notes_count: number;
  labels: string[];
  author: {
    username: string;
    avatar_url: string;
  };
}

export interface GitLabCodeData {
  content: string;
  filePath: string;
  language: string;
  lineStart?: number;
  lineEnd?: number;
  web_url: string;
}

export type GitLabData = GitLabRepoData | GitLabMRData | GitLabIssueData | GitLabCodeData;

const GITLAB_API_BASE = "https://gitlab.com/api/v4";

function encodeProjectPath(projectPath: string): string {
  return encodeURIComponent(projectPath);
}

export async function fetchGitLabRepoData(projectPath: string): Promise<GitLabRepoData> {
  const response = await fetch(`${GITLAB_API_BASE}/projects/${encodeProjectPath(projectPath)}`, {
    headers: {
      Accept: "application/json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch project: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  return {
    name: data.name,
    path_with_namespace: data.path_with_namespace,
    description: data.description,
    web_url: data.web_url,
    star_count: data.star_count,
    forks_count: data.forks_count,
    owner: {
      username: data.namespace?.name || data.path_with_namespace.split("/")[0],
      avatar_url: data.avatar_url || data.namespace?.avatar_url || "",
    },
  };
}

export async function fetchGitLabMRData(projectPath: string, mrNumber: number): Promise<GitLabMRData> {
  const response = await fetch(`${GITLAB_API_BASE}/projects/${encodeProjectPath(projectPath)}/merge_requests/${mrNumber}`, {
    headers: {
      Accept: "application/json",
      "User-Agent": "Atuin-Desktop",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch MR: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  return {
    iid: data.iid,
    title: data.title,
    description: data.description,
    web_url: data.web_url,
    state: data.state,
    author: {
      username: data.author.username,
      avatar_url: data.author.avatar_url,
    },
    changes_count: data.changes_count,
  };
}

export async function fetchGitLabIssueData(projectPath: string, issueNumber: number): Promise<GitLabIssueData> {
  const response = await fetch(`${GITLAB_API_BASE}/projects/${encodeProjectPath(projectPath)}/issues/${issueNumber}`, {
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
    iid: data.iid,
    title: data.title,
    description: data.description,
    web_url: data.web_url,
    state: data.state,
    user_notes_count: data.user_notes_count || 0,
    labels: data.labels || [],
    author: {
      username: data.author.username,
      avatar_url: data.author.avatar_url,
    },
  };
}

export async function fetchGitLabCodeData(
  projectPath: string,
  branch: string,
  filePath: string,
  lineStart?: number,
  lineEnd?: number,
): Promise<GitLabCodeData> {
  const response = await fetch(
    `${GITLAB_API_BASE}/projects/${encodeProjectPath(projectPath)}/repository/files/${encodeURIComponent(filePath)}/raw?ref=${encodeURIComponent(branch)}`,
    {
      headers: {
        "User-Agent": "Atuin-Desktop",
      },
    },
  );

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

  let web_url = `https://gitlab.com/${projectPath}/-/blob/${branch}/${filePath}`;
  if (lineStart !== undefined) {
    web_url += `#L${lineStart}`;
    if (lineEnd !== undefined && lineEnd !== lineStart) {
      web_url += `-${lineEnd}`;
    }
  }

  return {
    content,
    filePath,
    language,
    lineStart,
    lineEnd,
    web_url,
  };
}

