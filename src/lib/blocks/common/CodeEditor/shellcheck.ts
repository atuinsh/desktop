import { linter, Diagnostic } from "@codemirror/lint";
import { Extension, Text } from "@codemirror/state";
import { invoke } from "@tauri-apps/api/core";
import { Comment } from "@/rs-bindings/Comment";
import { ShellCheckOutput } from "@/rs-bindings/ShellCheckOutput";


function posToOffset(doc: Text, line: number, column: number) {
  return doc.line(line).from + column - 1;
}

function createMessageNode(code: number, message: string) {
  const scCode = `SC${code}`;
  const referenceURL = `https://www.shellcheck.net/wiki/${scCode}`;

  const a = document.createElement("a");
  a.setAttribute("href", referenceURL);
  a.setAttribute("target", "_blank");
  a.setAttribute("rel", "noreferrer");
  a.setAttribute("rel", "noopener");
  a.setAttribute("title", referenceURL);
  a.appendChild(document.createTextNode(scCode));

  const messageNode = document.createElement("span");
  messageNode.appendChild(document.createTextNode(`${message} (`));
  messageNode.appendChild(a);
  messageNode.appendChild(document.createTextNode(")"));

  return messageNode;
}

function shellCheckCommentToDiagnostic(doc: Text, {
  line,
  column,
  endLine,
  endColumn,
  level,
  code,
  message,
}: Comment): Diagnostic {
  return {
    from: posToOffset(doc, line, column),
    to: posToOffset(doc, endLine, endColumn),
    severity: level === "style" ? "hint" : level,
    source: "ShellCheck",
    renderMessage: () => createMessageNode(code, message),
    message: message,
  }
}

export function makeShellCheckLinter(arg0: string, shell: string): Extension {
  return linter(async view => {
    const raw = await invoke<ArrayBuffer>("shellcheck", {
      arg0,
      shell,
      script: view.state.doc.toString(),
    });
    const shellCheckOutput = JSON.parse(new TextDecoder().decode(raw)) as ShellCheckOutput;

    return shellCheckOutput.comments.map(comment =>
      (shellCheckCommentToDiagnostic(view.state.doc, comment))
    );
  })
}

export const supportedShells = ["sh", "bash", "dash", "ksh", "busybox"] as const;
