import CodeMirror, { KeyBinding, keymap, Prec } from "@uiw/react-codemirror";
import * as themes from "@uiw/codemirror-themes-all";
import { langs } from "@uiw/codemirror-extensions-langs";
import { jinja } from "@codemirror/lang-jinja";
import { extensions, atuinHistoryCompletions, buildJinjaAutocomplete } from "./extensions";
import { useMemo, useState } from "react";
import { acceptCompletion, completionStatus } from "@codemirror/autocomplete";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import { indentLess, indentMore } from "@codemirror/commands";
import { vim } from "@replit/codemirror-vim";
import { useStore } from "@/state/store";
import { makeShellCheckLinter, supportedShells } from "./shellcheck";
import { Button, Tooltip, addToast } from "@heroui/react";
import { ClipboardIcon } from "lucide-react";
import { useBlockContext } from "@/lib/hooks/useDocumentBridge";

interface CodeEditorProps {
  id: string;
  blockId?: string;
  code: string;
  isEditable: boolean;
  language: string;
  theme: string;
  keyMap?: KeyBinding[];
  onChange: (code: string) => void;
  onFocus?: () => void;
}

export const TabAutoComplete: KeyBinding = {
  key: "Tab",
  run: (view) => {
    // Only accept completion if there's an active completion popup
    if (completionStatus(view.state) === "active") {
      return acceptCompletion(view);
    }
    // Otherwise, perform normal tab indentation
    return indentMore(view);
  },
  shift: (view) => {
    return indentLess(view);
  },
};

export default function CodeEditor({
  id,
  blockId,
  code,
  isEditable,
  onChange,
  language,
  theme,
  keyMap,
  onFocus,
}: CodeEditorProps) {
  const vimModeEnabled = useStore((state) => state.vimModeEnabled);

  const shellCheckEnabled = useStore((state) => state.shellCheckEnabled);
  const shellCheckPath = useStore((state) => state.shellCheckPath || "shellcheck");

  const [isFocused, setIsFocused] = useState(false);

  let blockContext = useBlockContext(blockId || id);

  let editorLanguage = useMemo(() => {
    // Do the best we can with the interpreter name - get the language
    // TODO: consider dropdown to override this
    if (
      language.indexOf("bash") != -1 ||
      language.indexOf("sh") != -1 ||
      language.indexOf("zsh") != -1
    ) {
      return langs.shell();
    }

    if (language.indexOf("python") != -1) {
      return langs.python();
    }

    if (
      language.indexOf("node") != -1 ||
      language.indexOf("js") != -1 ||
      language.indexOf("bun") != -1 ||
      language.indexOf("deno") != -1
    ) {
      return langs.javascript();
    }

    if (language.indexOf("lua") != -1) {
      return langs.lua();
    }

    if (language.indexOf("ruby") != -1) {
      return langs.ruby();
    }

    return null;
  }, [language]);

  const customKeymap = Prec.highest(keymap.of(keyMap || [TabAutoComplete]));
  const themeObj = (themes as any)[theme];
  const codeMirrorValue = useCodeMirrorValue(code, onChange);

  const shellCheckShell = useMemo(() => {
    for (const shell of supportedShells) {
      if (language.indexOf(shell) != -1) {
        return shell;
      }
    }

    return null;
  }, [language]);

  const shellCheckOn = shellCheckShell !== null && shellCheckEnabled;

  let editorExtensions: any[] = useMemo(() => {
    const ext = [...extensions(), customKeymap];
    if (vimModeEnabled) {
      ext.unshift(vim());
    }

    ext.push(jinja(buildJinjaAutocomplete(blockContext)));

    if (editorLanguage) {
      ext.push(editorLanguage);

      if (typeof editorLanguage === "object" && "data" in editorLanguage) {
        ext.push(
          editorLanguage.data.of({
            autocomplete: atuinHistoryCompletions,
          }),
        );
      }
    }

    if (shellCheckOn) {
      ext.push(makeShellCheckLinter(shellCheckPath, shellCheckShell));
    }

    return ext;
  }, [
    editorLanguage,
    customKeymap,
    vimModeEnabled,
    shellCheckOn,
    shellCheckShell,
    shellCheckPath,
    blockContext,
  ]);

  const handleCopyCode = async () => {
    try {
      await navigator.clipboard.writeText(code);
      addToast({
        title: "Code copied to clipboard",
        color: "success",
        radius: "sm",
      });
    } catch (err) {
      console.error("Failed to copy to clipboard:", err);
      addToast({
        title: "Failed to copy code",
        color: "danger",
        radius: "sm",
      });
    }
  };

  const handleFocus = () => {
    setIsFocused(true);
    onFocus?.();
  };

  const handleBlur = () => {
    setIsFocused(false);
  };

  return (
    <div className="relative">
      <CodeMirror
        id={id}
        placeholder={"Write your code here..."}
        className="!pt-0 max-w-full border border-gray-300 rounded flex-grow"
        value={codeMirrorValue.value}
        editable={isEditable}
        onChange={codeMirrorValue.onChange}
        onFocus={handleFocus}
        onBlur={handleBlur}
        extensions={editorExtensions}
        basicSetup={false}
        indentWithTab={false}
        theme={themeObj}
      />
      {code.trim() && (
        <div
          className={`absolute top-[4px] right-[4px] transition-opacity duration-200 ${
            isFocused ? "opacity-100" : "opacity-50"
          }`}
        >
          <Tooltip content="Copy code">
            <Button
              onPress={handleCopyCode}
              size="sm"
              variant="light"
              isIconOnly
              className="bg-gray-100 dark:bg-gray-700 backdrop-blur-sm shadow-sm border border-gray-200 dark:border-gray-600 min-w-6 h-6"
            >
              <ClipboardIcon size={12} />
            </Button>
          </Tooltip>
        </div>
      )}
    </div>
  );
}
