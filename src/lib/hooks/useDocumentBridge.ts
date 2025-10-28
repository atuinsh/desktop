import { Channel, invoke } from "@tauri-apps/api/core";
import { createContext, useContext, useEffect, useState } from "react";
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
  public readonly runbookId: string;
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
    switch (message.type) {
      case "blockContextUpdate":
        this.emitter.emit(`block_context:update:${message.data.blockId}`, message.data.context);
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
    const unsub = this.emitter.on(`block_context:update:${blockId}`, (context) => {
      callback(context);
    });

    return () => {
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
