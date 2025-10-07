import { useEffect, useState, useMemo } from "react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import * as LanguageData from "@codemirror/language-data";
import EditorBlockType from "@/lib/workflow/blocks/editor.ts";
import track_event from "@/tracking";

import CodeMirror, { Extension } from "@uiw/react-codemirror";

import "@xterm/xterm/css/xterm.css";
import { ChevronDownIcon, CodeIcon, RefreshCwIcon, RefreshCwOffIcon, ArrowDownToLineIcon, ArrowUpToLineIcon, ClipboardIcon } from "lucide-react";
import {
  Dropdown,
  DropdownTrigger,
  Button,
  DropdownMenu,
  DropdownItem,
  Input,
  Switch,
  Tooltip,
  addToast,
} from "@heroui/react";
import EditableHeading from "@/components/EditableHeading/index.tsx";

// Extra languages
// Note that the languagedata package handles the dynamic loading of languages. This is different,
// as by importing it we have already loaded it. Not really a big deal for our use case.
import { hcl } from "codemirror-lang-hcl";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme.ts";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import Block from "@/lib/blocks/common/Block";
import { getTemplateVar, setTemplateVar } from "@/state/templates";
import { exportPropMatter, cn } from "@/lib/utils";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import RunbookBus from "@/lib/app/runbook_bus";

interface LanguageLoader {
  name: string;
  extension: () => Promise<Extension>;
}

// Seeing as we now have a mix of _custom_ languages, and also teh default languages, we use this to generate
// a cohesive list of languages with a consistent interface.
function languageLoaders(): LanguageLoader[] {
  // Build an array of language loaders, first of the default languages
  let languages = LanguageData.languages.map((lang) => {
    return {
      name: lang.name,
      extension: async () => {
        let loaded = await lang.load();
        return loaded.extension;
      },
    };
  });

  // then append the custom languages
  languages.push({
    name: "HCL",
    extension: async () => hcl(),
  });

  // sort it alphabetically by name
  return languages.toSorted((a, b) => a.name.localeCompare(b.name));
}

interface CodeBlockProps {
  name: string;
  setName: (name: string) => void;
  editor: EditorBlockType;

  onChange: (val: string) => void;
  onLanguageChange: (val: string) => void;
  onVariableNameChange: (val: string) => void;
  onSyncVariableChange: (val: boolean) => void;
  code: string;
  language: string;
  variableName: string;
  syncVariable: boolean;
  isEditable: boolean;
  onCodeMirrorFocus?: () => void;
  
  collapseCode: boolean;
  setCollapseCode: (collapse: boolean) => void;
}

const EditorBlock = ({
  onChange,
  code,
  language,
  onLanguageChange,
  onVariableNameChange,
  onSyncVariableChange,
  variableName,
  syncVariable,
  isEditable,
  name,
  setName,
  editor,
  onCodeMirrorFocus,
  collapseCode,
  setCollapseCode,
}: CodeBlockProps) => {
  const languages: LanguageLoader[] = useMemo(() => languageLoaders(), []);
  const currentRunbookId = useCurrentRunbookId();

  const codeMirrorValue = useCodeMirrorValue(code, onChange);

  const [extension, setExtension] = useState<Extension | null>(null);
  const [selected, setSelected] = useState<any | null>(
    languages.find((lang) => lang.name.toLowerCase() === language),
  );
  const [filterText, setFilterText] = useState("");
  const [isOpen, setIsOpen] = useState(false);

  const filteredItems = useMemo(() => {
    return languages
      .filter((item) => item.name.toLowerCase().includes(filterText.toLowerCase()))
      .toSorted((a, b) => a.name.localeCompare(b.name));
  }, [languages, filterText]);

  useEffect(() => {
    (async () => {
      if (!selected) return;
      onLanguageChange(selected.name.toLowerCase());
    })();
  }, [selected]);

  useEffect(() => {
    (async () => {
      let linfo = languages.find((lang) => lang.name.toLowerCase() === language);
      if (!linfo) return;

      let extension = await linfo.extension();

      setExtension(extension);
    })();
  }, [language]);

  useEffect(() => {
    if (syncVariable && currentRunbookId && variableName) {
      const bus = RunbookBus.get(currentRunbookId);
      return bus.onVariableChanged((name, value) => {
        if (name === variableName) {
          onChange(value);
        }
      });
    }
  }, [syncVariable, currentRunbookId, variableName, onChange]);

  const themeObj = useCodemirrorTheme();

  const handleCopyCode = () => {
    navigator.clipboard.writeText(code);
    addToast({
      title: "Code copied to clipboard",
      color: "success",
      radius: "sm",
    });
  };

  return (
    <Block
      name={name}
      block={editor}
      type={editor.typeName}
      setDependency={() => {}}
      setName={setName}
      inlineHeader
      header={
        <div className="flex flex-col gap-2 w-full">
          <div className="flex flex-row justify-between w-full">
            <EditableHeading initialText={name} onTextChange={setName} />
            <div className="flex flex-row gap-2 items-center">
              <Input
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck="false"
                size="sm"
                placeholder="Variable"
                value={variableName}
                onChange={(e) => onVariableNameChange(e.target.value)}
                disabled={!isEditable}
                className="font-mono text-xs"
              />
              <Tooltip content="When on, the editor content will update when the variable content changes">
                <Switch
                  isSelected={syncVariable}
                  onValueChange={(val) => onSyncVariableChange(val)}
                  thumbIcon={
                    syncVariable ? <RefreshCwIcon size={16} /> : <RefreshCwOffIcon size={16} />
                  }
                  isDisabled={!isEditable || variableName.trim() === ""}
                />
              </Tooltip>
               <Tooltip content={"Copy code"}>
                 <Button
                   onPress={handleCopyCode}
                   size="sm"
                   variant="flat"
                   isIconOnly
                 >
                   <ClipboardIcon size={20} />
                 </Button>
               </Tooltip>
              <Tooltip content={collapseCode ? "Expand code" : "Collapse code"}>
                <Button
                  onPress={() => setCollapseCode(!collapseCode)}
                  size="sm"
                  variant="flat"
                  isIconOnly
                >
                  {collapseCode ? <ArrowDownToLineIcon size={20} /> : <ArrowUpToLineIcon size={20} />}
                </Button>
              </Tooltip>
              <Dropdown
                isOpen={isOpen}
                onOpenChange={(open) => setIsOpen(open)}
                isDisabled={!isEditable}
              >
                <DropdownTrigger>
                  <Button
                    variant="flat"
                    size="sm"
                    className="capitalize min-w-[200px]"
                    endContent={<ChevronDownIcon size={16} />}
                  >
                    {selected ? selected.name : "Select a language"}
                  </Button>
                </DropdownTrigger>
                <DropdownMenu
                  aria-label="Scrollable dropdown"
                  className="max-h-[300px] overflow-y-auto"
                  items={filteredItems}
                  topContent={
                    <Input
                      autoFocus
                      autoComplete="off"
                      autoCapitalize="off"
                      autoCorrect="off"
                      spellCheck="false"
                      type="text"
                      placeholder="Filter languages..."
                      value={filterText}
                      onChange={(e) => setFilterText(e.target.value)}
                      className="w-full"
                      onClick={(e) => e.stopPropagation()}
                      disabled={!isEditable}
                    />
                  }
                >
                  {(item) => (
                    <DropdownItem
                      key={item.name}
                      onPress={() => {
                        setSelected(item);
                        setFilterText("");
                        setIsOpen(false);
                      }}
                    >
                      {item.name}
                    </DropdownItem>
                  )}
                </DropdownMenu>
              </Dropdown>
            </div>
          </div>
        </div>
      }
    >
      <div className={cn("w-full transition-all duration-300 ease-in-out relative", {
        "max-h-10 overflow-hidden": collapseCode,
      })}>
        <CodeMirror
          className="!pt-0 max-w-full border border-gray-300 rounded flex-grow max-h-1/2 overflow-scroll"
          placeholder={"Write some code..."}
          value={codeMirrorValue.value}
          readOnly={!isEditable}
          onChange={codeMirrorValue.onChange}
          onFocus={onCodeMirrorFocus}
          extensions={extension ? [extension] : []}
          basicSetup={true}
          theme={themeObj}
        />
        {collapseCode && (
          <div className="absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-white dark:from-gray-900 to-transparent pointer-events-none" />
        )}
      </div>
    </Block>
  );
};

export default createReactBlockSpec(
  {
    type: "editor",
    propSchema: {
      name: { default: "Editor" },
      code: { default: "" },
      language: { default: "" },
      variableName: { default: "" },
      syncVariable: { default: false },
      collapseCode: { default: false },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("editor", block.props, ["name", "language"]);
      return (
        <pre lang={block.props.language}>
          <code>
            {propMatter}
            {block.props.code}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const currentRunbookId = useCurrentRunbookId();

      const handleCodeMirrorFocus = () => {
        // Ensure BlockNote knows which block contains the focused CodeMirror
        editor.setTextCursorPosition(block.id, "start");
      };

      const onCodeChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, code: val },
        });

        // Store in template variable if variable name is specified
        if (block.props.variableName && currentRunbookId) {
          setTemplateVar(currentRunbookId, block.props.variableName, val).catch(console.error);
        }
      };

      const onLanguageChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, language: val },
        });
      };

      const onVariableNameChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, variableName: val },
        });
      };

      const onSyncVariableChange = async (sync: boolean) => {
        if (sync && currentRunbookId && block.props.variableName) {
          const value = await getTemplateVar(currentRunbookId, block.props.variableName);
          editor.updateBlock(block, {
            props: { ...block.props, code: value || "", syncVariable: sync },
          });
        } else {
          editor.updateBlock(block, {
            props: { ...block.props, syncVariable: sync },
          });
        }
      };

      const setName = (name: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, name: name },
        });
      };

      const setCollapseCode = (collapse: boolean) => {
        editor.updateBlock(block, {
          props: { ...block.props, collapseCode: collapse },
        });
      };

      // The editor block cannot have a dependency, because it is not runnable.
      // TODO(ellie): a more elegant way of expressing this
      let editorBlock = new EditorBlockType(
        block.id,
        block.props.name,
        DependencySpec.empty(),
        block.props.code,
        block.props.language,
      );

      return (
        <EditorBlock
          name={block.props.name}
          editor={editorBlock}
          setName={setName}
          onChange={onCodeChange}
          onLanguageChange={onLanguageChange}
          onVariableNameChange={onVariableNameChange}
          onSyncVariableChange={onSyncVariableChange}
          code={block.props.code}
          language={block.props.language}
          variableName={block.props.variableName}
          syncVariable={block.props.syncVariable}
          isEditable={editor.isEditable}
          onCodeMirrorFocus={handleCodeMirrorFocus}
          collapseCode={block.props.collapseCode}
          setCollapseCode={setCollapseCode}
        />
      );
    },
  },
);

export const insertEditor = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Editor",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "editor" });

    let editorBlocks = editor.document.filter((block: any) => block.type === "editor");
    let name = `Editor ${editorBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "editor",
          // @ts-ignore
          props: {
            name: name,
          },
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <CodeIcon size={18} />,
  group: "Misc",
  aliases: ["code"],
});
