import { Channel, invoke } from "@tauri-apps/api/core";
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { autobind } from "../decorators";
import Emittery from "emittery";
import { DocumentBridgeMessage } from "@/rs-bindings/DocumentBridgeMessage";
import { ResolvedContext } from "@/rs-bindings/ResolvedContext";
import { StreamingBlockOutput } from "@/rs-bindings/StreamingBlockOutput";
import Logger from "../logger";
import { cancelExecution, executeBlock } from "../runtime";
import { JsonValue } from "@/rs-bindings/serde_json/JsonValue";
import { handleClientPrompt } from "../runtime_prompt";

export const DocumentBridgeContext = createContext<DocumentBridge | null>(null);

export default function useDocumentBridge(): DocumentBridge | null {
  return useContext(DocumentBridgeContext);
}

export type BlockContext = {};

export type Omit<T, K> = Pick<T, Exclude<keyof T, K>>;
export type GenericBlockOutput<T = JsonValue> = Omit<StreamingBlockOutput, "object"> &
  Partial<{
    object?: T;
  }>;

export class DocumentBridge {
  public readonly runbookId: string;
  private _channel: Channel<DocumentBridgeMessage>;
  private emitter: Emittery;
  public readonly logger: Logger;

  public get channel(): Channel<DocumentBridgeMessage> {
    return this._channel;
  }

  constructor(runbookId: string) {
    this.runbookId = runbookId;
    this.logger = new Logger(`DocumentBridge ${this.runbookId}`);
    this._channel = new Channel<DocumentBridgeMessage>((message) => this.onMessage(message));
    this.emitter = new Emittery();
  }

  @autobind
  private async onMessage(message: DocumentBridgeMessage) {
    this.logger.debug("Received message from document bridge", message);

    switch (message.type) {
      case "blockContextUpdate":
        this.emitter.emit(`block_context:update:${message.data.blockId}`, message.data.context);
        break;
      case "blockOutput":
        this.emitter.emit(`block_output:${message.data.blockId}`, message.data.output);
        break;
      case "blockStateChanged":
        this.emitter.emit(`block_state:changed:${message.data.blockId}`, message.data.state);
        break;
      case "clientPrompt":
        const result = await handleClientPrompt(message.data.prompt);
        await invoke("respond_to_block_prompt", {
          executionId: message.data.executionId,
          promptId: message.data.promptId,
          answer: result,
        });
        break;
      default:
        break;
    }
  }

  public getBlockContext(blockId: string): Promise<ResolvedContext> {
    return invoke("get_flattened_block_context", {
      documentId: this.runbookId,
      blockId,
    });
  }

  public getBlockState<T = JsonValue>(blockId: string): Promise<T> {
    return invoke("get_block_state", {
      documentId: this.runbookId,
      blockId,
    });
  }

  public onBlockContextUpdate(blockId: string, callback: (context: ResolvedContext) => void) {
    return this.emitter.on(`block_context:update:${blockId}`, callback);
  }

  public onBlockOutput<T = any>(
    blockId: string,
    callback: (output: GenericBlockOutput<T>) => void,
  ) {
    return this.emitter.on(`block_output:${blockId}`, callback);
  }

  public onBlockStateChanged<T>(blockId: string, callback: (state: T) => void) {
    return this.emitter.on(`block_state:changed:${blockId}`, callback);
  }
}

const DEFAULT_CONTEXT: ResolvedContext = {
  variables: {},
  variablesSources: {},
  cwd: "",
  envVars: {},
  sshHost: null,
};

export function useBlockContext(blockId: string, suppressErrors: boolean = false): ResolvedContext {
  const [context, setContext] = useState<ResolvedContext | null>(null);

  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }

    documentBridge
      .getBlockContext(blockId)
      .then((context) => {
        setContext(context);
      })
      .catch((error) => {
        if (!suppressErrors) {
          throw error;
        }
      });

    return documentBridge.onBlockContextUpdate(blockId, (context) => {
      setContext(context);
    });
  }, [documentBridge, blockId]);

  return context ?? DEFAULT_CONTEXT;
}

export function useBlockOutput<T = JsonValue>(
  blockId: string,
  callback: (output: GenericBlockOutput<T>) => void,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): void {
  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }

    return documentBridge.onBlockOutput(blockId, (output) => {
      callback(output as GenericBlockOutput<T>);
    });
  }, [documentBridge, blockId, callback]);
}

export function useBlockState<T = JsonValue>(blockId: string): T | null {
  const [state, setState] = useState<T | null>(null);

  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }

    documentBridge.getBlockState(blockId).then((state) => {
      setState(state as T);
    });

    return documentBridge.onBlockStateChanged(blockId, (state) => {
      setState(state as T);
    });
  }, [documentBridge, blockId]);

  return state;
}

export type ExecutionLifecycle = "idle" | "running" | "success" | "error" | "cancelled";

export interface ClientExecutionHandle {
  isRunning: boolean;
  isStarting: boolean;
  isStopping: boolean;
  isSuccess: boolean;
  isError: boolean;
  isCancelled: boolean;
  error: string | null;
  execute: () => Promise<void>;
  cancel: () => Promise<void>;
  reset: () => void;
}

// TODO: since the state is stored locally based on messages,
// it will be lost if the tab is closed or the page is reloaded.
export function useBlockExecution(blockId: string): ClientExecutionHandle {
  const documentBridge = useDocumentBridge();

  const [lifecycle, setLifecycle] = useState<ExecutionLifecycle>("idle");
  const [error, setError] = useState<string | null>(null);
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);

  const startExecution = useCallback(async () => {
    if (!documentBridge) {
      console.error("`startExecution` called but document bridge not found");
      return;
    }
    if (lifecycle === "running") {
      documentBridge.logger.error("`startExecution` called but lifecycle is already running");
      return;
    }

    setIsStarting(true);
    setError(null);
    documentBridge.logger.info(
      `Starting execution of block ${blockId} in runbook ${documentBridge.runbookId}`,
    );

    let executionId: string | null = null;
    try {
      executionId = await executeBlock(documentBridge.runbookId, blockId);
    } catch (error) {
      setIsStarting(false);
      documentBridge.logger.warn(
        `Failed to execute block ${blockId} in runbook ${documentBridge.runbookId} (block should send a BlockOutput with lifecycle set to error)`,
        error,
      );
    }

    if (executionId) {
      documentBridge.logger.debug(
        `Execution of block ${blockId} in runbook ${documentBridge.runbookId} started with execution ID: ${executionId}`,
      );
      setExecutionId(executionId);
    } else {
      documentBridge.logger.info("`startExecution` successful but did not return an execution ID");
      setExecutionId(null);
    }
  }, [documentBridge, blockId, executionId, lifecycle]);

  const stopExecution = useCallback(async () => {
    if (!documentBridge) {
      console.error("`stopExecution` called but document bridge not found");
      return;
    }

    if (!executionId) {
      documentBridge.logger.error("`stopExecution` called but no execution ID set");
      return;
    }

    if (lifecycle !== "running") {
      documentBridge.logger.error(
        "`stopExecution` called but lifecycle is not running: ",
        lifecycle,
      );
      return;
    }

    setIsStopping(true);
    documentBridge.logger.info(
      `Cancelling execution of block ${blockId} in runbook ${documentBridge.runbookId} with execution ID: ${executionId}`,
    );
    try {
      await cancelExecution(executionId);
    } catch (error) {
      setIsStopping(false);
      documentBridge.logger.error("Failed to cancel execution", error);
    }
    setExecutionId(null);
    setError(null);
  }, [documentBridge, blockId, executionId, lifecycle]);

  const handleBlockOutput = useCallback((output: GenericBlockOutput<any>) => {
    switch (output.lifecycle?.type) {
      case "finished":
        setLifecycle("success");
        setExecutionId(null);
        setError(null);
        setIsStarting(false);
        setIsStopping(false);
        break;
      case "cancelled":
        setLifecycle("cancelled");
        setExecutionId(null);
        setError(null);
        setIsStarting(false);
        setIsStopping(false);
        break;
      case "error":
        setLifecycle("error");
        setExecutionId(null);
        setError(output.lifecycle?.data.message);
        setIsStarting(false);
        setIsStopping(false);
        break;
      case "started":
        setLifecycle("running");
        setIsStarting(false);
        if (output.lifecycle?.data) {
          setExecutionId(output.lifecycle.data);
        }
        setError(null);
        break;
      case "paused":
        // Paused is treated like success - the block completed its work
        // (which was to pause the workflow)
        setLifecycle("success");
        setExecutionId(null);
        setError(null);
        setIsStarting(false);
        setIsStopping(false);
        break;

      default:
        if (output.lifecycle !== null) {
          const x: never = output.lifecycle;
          throw new Error(`Unhandled lifecycle event: ${x}`);
        }
    }
  }, []);

  useBlockOutput(blockId, handleBlockOutput);

  return {
    isRunning: lifecycle === "running",
    isStarting,
    isStopping,
    isSuccess: lifecycle === "success",
    isError: lifecycle === "error",
    isCancelled: lifecycle === "cancelled",
    error: error ?? null,
    execute: startExecution,
    cancel: stopExecution,
    reset: () => {
      setLifecycle("idle");
      setError(null);
      setExecutionId(null);
      setIsStarting(false);
      setIsStopping(false);
    },
  };
}

export function useBlockStart(blockId: string, callback: () => void): void {
  const execution = useBlockExecution(blockId);

  useEffect(() => {
    if (execution.isRunning) {
      callback();
    }
  }, [execution.isRunning, blockId]);
}

export function useBlockStop(blockId: string, callback: () => void): void {
  const execution = useBlockExecution(blockId);
  const wasRunning = useRef(false);

  useEffect(() => {
    if (execution.isRunning) {
      wasRunning.current = true;
    } else if (wasRunning.current && !execution.isRunning) {
      callback();
      wasRunning.current = false;
    }
  }, [execution.isRunning, blockId]);
}
