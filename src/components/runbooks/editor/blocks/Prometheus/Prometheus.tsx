import { useState, useEffect } from "react";
import {
  Button,
  Dropdown,
  DropdownItem,
  DropdownMenu,
  DropdownTrigger,
  ButtonGroup,
  Spinner,
  DropdownSection,
} from "@heroui/react";

import {
  ChevronDown,
  ChevronDownIcon,
  ClockIcon,
  CloudOffIcon,
  DatabaseIcon,
  FileTerminalIcon,
  LineChartIcon,
  LockIcon,
  RefreshCwIcon,
} from "lucide-react";
import CodeMirror from "@uiw/react-codemirror";
import { PromQLExtension } from "@prometheus-io/codemirror-promql";

// @ts-ignore
import { createReactBlockSpec, useBlockNoteEditor } from "@blocknote/react";

import { PromLineChart } from "./lineChart";
import { Settings } from "@/state/settings";
import { PrometheusBlock as PrometheusBlockType } from "@/lib/workflow/blocks/prometheus";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import ErrorCard from "@/lib/blocks/common/ErrorCard";
import PlayButton from "@/lib/blocks/common/PlayButton";
import Block from "@/lib/blocks/common/Block";
import { exportPropMatter, toSnakeCase } from "@/lib/utils";
import { useBlockExecution, useBlockOutput } from "@/lib/hooks/useDocumentBridge";
import { PrometheusQueryResult } from "@/rs-bindings/PrometheusQueryResult";
import MaskedInput from "@/components/MaskedInput/MaskedInput";
import { useInterval } from "usehooks-ts";

interface PromProps {
  setName: (name: string) => void;
  setQuery: (query: string) => void;
  setEndpoint: (endpoint: string) => void;
  setPeriod: (period: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
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

const autoRefreshChoices = [
  { label: "Off", value: 0 },
  { label: "1s", value: 1000 },
  { label: "5s", value: 5000 },
  { label: "10s", value: 10000 },
  { label: "30s", value: 30000 },
  { label: "1m", value: 60000 },
  { label: "2m", value: 120000 },
  { label: "5m", value: 300000 },
  { label: "10m", value: 600000 },
  { label: "30m", value: 1800000 },
];

// Note: calculateStepSize is now handled by the backend

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
  const [value, setValue] = useState<string>(prometheus.query);
  const [data, setData] = useState<any[]>([]);
  const [config, _setConfig] = useState<{}>({});
  const [timeFrame, setTimeFrame] = useState<TimeFrame>(
    timeOptions.find((t) => t.short === prometheus.period) || timeOptions[3],
  );

  const [prometheusUrl, setPrometheusUrl] = useState<string | null>(null);
  const [promExtension, setPromExtension] = useState<PromQLExtension | null>(null);

  const execution = useBlockExecution(prometheus.id);
  const isRunning = execution.isRunning;

  useBlockOutput<PrometheusQueryResult>(prometheus.id, (output) => {
    if (output.object) {
      // Backend returns PrometheusQueryResult in the object field
      const result = output.object as PrometheusQueryResult;
      setData(result.series as any[]);
    }
  });

  useInterval(
    () => {
      // let's not stack queries
      if (execution.isRunning) return;

      (async () => {
        await execution.execute();
      })();
    },
    prometheus.autoRefresh > 0 ? prometheus.autoRefresh : null,
  );

  // Set up Prometheus URL and extension for autocomplete
  useEffect(() => {
    (async () => {
      // if have passed in an endpoint via props directly, use it
      if (prometheus.endpoint) {
        setPrometheusUrl(prometheus.endpoint);
        return;
      }

      // otherwise fetch the default endpoint from settings
      let url = await Settings.runbookPrometheusUrl();
      setPrometheusUrl(url);
    })();
  }, [prometheus.endpoint]);

  useEffect(() => {
    if (!prometheusUrl) return;

    // Set up PromQL extension for autocomplete
    let promExt = new PromQLExtension().setComplete({
      remote: { url: prometheusUrl },
    });

    setPromExtension(promExt);
  }, [prometheusUrl]);

  const themeObj = useCodemirrorTheme();
  const codeMirrorValue = useCodeMirrorValue(value, (val) => {
    setValue(val);
    setQuery(val);
  });

  const addLocalVar = () => {
    let blockName = toSnakeCase(prometheus.name);

    editor.insertBlocks(
      [
        {
          // @ts-ignore
          type: "local-var",
          props: {
            name: `${blockName}`,
          },
        },
      ],
      prometheus.id,
      "before",
    );

    setEndpoint(`{{ var.${blockName} }}`);
  };

  const addScriptForUri = () => {
    let blockName = toSnakeCase(prometheus.name);

    // TODO: register custom schema type for typescript blocknote
    editor.insertBlocks(
      [
        {
          // @ts-ignore
          type: "script",
          props: {
            name: `uri for ${prometheus.name}`,
            // @ts-ignore
            outputVariable: blockName,
            outputVisible: false,
            code: `# Output the uri for ${prometheus.name}`,
          },
        },
      ],
      prometheus.id,
      "before",
    );

    setEndpoint(`{{ var.${blockName} }}`);
  };

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
          <div className="flex flex-row gap-2 w-full items-center">
            <MaskedInput
              size="sm"
              maskRegex={/(?<=:\/\/).*(?=@[^@]*$)/}
              placeholder={"protocol://user:password@host:port/db"}
              label="Endpoint"
              isRequired
              startContent={<DatabaseIcon size={18} />}
              value={prometheus.endpoint}
              onChange={(val: string) => {
                setEndpoint(val);
              }}
              disabled={!isEditable}
            />

            <Dropdown>
              <DropdownTrigger>
                <Button isIconOnly variant="flat">
                  <LockIcon size={16} />
                </Button>
              </DropdownTrigger>
              <DropdownMenu disabledKeys={["secret"]}>
                <DropdownSection title="Use a local variable or script">
                  <DropdownItem
                    key="local-var"
                    description="Local variable - not synced"
                    startContent={<CloudOffIcon size={16} />}
                    onPress={addLocalVar}
                  >
                    Variable
                  </DropdownItem>
                  <DropdownItem
                    key="template"
                    description="Shell command output"
                    startContent={<FileTerminalIcon size={16} />}
                    onPress={addScriptForUri}
                  >
                    Script
                  </DropdownItem>
                  <DropdownItem
                    key="secret"
                    description="Synchronized + encrypted secret"
                    startContent={<LockIcon size={16} />}
                  >
                    Secret
                  </DropdownItem>
                </DropdownSection>
              </DropdownMenu>
            </Dropdown>
          </div>
          <div className="w-full !max-w-full !outline-none overflow-none flex flex-row gap-2">
            <PlayButton
              eventName="runbooks.block.execute"
              eventProps={{ type: "prometheus" }}
              onPlay={async () => {
                execution.execute();
              }}
              isRunning={isRunning}
              cancellable={true}
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
      footer={
        <div className="flex justify-between p-3 border-t w-full">
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
            </ButtonGroup>
          </div>

          <ButtonGroup>
            <Dropdown showArrow>
              <DropdownTrigger>
                <Button
                  size="sm"
                  variant="flat"
                  startContent={<RefreshCwIcon size={16} />}
                  endContent={<ChevronDown size={16} />}
                >
                  Auto refresh:{" "}
                  {prometheus.autoRefresh == 0
                    ? "Off"
                    : (
                        autoRefreshChoices.find((a) => a.value == prometheus.autoRefresh) || {
                          label: "Off",
                        }
                      ).label}
                </Button>
              </DropdownTrigger>
              <DropdownMenu variant="faded" aria-label="Select time frame for chart">
                {autoRefreshChoices.map((setting) => {
                  return (
                    <DropdownItem
                      key={setting.label}
                      onPress={() => {
                        setAutoRefresh(setting.value);
                      }}
                    >
                      {setting.label}
                    </DropdownItem>
                  );
                })}
              </DropdownMenu>
            </Dropdown>
          </ButtonGroup>
        </div>
      }
    >
      <div className="min-h-64 overflow-x-scroll">
        {!prometheusUrl ? (
          <ErrorCard error="No Prometheus endpoint set" />
        ) : execution.isError ? (
          <ErrorCard error={execution.error} />
        ) : isRunning && data.length === 0 ? (
          <Spinner />
        ) : (
          <PromLineChart data={data} config={config} />
        )}
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
      autoRefresh: { default: 0 },
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

      const setAutoRefresh = (autoRefresh: number) => {
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

    // fetch the default endpoint from the old settings
    Settings.runbookPrometheusUrl().then((url) => {
      editor.insertBlocks(
        [
          {
            type: "prometheus",
            // @ts-ignore
            props: {
              name: name,
              endpoint: url,
            },
          },
        ],
        editor.getTextCursorPosition().block.id,
        "before",
      );
    });
  },
  icon: <LineChartIcon size={18} />,
  aliases: ["prom", "promql", "grafana"],
  group: "Monitor",
});
