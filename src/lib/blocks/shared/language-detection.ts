/**
 * Map file extensions to language identifiers for syntax highlighting.
 */
const EXTENSION_TO_LANGUAGE: Record<string, string> = {
  js: "javascript",
  jsx: "jsx",
  ts: "typescript",
  tsx: "tsx",
  py: "python",
  rb: "ruby",
  rs: "rust",
  go: "go",
  java: "java",
  kt: "kotlin",
  swift: "swift",
  c: "c",
  cpp: "cpp",
  h: "c",
  hpp: "cpp",
  cs: "csharp",
  php: "php",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  yml: "yaml",
  yaml: "yaml",
  json: "json",
  xml: "xml",
  html: "html",
  css: "css",
  scss: "scss",
  sass: "sass",
  less: "less",
  md: "markdown",
  sql: "sql",
  graphql: "graphql",
  dockerfile: "dockerfile",
  toml: "toml",
  ini: "ini",
  conf: "ini",
};

/**
 * Get the language identifier for syntax highlighting based on file extension.
 */
export function extensionToLanguage(ext: string): string {
  return EXTENSION_TO_LANGUAGE[ext.toLowerCase()] || "text";
}
