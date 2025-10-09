import { useCallback, useRef, useState } from "react";
import { Input, Tabs, Tab } from "@heroui/react";
import { GlobeIcon } from "lucide-react";
import { fetch } from "@tauri-apps/plugin-http";
import CodeMirror from "@uiw/react-codemirror";
import { langs } from "@uiw/codemirror-extensions-langs";

// @ts-ignore
import { useBlockNoteEditor } from "@blocknote/react";

import HttpResponse from "./components/HttpResponse";
import HttpVerbDropdown from "./components/VerbDropdown";
import RequestHeaders from "./components/RequestHeaders";
import { templateString } from "@/state/templates";
import { useStore } from "@/state/store";
import { logExecution } from "@/lib/exec_log";
import { HttpBlock as HttpBlockType, HttpVerb, HttpHeaders } from "./schema";
import { DependencySpec } from "@/lib/workflow/dependency";
import { useBlockBusRunSubscription } from "@/lib/hooks/useBlockBus";
import BlockBus from "@/lib/workflow/block_bus";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import PlayButton from "../common/PlayButton";
import Block from "../common/Block";
import { useCurrentRunbookId } from "@/context/runbook_id_context";

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

async function makeHttpRequest(
  url: string,
  verb: HttpVerb,
  body: string,
  headers: HttpHeaders,
): Promise<any> {
  try {
    const options: any = {
      method: verb,
      headers: headers,
    };

    if (verb !== HttpVerb.GET && verb !== HttpVerb.HEAD && body) {
      options.body = body;
    }

    let start = performance.now();
    const response = await fetch(url, options);
    let duration = performance.now() - start;

    const responseData = await response.text();

    return {
      status: response.status,
      statusText: response.statusText,
      headers: Object.fromEntries(response.headers),
      data: responseData,
      duration: duration,
      time: new Date(),
    };
  } catch (error) {
    throw error;
  }
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
  let editor = useBlockNoteEditor();
  const currentRunbookId = useCurrentRunbookId();
  const colorMode = useStore((state) => state.functionalColorMode);
  const [isRunning, setIsRunning] = useState<boolean>(false);
  const [response, setResponse] = useState<any | null>(null);
  const [error, setError] = useState<any | null>(null);
  const [activeTab, setActiveTab] = useState("headers");
  const elementRef = useRef<HTMLDivElement>(null);

  const formatJSON = () => {
    if (http.verb === HttpVerb.GET || http.verb === HttpVerb.HEAD) return;

    try {
      let parsed = JSON.parse(body);
      let pretty = JSON.stringify(parsed, null, 2);
      setBody(pretty);
    } catch (e) {
      // If it's not valid JSON, don't try to format it
    }
  };

  const onPlay = useCallback(async () => {
    setIsRunning(true);
    const startTime = Date.now() * 1000000; // Convert to nanoseconds

    let template = async (input: string) =>
      await templateString(http.id, input, editor.document, currentRunbookId);

    let tUrl = await template(http.url);
    let tBody = await template(body);
    let tHeaders: { [key: string]: string } = {};

    for (const [key, value] of Object.entries(http.headers)) {
      const templatedKey = await template(key);
      const templatedValue = await template(value);
      tHeaders[templatedKey] = templatedValue;
    }

    try {
      if (elementRef.current) {
        elementRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
      }

      let res = await makeHttpRequest(tUrl, http.verb, tBody, tHeaders);

      setResponse(res);
      setError(null);

      // Log the execution without including the response body
      const endTime = Date.now() * 1000000; // Convert to nanoseconds

      // Create a sanitized response object without the body data
      const logResponse = {
        status: res.status,
        statusText: res.statusText,
        headers: res.headers,
        duration: res.duration,
        time: res.time,
      };

      await logExecution(http, http.typeName, startTime, endTime, JSON.stringify(logResponse));
      BlockBus.get().blockFinished(http);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      console.error("Request failed:", errorMessage);
      setResponse(null);
      setError(err);

      // Log the error without including request/response bodies
      const endTime = Date.now() * 1000000; // Convert to nanoseconds

      await logExecution(
        http,
        http.typeName,
        startTime,
        endTime,
        JSON.stringify({ error: errorMessage }),
      );
      BlockBus.get().blockFinished(http);
    }

    setIsRunning(false);
    formatJSON();
  }, [http, editor.document, currentRunbookId, body, http.headers, http.url, http.verb]);

  useBlockBusRunSubscription(http.id, onPlay);

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
        (response || error) && (
          <HttpResponse
            response={response}
            error={error}
            colorMode={colorMode}
            dismiss={() => {
              setResponse(null);
              setError(null);
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
          isDisabled={http.verb === HttpVerb.GET}
          className="overflow-scroll"
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
