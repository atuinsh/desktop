// Based on the basicSetup extension, as suggested by the source. Customized for Atuin.

import {
  KeyBinding,
  lineNumbers,
  highlightActiveLineGutter,
  highlightSpecialChars,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor,
  highlightActiveLine,
  keymap,
} from "@codemirror/view";
import { EditorState, Extension } from "@codemirror/state";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";

import {
  closeBrackets,
  autocompletion,
  closeBracketsKeymap,
  completionKeymap,
  CompletionContext,
} from "@codemirror/autocomplete";

import {
  foldGutter,
  indentOnInput,
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
  indentUnit,
  foldKeymap,
} from "@codemirror/language";

import { lintKeymap } from "@codemirror/lint";
import { invoke } from "@tauri-apps/api/core";
import { withoutProperties } from "@/lib/utils";
import { JinjaCompletionConfig } from "@codemirror/lang-jinja";
import { ResolvedContext } from "@/rs-bindings/ResolvedContext";

export interface MinimalSetupOptions {
  highlightSpecialChars?: boolean;
  history?: boolean;
  drawSelection?: boolean;
  syntaxHighlighting?: boolean;

  defaultKeymap?: boolean;
  historyKeymap?: boolean;
}

export interface BasicSetupOptions extends MinimalSetupOptions {
  lineNumbers?: boolean;
  highlightActiveLineGutter?: boolean;
  foldGutter?: boolean;
  dropCursor?: boolean;
  allowMultipleSelections?: boolean;
  indentOnInput?: boolean;
  bracketMatching?: boolean;
  closeBrackets?: boolean;
  autocompletion?: boolean;
  rectangularSelection?: boolean;
  crosshairCursor?: boolean;
  highlightActiveLine?: boolean;
  highlightSelectionMatches?: boolean;

  closeBracketsKeymap?: boolean;
  searchKeymap?: boolean;
  foldKeymap?: boolean;
  completionKeymap?: boolean;
  lintKeymap?: boolean;
  tabSize?: number;
}

export type JinjaVarSource = Record<string, string> | string;

export interface JinjaVariables {
  var: Record<string, string>;
  varSource: Record<string, JinjaVarSource>;
  doc: Record<string, string>;
  env: Record<string, string>;
  workspace: Record<string, string>;
}

export function atuinHistoryCompletions(context: CompletionContext) {
  let word = context.matchBefore(/^.*/);

  if (!word) return null;
  if (word.from == word.to && !context.explicit) return null;

  return invoke("prefix_search", { query: word.text }).then(
    // @ts-ignore
    (results: string[]) => {
      let options = results.map((i) => {
        return { label: i, type: "text" };
      });

      return {
        from: word.from,
        options,
      };
    },
  );
}

export function buildJinjaAutocomplete(blockContext: ResolvedContext): JinjaCompletionConfig {
  let jinjaVariables: JinjaVariables = {
    var: blockContext.variables as Record<string, string>,
    varSource: blockContext.variablesSources as Record<string, JinjaVarSource>,
    doc: {
      first: "first block",
      last: "last block",
      previous: "previous block",
      named: "named block",
      content: "runbook blocks",
    },
    env: blockContext.envVars as Record<string, string>,
    workspace: {
      root: "workspace root",
    },
  };

  return {
    // Top-level variables: var, doc, env, etc.
    variables: Object.keys(withoutProperties(jinjaVariables, ["varSource"])).map((v) => {
      let detail = "properties";
      if (v === "var") {
        detail = "template variables";
      } else if (v === "doc") {
        detail = "document properties";
      } else if (v === "env") {
        detail = "environment variables";
      } else if (v === "workspace") {
        detail = "workspace properties";
      }

      return {
        label: v,
        type: "constant",
        detail: detail,
      };
    }),
    properties: (path, _state, _context) => {
      // Lookup variables based on the `path` array.
      // When accessing var.*, path will be ["var"]
      // When accessing var.something.*, path will be ["var", "something"]
      let current: any = withoutProperties(jinjaVariables, ["varSource"]);
      let currentSource: Record<string, JinjaVarSource> | JinjaVarSource | undefined = undefined;

      for (const [index, segment] of path.entries()) {
        if (index === 0 && segment === "var") {
          currentSource = jinjaVariables.varSource;
        }

        if (current && typeof current === "object" && segment in current) {
          current = current[segment];
          if (index !== 0) {
            if (currentSource && typeof currentSource === "object" && segment in currentSource) {
              currentSource = currentSource[segment];
            } else {
              currentSource = undefined;
            }
          }
        } else {
          return [];
        }
      }
      if (current && typeof current === "object" && !Array.isArray(current)) {
        let sectionName = "Properties";
        if (path[0] === "var") {
          sectionName = "Template Variables";
        } else if (path[0] === "doc") {
          sectionName = "Document Properties";
        } else if (path[0] === "env") {
          sectionName = "Environment Variables";
        } else if (path[0] === "workspace") {
          sectionName = "Workspace Properties";
        }

        return Object.keys(current).map((v) => ({
          label: v,
          type: "property" as const,
          detail:
            currentSource &&
            typeof currentSource === "object" &&
            typeof currentSource[v] === "string"
              ? currentSource[v]
              : current[v]
              ? current[v]
              : null,
          section: {
            name: sectionName,
          },
        }));
      }
      return [];
    },
  };
}

const buildAutocomplete = (): Extension => {
  let ac = autocompletion({
    activateOnTyping: true,
    defaultKeymap: true,
    tooltipClass: () => "atuin-cm-completion",
  });

  return ac;
};

export const extensions = (options: BasicSetupOptions = {}): Extension[] => {
  const { crosshairCursor: initCrosshairCursor = false } = options;

  let keymaps: KeyBinding[] = [];
  if (options.closeBracketsKeymap !== false) {
    keymaps = keymaps.concat(closeBracketsKeymap);
  }
  if (options.defaultKeymap !== false) {
    keymaps = keymaps.concat(defaultKeymap);
  }
  if (options.searchKeymap !== false) {
    keymaps = keymaps.concat(searchKeymap);
  }
  if (options.historyKeymap !== false) {
    keymaps = keymaps.concat(historyKeymap);
  }
  if (options.foldKeymap !== false) {
    keymaps = keymaps.concat(foldKeymap);
  }
  if (options.completionKeymap !== false) {
    keymaps = keymaps.concat(completionKeymap);
  }
  if (options.lintKeymap !== false) {
    keymaps = keymaps.concat(lintKeymap);
  }
  const extensions: Extension[] = [];
  if (options.lineNumbers !== false) extensions.push(lineNumbers());
  if (options.highlightActiveLineGutter !== false) extensions.push(highlightActiveLineGutter());
  if (options.highlightSpecialChars !== false) extensions.push(highlightSpecialChars());
  if (options.history !== false) extensions.push(history());
  if (options.foldGutter !== false) extensions.push(foldGutter());
  if (options.drawSelection !== false) extensions.push(drawSelection());
  if (options.dropCursor !== false) extensions.push(dropCursor());
  if (options.allowMultipleSelections !== false)
    extensions.push(EditorState.allowMultipleSelections.of(true));
  if (options.indentOnInput !== false) extensions.push(indentOnInput());
  if (options.syntaxHighlighting !== false)
    extensions.push(syntaxHighlighting(defaultHighlightStyle, { fallback: true }));

  if (options.bracketMatching !== false) extensions.push(bracketMatching());
  if (options.closeBrackets !== false) extensions.push(closeBrackets());
  if (options.autocompletion !== false) {
    // extensions.push(atuinHistoryCompletionsLangData);
    extensions.push(buildAutocomplete());
  }

  if (options.rectangularSelection !== false) extensions.push(rectangularSelection());
  if (initCrosshairCursor !== false) extensions.push(crosshairCursor());
  if (options.highlightActiveLine !== false) extensions.push(highlightActiveLine());
  if (options.highlightSelectionMatches !== false) extensions.push(highlightSelectionMatches());
  if (options.tabSize && typeof options.tabSize === "number")
    extensions.push(indentUnit.of(" ".repeat(options.tabSize)));

  return extensions.concat([keymap.of(keymaps.flat())]).filter(Boolean);
};
