import { Card, CardBody, CardHeader } from "@heroui/react";
import { FileCodeIcon, ExternalLinkIcon } from "lucide-react";
import { Highlight } from "prism-react-renderer";
import Prism from "prismjs";
import type { GitLabCodeData } from "../api";
import { open } from "@tauri-apps/plugin-shell";
import { useStore } from "@/state/store";
import { themes } from "prism-react-renderer";

import "prismjs/components/prism-typescript";
import "prismjs/components/prism-jsx";
import "prismjs/components/prism-tsx";
import "prismjs/components/prism-python";
import "prismjs/components/prism-rust";
import "prismjs/components/prism-go";
import "prismjs/components/prism-java";
import "prismjs/components/prism-bash";
import "prismjs/components/prism-yaml";
import "prismjs/components/prism-json";
import "prismjs/components/prism-sql";
import "prismjs/components/prism-toml";

interface CodePreviewProps {
  data: GitLabCodeData;
}

export default function CodePreview({ data }: CodePreviewProps) {
  const colorMode = useStore((state) => state.functionalColorMode);

  const handleClick = () => {
    open(data.web_url);
  };

  const theme = colorMode === "dark" ? themes.vsDark : themes.vsLight;
  const startLineNumber = data.lineStart ?? 1;

  return (
    <Card className="w-full" shadow="sm">
      <CardHeader
        className="p-3 bg-default-50 cursor-pointer hover:bg-default-100 transition-colors"
        onClick={handleClick}
      >
        <div className="flex items-center gap-2 text-sm">
          <FileCodeIcon size={14} className="text-default-400" />
          <span className="font-mono text-default-600">{data.filePath}</span>
          {data.lineStart && (
            <span className="text-default-400">
              L{data.lineStart}
              {data.lineEnd && data.lineEnd !== data.lineStart && `-${data.lineEnd}`}
            </span>
          )}
          <ExternalLinkIcon size={14} className="text-default-400 ml-auto" />
        </div>
      </CardHeader>
      <CardBody className="p-0 overflow-auto max-h-80">
        <Highlight
          theme={theme}
          code={data.content}
          // @ts-ignore
          prism={Prism}
          language={data.language}
        >
          {({ style, tokens, getLineProps, getTokenProps }) => (
            <pre
              style={{ ...style, margin: 0, padding: "12px" }}
              className="text-sm font-mono overflow-x-auto"
            >
              {tokens.map((line, i) => (
                <div key={i} {...getLineProps({ line })}>
                  <span className="text-default-400 select-none inline-block w-10 text-right pr-4">
                    {startLineNumber + i}
                  </span>
                  {line.map((token, key) => (
                    <span key={key} {...getTokenProps({ token })} />
                  ))}
                </div>
              ))}
            </pre>
          )}
        </Highlight>
      </CardBody>
    </Card>
  );
}
