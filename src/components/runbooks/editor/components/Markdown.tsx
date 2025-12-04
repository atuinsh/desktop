import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";
import { open } from "@tauri-apps/plugin-shell";

interface MarkdownProps {
  content?: string;
}

export default function Markdown(props: MarkdownProps) {
  const handleLinkClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const link = target.closest("a");
    if (link) {
      e.preventDefault();
      const href = link.getAttribute("href");
      if (href) {
        open(href);
      }
    }
  };

  const renderMarkdown = (content: string): string => {
    return micromark(content, {
      extensions: [gfm()],
      htmlExtensions: [gfmHtml()],
    });
  };

  return (
    <div
      className="markdown-content prose dark:prose-invert"
      dangerouslySetInnerHTML={{ __html: renderMarkdown(props.content || "") }}
      onClick={handleLinkClick}
    />
  );
}
