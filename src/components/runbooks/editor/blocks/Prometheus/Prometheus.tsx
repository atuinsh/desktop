import { useState, useCallback, useEffect } from "react";
import {
  Button,
  Dropdown,
  DropdownItem,
  DropdownMenu,
  DropdownTrigger,
  ButtonGroup,
  Spinner,
  Switch,
} from "@heroui/react";
import { ChevronDownIcon, ClockIcon, LineChartIcon } from "lucide-react";
import CodeMirror from "@uiw/react-codemirror";
import { PromQLExtension } from "@prometheus-io/codemirror-promql";
import { useInterval } from "usehooks-ts";
import { invoke, Channel } from "@tauri-apps/api/core";

// @ts-ignore
import { createReactBlockSpec, useBlockNoteEditor } from "@blocknote/react";

import { PromLineChart } from "./lineChart";
import PromSettings, { PrometheusConfig } from "./promSettings";
import { Settings } from "@/state/settings";
import { useStore } from "@/state/store";
import { PrometheusBlock as PrometheusBlockType } from "@/lib/workflow/blocks/prometheus";
import { DependencySpec } from "@/lib/workflow/dependency";
import { useBlockBusRunSubscription } from "@/lib/hooks/useBlockBus";
import BlockBus from "@/lib/workflow/block_bus";
import track_event from "@/tracking";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import ErrorCard from "@/lib/blocks/common/ErrorCard";
import PlayButton from "@/lib/blocks/common/PlayButton";
import Block from "@/lib/blocks/common/Block";
import { exportPropMatter } from "@/lib/utils";
import { BlockOutput } from "@/rs-bindings/BlockOutput";

interface PromProps {
  setName: (name: string) => void;
  setQuery: (query: string) => void;
  setEndpoint: (endpoint: string) => void;
  setPeriod: (period: string) => void;
  setAutoRefresh: (autoRefresh: boolean) => void;
  setDependency: (dependency: DependencySpec) => void;

  isEditable: boolean;
  prometheus: PrometheusBlockType;
}

interface TimeFrame {
  name: string;
  seconds: number;
  short: string;
}

const timeOptions: TimeFrame[] = [
  { name: "Last 5 mins", seconds: 5 * 60, short: "5m" },
  { name: "Last 15 mins", seconds: 15 * 60, short: "15m" },
  { name: "Last 30 mins", seconds: 30 * 60, short: "30m" },
  { name: "Last 1 hr", seconds: 60 * 60, short: "1h" },
  { name: "Last 3 hrs", seconds: 3 * 60 * 60, short: "3h" },
  { name: "Last 6 hrs", seconds: 6 * 60 * 60, short: "6h" },
  { name: "Last 24 hrs", seconds: 24 * 60 * 60, short: "24h" },
  { name: "Last 2 days", seconds: 2 * 24 * 60 * 60, short: "2d" },
  { name: "Last 7 days", seconds: 7 * 24 * 60 * 60, short: "7d" },
  { name: "Last 30 days", seconds: 30 * 24 * 60 * 60, short: "30d" },
  { name: "Last 90 days", seconds: 90 * 24 * 60 * 60, short: "90d" },
  { name: "Last 180 days", seconds: 180 * 24 * 60 * 60, short: "180d" },
];



const Prometheus = ({
  prometheus,
  isEditable,
  setName,
  setQuery,
  setEndpoint,
  setPeriod,
  setAutoRefresh,
  setDependency,
}: PromProps) => {
  let editor = useBlockNoteEditor();
  const currentRunbookId = useStore((state) => state.currentRunbookId);

  const [value, setValue] = useState<string>(prometheus.query);
  const [data, setData] = useState<any[]>([]);
  const [config, _setConfig] = useState<{}>({});
  const [timeFrame, setTimeFrame] = useState<TimeFrame>(
    timeOptions.find((t) => t.short === prometheus.period) || timeOptions[3],
  );
  const [isRunning, setIsRunning] = useState<boolean>(false);

  const [prometheusUrl, setPrometheusUrl] = useState<string | null>(null);
  const [promExtension, setPromExtension] = useState<PromQLExtension | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runQuery = useCallback(
    async (onResult?: (data: any[]) => void, onError?: (error: string) => void) => {
      // Create a channel for output streaming
      const outputChannel = new Channel<BlockOutput>();

      // Set up output handler
      outputChannel.onmessage = (output: BlockOutput) => {
        console.log("Prometheus output:", output);

        // Handle lifecycle events
        if (output.lifecycle) {
          switch (output.lifecycle.type) {
            case "started":
              console.log("Prometheus execution started");
              setIsRunning(true);
              break;
            case "finished":
              console.log(`Prometheus execution finished, success: ${output.lifecycle.data.success}`);
              setIsRunning(false);
              if (output.lifecycle.data.success) {
                setError(null);
              }
              break;
            case "cancelled":
              console.log("Prometheus execution was cancelled");
              setIsRunning(false);
              break;
            case "error":
              console.error("Prometheus execution error:", output.lifecycle.data.message);
              setIsRunning(false);
              setError(output.lifecycle.data.message);
              if (onError) onError(output.lifecycle.data.message);
              return;
          }
        }

        // Handle structured JSON object data (chart data)
        if (output.object && typeof output.object === "object" && output.object !== null) {
          const parsed = output.object as any;
          if (parsed.series) {
            setData(parsed.series);
            setError(null);
            if (onResult) onResult(parsed.series);
            BlockBus.get().blockFinished(prometheus);
          }
        }
      };

      try {
        setIsRunning(true);
        setError(null);
        
        // Execute the block using the generic command
        await invoke<string>("execute_block", {
          blockId: prometheus.id,
          runbookId: currentRunbookId || "",
          editorDocument: editor.document,
          outputChannel,
        });
      } catch (err: any) {
        console.error("Failed to execute Prometheus query:", err);
        setIsRunning(false);
        setError(err.message || "Failed to execute query");
        if (onError) onError(err.message || "Failed to execute query");
      }
    },
    [prometheus.id, editor.document, currentRunbookId],
  );

  useBlockBusRunSubscription(prometheus.id, () => runQuery());

  // Initialize prometheus URL and PromQL extension for autocompletion
  const initializePrometheus = useCallback(async () => {
    let url = prometheus.endpoint;
    if (!url) {
      url = await Settings.runbookPrometheusUrl();
    }
    
    if (url) {
      setPrometheusUrl(url);
      let promExt = new PromQLExtension().setComplete({
        remote: { url: url },
      });
      setPromExtension(promExt);
    }
  }, [prometheus.endpoint]);

  // Initialize on component mount and when endpoint changes
  useEffect(() => {
    initializePrometheus();
  }, [initializePrometheus]);

  useInterval(
    () => {
      runQuery();
    },
    prometheus.autoRefresh ? 5000 : null,
  );

  const themeObj = useCodemirrorTheme();
  const codeMirrorValue = useCodeMirrorValue(value, (val) => {
    setValue(val);
    setQuery(val);
  });

  return (
    <Block
      block={prometheus}
      hasDependency
      setDependency={setDependency}
      name={prometheus.name}
      type={"Prometheus"}
      setName={setName}
      header={
        <>
          <div className="w-full !max-w-full !outline-none overflow-none flex flex-row gap-2">
            <PlayButton
              eventName="runbooks.block.execute"
              eventProps={{ type: "prometheus" }}
              onPlay={async () => {
                runQuery();
              }}
              isRunning={isRunning}
              cancellable={false}
            />
            <CodeMirror
              placeholder={"Write your query here..."}
              className="!pt-0 max-w-full border border-gray-300 rounded flex-grow"
              value={codeMirrorValue.value}
              onChange={codeMirrorValue.onChange}
              extensions={promExtension ? [promExtension.asExtension()] : []}
              basicSetup={true}
              editable={isEditable}
              theme={themeObj}
            />
          </div>
        </>
      }
    >
      <div className="min-h-64 overflow-x-scroll">
        {!prometheusUrl ? (
          <ErrorCard error="No Prometheus endpoint set" />
        ) : isRunning && data.length === 0 ? (
          <Spinner />
        ) : error ? (
          <ErrorCard error={error} />
        ) : (
          <PromLineChart data={data} config={config} />
        )}
      </div>

      <div className="flex justify-between p-3 border-t">
        <div className="flex-row content-center items-center justify-center">
          <ButtonGroup className="mr-2">
            <Dropdown showArrow>
              <DropdownTrigger>
                <Button
                  variant="flat"
                  size="sm"
                  startContent={<ClockIcon />}
                  endContent={<ChevronDownIcon />}
                >
                  {timeFrame.short}
                </Button>
              </DropdownTrigger>
              <DropdownMenu variant="faded" aria-label="Select time frame for chart">
                {timeOptions.map((timeOption) => {
                  return (
                    <DropdownItem
                      key={timeOption.name}
                      onPress={() => {
                        setTimeFrame(timeOption);
                        setPeriod(timeOption.short);
                      }}
                    >
                      {timeOption.name}
                    </DropdownItem>
                  );
                })}
              </DropdownMenu>
            </Dropdown>

            <PromSettings
              config={{
                endpoint: prometheusUrl,
              }}
              onSave={(config: PrometheusConfig) => {
                if (config.endpoint) setPrometheusUrl(config.endpoint);

                if (config.endpoint != prometheus.endpoint) {
                  setEndpoint(config.endpoint);
                }
              }}
            />
          </ButtonGroup>
        </div>

        <Switch
          isSelected={prometheus.autoRefresh}
          size="sm"
          onValueChange={(value) => {
            setAutoRefresh(value);
          }}
        >
          <h3 className="text-sm">Auto refresh</h3>
        </Switch>
      </div>
    </Block>
  );
};

export default createReactBlockSpec(
  {
    type: "prometheus",
    propSchema: {
      name: { default: "Prometheus" },
      query: { default: "" },
      endpoint: { default: "" },
      period: { default: "" },
      autoRefresh: { default: false },
      dependency: { default: "{}" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("prometheus", block.props, ["name", "endpoint", "period"]);
      return (
        <pre lang="prometheus">
          <code>
            {propMatter}
            {block.props.query}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const setName = (name: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, name: name },
        });
      };

      const setQuery = (query: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, query: query },
        });
      };

      const setEndpoint = (endpoint: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, endpoint: endpoint },
        });
      };

      const setPeriod = (period: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, period: period },
        });
      };

      const setAutoRefresh = (autoRefresh: boolean) => {
        editor.updateBlock(block, {
          props: { ...block.props, autoRefresh: autoRefresh },
        });
      };

      const setDependency = (dependency: DependencySpec) => {
        editor.updateBlock(block, {
          props: { ...block.props, dependency: dependency.serialize() },
        });
      };

      let dependency = DependencySpec.deserialize(block.props.dependency);
      let prometheus = new PrometheusBlockType(
        block.id,
        block.props.name,
        dependency,
        block.props.query,
        block.props.endpoint,
        block.props.period,
        block.props.autoRefresh,
      );

      return (
        <Prometheus
          prometheus={prometheus}
          setName={setName}
          setQuery={setQuery}
          setEndpoint={setEndpoint}
          setPeriod={setPeriod}
          setAutoRefresh={setAutoRefresh}
          setDependency={setDependency}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);

export const insertPrometheus = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Prometheus",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "prometheus" });

    let prometheusBlocks = editor.document.filter((block: any) => block.type === "prometheus");
    let name = `Prometheus ${prometheusBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "prometheus",
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
  icon: <LineChartIcon size={18} />,
  aliases: ["prom", "promql", "grafana"],
  group: "Monitor",
});
