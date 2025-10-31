import { Channel, invoke } from "@tauri-apps/api/core";
import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { autobind } from "../decorators";
import Emittery from "emittery";
import { DocumentBridgeMessage } from "@/rs-bindings/DocumentBridgeMessage";
import { ResolvedContext } from "@/rs-bindings/ResolvedContext";
import { BlockOutput } from "@/rs-bindings/BlockOutput";
import Logger from "../logger";
import { cancelExecution, executeBlock } from "../runtime";

export const DocumentBridgeContext = createContext<DocumentBridge | null>(null);

export default function useDocumentBridge(): DocumentBridge | null {
  return useContext(DocumentBridgeContext);
}

export type BlockContext = {};

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
  private onMessage(message: DocumentBridgeMessage) {
    switch (message.type) {
      case "blockContextUpdate":
        this.emitter.emit(`block_context:update:${message.data.blockId}`, message.data.context);
        break;
      case "blockOutput":
        this.emitter.emit(`block_output:${message.data.blockId}`, message.data.output);
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

  public onBlockContextUpdate(blockId: string, callback: (context: ResolvedContext) => void) {
    return this.emitter.on(`block_context:update:${blockId}`, callback);
  }

  public onBlockOutput(blockId: string, callback: (output: BlockOutput) => void) {
    return this.emitter.on(`block_output:${blockId}`, callback);
  }
}

const DEFAULT_CONTEXT: ResolvedContext = {
  variables: {},
  cwd: "",
  envVars: {},
  sshHost: null,
};

export function useBlockContext(blockId: string): ResolvedContext {
  const [context, setContext] = useState<ResolvedContext | null>(null);

  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }

    documentBridge.getBlockContext(blockId).then((context) => {
      setContext(context);
    });

    return documentBridge.onBlockContextUpdate(blockId, (context) => {
      setContext(context);
    });
  }, [documentBridge, blockId]);

  return context ?? DEFAULT_CONTEXT;
}

export function useBlockOutput(blockId: string, callback: (output: BlockOutput) => void): void {
  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }

    return documentBridge.onBlockOutput(blockId, callback);
  }, [documentBridge, blockId, callback]);
}

export type ExecutionLifecycle = "idle" | "running" | "success" | "error" | "cancelled";

export interface ClientExecutionHandle {
  isRunning: boolean;
  isSuccess: boolean;
  isError: boolean;
  isCancelled: boolean;
  execute: () => Promise<void>;
  cancel: () => Promise<void>;
}

// TODO: since the state is stored locally based on messages,
// it will be lost if the tab is closed or the page is reloaded.
export function useBlockExecution(blockId: string): ClientExecutionHandle {
  const documentBridge = useDocumentBridge();

  const [lifecycle, setLifecycle] = useState<ExecutionLifecycle>("idle");
  const [executionId, setExecutionId] = useState<string | null>(null);

  const startExecution = useCallback(async () => {
    if (!documentBridge) {
      console.error("`startExecution` called but document bridge not found");
      return;
    }
    if (lifecycle === "running") {
      documentBridge.logger.error("`startExecution` called but lifecycle is already running");
      return;
    }

    documentBridge.logger.info(
      `Starting execution of block ${blockId} in runbook ${documentBridge.runbookId}`,
    );

    const executionId = await executeBlock(documentBridge.runbookId, blockId);
    documentBridge.logger.debug(
      `Execution of block ${blockId} in runbook ${documentBridge.runbookId} started with execution ID: ${executionId}`,
    );
    setExecutionId(executionId);
  }, []);

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

    documentBridge.logger.info(
      `Cancelling execution of block ${blockId} in runbook ${documentBridge.runbookId} with execution ID: ${executionId}`,
    );
    await cancelExecution(executionId);
    setExecutionId(null);
  }, [executionId]);

  const handleBlockOutput = useCallback((output: BlockOutput) => {
    switch (output.lifecycle?.type) {
      case "finished":
        setLifecycle("success");
        break;
      case "cancelled":
        setLifecycle("cancelled");
        break;
      case "error":
        setLifecycle("error");
        break;
      case "started":
        setLifecycle("running");
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
    isSuccess: lifecycle === "success",
    isError: lifecycle === "error",
    isCancelled: lifecycle === "cancelled",
    execute: startExecution,
    cancel: stopExecution,
  };
}
