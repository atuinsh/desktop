import { useMemo } from "react";
import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";
import hljs from "highlight.js";
import "highlight.js/styles/github-dark.css";

interface MarkdownContentProps {
  content: string;
  className?: string;
}

export default function MarkdownContent({ content, className = "" }: MarkdownContentProps) {
  const html = useMemo(() => {
    if (!content) return "";

    // Convert markdown to HTML
    let rawHtml = micromark(content, {
      extensions: [gfm()],
      htmlExtensions: [gfmHtml()],
    });

    // Post-process to add syntax highlighting to code blocks
    // This is a simple regex-based approach
    rawHtml = rawHtml.replace(
      /<pre><code class="language-(\w+)">([\s\S]*?)<\/code><\/pre>/g,
      (match, lang, code) => {
        try {
          // Decode HTML entities in the code
          const decodedCode = code
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/&amp;/g, "&")
            .replace(/&quot;/g, '"')
            .replace(/&#39;/g, "'");

          const highlighted = hljs.highlight(decodedCode, {
            language: lang,
            ignoreIllegals: true,
          }).value;

          return `<pre><code class="hljs language-${lang}">${highlighted}</code></pre>`;
        } catch (e) {
          // If highlighting fails, return original
          return match;
        }
      }
    );

    // Also handle code blocks without language specified
    rawHtml = rawHtml.replace(
      /<pre><code>([\s\S]*?)<\/code><\/pre>/g,
      (match, code) => {
        try {
          const decodedCode = code
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/&amp;/g, "&")
            .replace(/&quot;/g, '"')
            .replace(/&#39;/g, "'");

          const highlighted = hljs.highlightAuto(decodedCode).value;
          return `<pre><code class="hljs">${highlighted}</code></pre>`;
        } catch (e) {
          return match;
        }
      }
    );

    return rawHtml;
  }, [content]);

  return (
    <div
      className={`markdown-content ${className}`}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

