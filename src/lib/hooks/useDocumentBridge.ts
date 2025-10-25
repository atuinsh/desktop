import { Channel } from "@tauri-apps/api/core";
import { createContext, useContext, useEffect, useMemo, useState } from "react";
import { autobind } from "../decorators";
import Emittery from "emittery";
import { DocumentBridgeMessage } from "@/rs-bindings/DocumentBridgeMessage";
import { ResolvedContext } from "@/rs-bindings/ResolvedContext";

export const DocumentBridgeContext = createContext<DocumentBridge | null>(null);

export default function useDocumentBridge(): DocumentBridge | null {
  return useContext(DocumentBridgeContext);
}

export type BlockContext = {};

export class DocumentBridge {
  private runbookId: string;
  private _channel: Channel<DocumentBridgeMessage>;
  private emitter: Emittery;

  public get channel(): Channel<DocumentBridgeMessage> {
    return this._channel;
  }

  constructor(runbookId: string) {
    this.runbookId = runbookId;
    this._channel = new Channel<DocumentBridgeMessage>((message) => this.onMessage(message));
    this.emitter = new Emittery();
  }

  @autobind
  private onMessage(message: DocumentBridgeMessage) {
    console.log("Document bridge message", message);
    switch (message.type) {
      case "blockContextUpdate":
        console.log("Emitting block context update", message.data.blockId);
        this.emitter.emit(`block_context:update:${message.data.blockId}`, message.data.context);
        break;
      default:
        break;
    }
  }

  public onBlockContextUpdate(blockId: string, callback: (context: ResolvedContext) => void) {
    console.log("On block context update", blockId);
    const unsub = this.emitter.on(`block_context:update:${blockId}`, (context) => {
      console.log("Block context update callback", context);
      callback(context);
    });

    return () => {
      console.log("Unsubscribing from block context update", blockId);
      unsub();
    };
  }
}

const DEFAULT_CONTEXT: ResolvedContext = {
  variables: {},
  cwd: "",
  envVars: {},
  sshHost: null,
};

export function useBlockContext(
  runbookId: string | null | undefined,
  blockId: string,
): ResolvedContext {
  const [context, setContext] = useState<ResolvedContext | null>(null);

  const documentBridge = useDocumentBridge();
  useEffect(() => {
    if (!documentBridge) {
      return;
    }
    console.log("Setting up block context update listener", blockId);
    return documentBridge.onBlockContextUpdate(blockId, (context) => {
      console.log("Block context update", context);
      setContext(context);
    });
  }, [documentBridge, blockId]);

  return context ?? DEFAULT_CONTEXT;
}
