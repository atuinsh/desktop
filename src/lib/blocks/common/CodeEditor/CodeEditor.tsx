import CodeMirror, { KeyBinding, keymap, Prec } from "@uiw/react-codemirror";
import * as themes from "@uiw/codemirror-themes-all";
import { langs } from "@uiw/codemirror-extensions-langs";
import { extensions } from "./extensions";
import { useMemo } from "react";
import { acceptCompletion, completionStatus } from "@codemirror/autocomplete";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import { indentLess, indentMore } from "@codemirror/commands";
import { vim } from "@replit/codemirror-vim";
import { useStore } from "@/state/store";


interface CodeEditorProps {
  id: string;
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
  code,
  isEditable,
  onChange,
  language,
  theme,
  keyMap,
  onFocus,
}: CodeEditorProps) {
  const vimModeEnabled = useStore((state) => state.vimModeEnabled);
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

  let editorExtensions: any[] = useMemo(() => {
    const ext = [...extensions(), customKeymap];
    if (vimModeEnabled) {
      ext.unshift(vim());
    }
    if (editorLanguage) {
      ext.push(editorLanguage);
    }
    return ext;
  }, [editorLanguage, customKeymap, vimModeEnabled]);

  return (
    <CodeMirror
      id={id}
      placeholder={"Write your code here..."}
      className="!pt-0 max-w-full border border-gray-300 rounded flex-grow"
      value={codeMirrorValue.value}
      editable={isEditable}
      onChange={codeMirrorValue.onChange}
      onFocus={onFocus}
      extensions={editorExtensions}
      basicSetup={false}
      indentWithTab={false}
      theme={themeObj}
    />
  );
}
