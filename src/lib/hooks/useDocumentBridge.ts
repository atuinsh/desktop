import { Channel } from "@tauri-apps/api/core";
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
  private _channel: Channel<DocumentBridgeMessage>;
  private emitter: Emittery;

  public get channel(): Channel<DocumentBridgeMessage> {
    return this._channel;
  }

  constructor() {
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
    return documentBridge.onBlockContextUpdate(blockId, (context) => {
      setContext(context);
    });
  }, [documentBridge, blockId]);

  return context ?? DEFAULT_CONTEXT;
}
