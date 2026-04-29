import type { CSSProperties, ReactNode } from "react";
import { createElement } from "react";
import { Highlight, themes } from "prism-react-renderer";
import Prism from "prismjs";
import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";
import { open } from "@tauri-apps/plugin-shell";
import { useStore } from "@/state/store";

import "prismjs/components/prism-bash";
import "prismjs/components/prism-json";
import "prismjs/components/prism-jsx";
import "prismjs/components/prism-markdown";
import "prismjs/components/prism-python";
import "prismjs/components/prism-rust";
import "prismjs/components/prism-sql";
import "prismjs/components/prism-toml";
import "prismjs/components/prism-tsx";
import "prismjs/components/prism-typescript";
import "prismjs/components/prism-yaml";

interface MarkdownProps {
  content?: string;
}

const MARKDOWN_CODE_LANGUAGE_ALIASES: Record<string, string> = {
  cjs: "javascript",
  js: "javascript",
  jsx: "jsx",
  md: "markdown",
  py: "python",
  rs: "rust",
  sh: "bash",
  shell: "bash",
  text: "",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  yml: "yaml",
  zsh: "bash",
};

const BOOLEAN_ATTRIBUTES = new Set(["checked", "disabled", "open"]);
const VOID_ELEMENTS = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

const ATTRIBUTE_NAME_MAP: Record<string, string> = {
  class: "className",
  colspan: "colSpan",
  for: "htmlFor",
  rowspan: "rowSpan",
  tabindex: "tabIndex",
};

export function normalizeMarkdownCodeLanguage(language?: string | null): string {
  if (!language) {
    return "";
  }

  const normalized = language.trim().toLowerCase();
  return MARKDOWN_CODE_LANGUAGE_ALIASES[normalized] ?? normalized;
}

export function renderMarkdownHtml(content: string): string {
  return micromark(content, {
    extensions: [gfm()],
    htmlExtensions: [gfmHtml()],
  });
}

function parseInlineStyle(styleText: string): CSSProperties {
  const style: Record<string, string> = {};

  for (const declaration of styleText.split(";")) {
    const [property, ...valueParts] = declaration.split(":");
    if (!property || valueParts.length === 0) {
      continue;
    }

    const cssProperty = property.trim();
    const cssValue = valueParts.join(":").trim();
    if (!cssProperty || !cssValue) {
      continue;
    }

    const camelCasedProperty = cssProperty.replace(/-([a-z])/g, (_, char: string) =>
      char.toUpperCase(),
    );
    style[camelCasedProperty] = cssValue;
  }

  return style;
}

function elementAttributesToProps(element: Element): Record<string, unknown> {
  const props: Record<string, unknown> = {};

  for (const attribute of Array.from(element.attributes)) {
    if (attribute.name.startsWith("on")) {
      continue;
    }

    if (attribute.name === "checked") {
      props.defaultChecked = true;
      continue;
    }

    if (attribute.name === "style") {
      const style = parseInlineStyle(attribute.value);
      if (Object.keys(style).length > 0) {
        props.style = style;
      }
      continue;
    }

    const propName = ATTRIBUTE_NAME_MAP[attribute.name] ?? attribute.name;
    if (BOOLEAN_ATTRIBUTES.has(attribute.name) && attribute.value === "") {
      props[propName] = true;
      continue;
    }

    props[propName] = attribute.value;
  }

  return props;
}

function extractCodeLanguage(codeElement: Element): string {
  const className = codeElement.getAttribute("class") ?? "";
  const languageClass = className
    .split(/\s+/)
    .find((entry) => entry.startsWith("language-"));

  return normalizeMarkdownCodeLanguage(languageClass?.replace("language-", ""));
}

function MarkdownCodeBlock({
  code,
  language,
  isDark,
}: {
  code: string;
  language: string;
  isDark: boolean;
}) {
  const theme = isDark ? themes.oneDark : themes.github;
  const grammar = language ? Prism.languages[language] : undefined;

  if (!grammar) {
    return (
      <pre className="markdown-code-block">
        <code>{code}</code>
      </pre>
    );
  }

  return (
    <Highlight
      theme={theme}
      code={code}
      prism={Prism}
      language={language}
    >
      {({ style, tokens, getLineProps, getTokenProps }) => (
        <pre style={style} className="markdown-code-block">
          <code>
            {tokens.map((line, lineIndex) => (
              <div key={lineIndex} {...getLineProps({ line })}>
                {line.map((token, tokenIndex) => (
                  <span key={tokenIndex} {...getTokenProps({ token })} />
                ))}
              </div>
            ))}
          </code>
        </pre>
      )}
    </Highlight>
  );
}

function renderMarkdownNode(node: ChildNode, key: string, isDark: boolean): ReactNode {
  if (node.nodeType === node.TEXT_NODE) {
    return node.textContent;
  }

  if (node.nodeType !== node.ELEMENT_NODE) {
    return null;
  }

  const element = node as Element;

  if (element.tagName === "PRE") {
    const firstChild = element.firstElementChild;
    if (firstChild?.tagName === "CODE") {
      return (
        <MarkdownCodeBlock
          key={key}
          code={firstChild.textContent ?? ""}
          language={extractCodeLanguage(firstChild)}
          isDark={isDark}
        />
      );
    }
  }

  const children = Array.from(element.childNodes).map((child, index) =>
    renderMarkdownNode(child, `${key}.${index}`, isDark),
  );
  const tagName = element.tagName.toLowerCase();

  if (VOID_ELEMENTS.has(tagName)) {
    return createElement(tagName, { key, ...elementAttributesToProps(element) });
  }

  return createElement(tagName, { key, ...elementAttributesToProps(element) }, children);
}

export function renderMarkdownNodes(content: string, isDark: boolean): ReactNode[] {
  const html = renderMarkdownHtml(content);
  const template = document.createElement("template");
  template.innerHTML = html;

  return Array.from(template.content.childNodes).map((node, index) =>
    renderMarkdownNode(node, `markdown-${index}`, isDark),
  );
}

export default function Markdown(props: MarkdownProps) {
  const isDark = useStore((state) => state.functionalColorMode === "dark");

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

  return (
    <div className="markdown-content" onClick={handleLinkClick}>
      {renderMarkdownNodes(props.content || "", isDark)}
    </div>
  );
}
