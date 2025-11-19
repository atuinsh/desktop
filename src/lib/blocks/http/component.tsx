import { useCallback, useRef, useState } from "react";
import { Input, Tabs, Tab } from "@heroui/react";
import { GlobeIcon } from "lucide-react";
import CodeMirror from "@uiw/react-codemirror";
import { langs } from "@uiw/codemirror-extensions-langs";

// @ts-ignore
import { useBlockNoteEditor } from "@blocknote/react";

import HttpResponseComponent from "./components/HttpResponse";
import HttpVerbDropdown from "./components/VerbDropdown";
import RequestHeaders from "./components/RequestHeaders";
import { useStore } from "@/state/store";
import { HttpBlock as HttpBlockType, HttpVerb, HttpHeaders } from "./schema";
import { DependencySpec } from "@/lib/workflow/dependency";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import PlayButton from "../common/PlayButton";
import Block from "../common/Block";
import { useBlockExecution, useBlockOutput } from "@/lib/hooks/useDocumentBridge";
import { HttpResponse } from "@/rs-bindings/HttpResponse";

interface HttpProps {
  http: HttpBlockType;
  body: string;
  isEditable: boolean;
  setName: (name: string) => void;
  setUrl: (url: string) => void;
  setVerb: (verb: HttpVerb) => void;
  setBody: (body: string) => void;
  setHeaders: (headers: HttpHeaders) => void;
  setDependency: (dependency: DependencySpec) => void;
}

export const Http = ({
  http,
  setName,
  body,
  isEditable,
  setUrl,
  setVerb,
  setBody,
  setHeaders,
  setDependency,
}: HttpProps) => {
  const colorMode = useStore((state) => state.functionalColorMode);
  const [response, setResponse] = useState<any | null>(null);
  const [activeTab, setActiveTab] = useState("headers");
  const elementRef = useRef<HTMLDivElement>(null);

  // const context = useBlockContext(http.id);
  const execution = useBlockExecution(http.id);
  useBlockOutput<HttpResponse>(http.id, (output) => {
    if (output.object) {
      const response = output.object;
      setResponse(response);
    }
  });

  const isRunning = execution.isRunning;

  const onPlay = useCallback(async () => {
    execution.execute();
  }, []);

  const themeObj = useCodemirrorTheme();
  const codeMirrorValue = useCodeMirrorValue(body, setBody);

  return (
    <Block
      hasDependency
      block={http}
      setDependency={setDependency}
      name={http.name}
      type={"HTTP"}
      setName={setName}
      header={
        <div className="flex flex-row items-center gap-2 w-full" ref={elementRef}>
          <PlayButton
            eventName="runbooks.block.execute"
            eventProps={{ type: "http" }}
            isRunning={isRunning}
            onPlay={onPlay}
            cancellable={false}
          />

          <HttpVerbDropdown
            selectedVerb={http.verb}
            onVerbChange={setVerb}
            disabled={!isEditable}
          />

          <Input
            placeholder="http://localhost:8080/hello/world"
            isRequired
            startContent={<GlobeIcon size={18} />}
            value={http.url}
            onValueChange={(val) => {
              setUrl(val);
            }}
            classNames={{
              input: "text-small",
              inputWrapper: "h-8 min-h-unit-8 px-1",
            }}
            variant="bordered"
            size="sm"
            disabled={!isEditable}
          />
        </div>
      }
      footer={
        (response || execution.error) && (
          <HttpResponseComponent
            response={response}
            error={execution.error}
            colorMode={colorMode}
            dismiss={() => {
              setResponse(null);
            }}
          />
        )
      }
    >
      <Tabs
        aria-label="Options"
        selectedKey={activeTab}
        onSelectionChange={setActiveTab as any}
        variant="underlined"
      >
        <Tab key="headers" title="Headers">
          <RequestHeaders pairs={http.headers} setPairs={setHeaders} disabled={!isEditable} />
        </Tab>
        <Tab
          key="body"
          title="Body"
          isDisabled={http.verb === HttpVerb.GET || http.verb === HttpVerb.HEAD}
        >
          <CodeMirror
            placeholder={"Request Body (JSON)"}
            className="!pt-0 max-w-full border border-gray-300 rounded flex-grow text-sm max-h-96"
            value={codeMirrorValue.value}
            onChange={codeMirrorValue.onChange}
            basicSetup={true}
            extensions={[langs.json()]}
            editable={isEditable}
            theme={themeObj}
          />
        </Tab>
      </Tabs>
    </Block>
  );
};

// Pure React component - BlockNote integration moved to spec.ts
