import Emittery from "emittery";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { GCEvent } from "@/rs-bindings/GCEvent";

// Event map for type-safe event handling
export interface GrandCentralEvents {
  "pty-opened": { pty_id: string; runbook: string; block: string; created_at: number };
  "pty-closed": { pty_id: string };
  "serial-execution-started": { runbook_id: string };
  "serial-execution-completed": { runbook_id: string };
  "serial-execution-cancelled": { runbook_id: string };
  "serial-execution-failed": { runbook_id: string; error: string };
  "serial-execution-paused": { runbook_id: string; block_id: string };
  "block-started": { block_id: string; runbook_id: string };
  "block-finished": { block_id: string; runbook_id: string; success: boolean };
  "block-failed": { block_id: string; runbook_id: string; error: string };
  "block-cancelled": { block_id: string; runbook_id: string };
  "ssh-certificate-load-failed": { host: string; cert_path: string; error: string };
  "ssh-certificate-expired": { host: string; cert_path: string; valid_until: string };
  "ssh-certificate-not-yet-valid": { host: string; cert_path: string; valid_from: string };
}

/**
 * Grand Central Event System
 *
 * Provides a type-safe, centralized event system for the entire application.
 * Extends Emittery to provide additional functionality for subscribing to
 * backend events via Tauri channels.
 */
export class GrandCentral extends Emittery<GrandCentralEvents> {
  private isListening = false;
  private unsubscribe?: () => void;

  constructor() {
    super();
  }

  /**
   * Start listening to events from the backend
   */
  async startListening(): Promise<void> {
    if (this.isListening) {
      return;
    }

    try {
      // Create a channel for receiving events
      const channel = new Channel<GCEvent>();

      // Set up the channel message handler
      channel.onmessage = (event: GCEvent) => {
        this.handleBackendEvent(event);
      };

      // Subscribe to the backend event stream
      await invoke("subscribe_to_events", {
        eventChannel: channel,
      });

      this.unsubscribe = () => {
        // Channel cleanup is handled automatically
      };

      this.isListening = true;
      console.log("Grand Central: Started listening to backend events");
    } catch (error) {
      console.error("Grand Central: Failed to start listening:", error);
      throw error;
    }
  }

  /**
   * Stop listening to events from the backend
   */
  async stopListening(): Promise<void> {
    if (!this.isListening) {
      return;
    }

    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = undefined;
    }

    this.isListening = false;
    console.log("Grand Central: Stopped listening to backend events");
  }

  /**
   * Handle events from the backend and emit them to frontend subscribers
   */
  private handleBackendEvent(event: GCEvent): void {
    console.log("handleBackendEvent", event);

    try {
      switch (event.type) {
        case "ptyOpened":
          this.emit("pty-opened", {
            pty_id: event.data.pid,
            runbook: event.data.runbook,
            block: event.data.block,
            created_at: Number(event.data.created_at),
          });
          break;

        case "ptyClosed":
          this.emit("pty-closed", {
            pty_id: event.data.pty_id,
          });
          break;

        case "serialExecutionStarted":
          this.emit("serial-execution-started", {
            runbook_id: event.data.runbook_id,
          });
          break;

        case "serialExecutionCompleted":
          this.emit("serial-execution-completed", {
            runbook_id: event.data.runbook_id,
          });
          break;

        case "serialExecutionCancelled":
          this.emit("serial-execution-cancelled", {
            runbook_id: event.data.runbook_id,
          });
          break;

        case "serialExecutionFailed":
          this.emit("serial-execution-failed", {
            runbook_id: event.data.runbook_id,
            error: event.data.error,
          });
          break;

        case "serialExecutionPaused":
          this.emit("serial-execution-paused", {
            runbook_id: event.data.runbook_id,
            block_id: event.data.block_id,
          });
          break;

        case "blockStarted":
          this.emit("block-started", {
            block_id: event.data.block_id,
            runbook_id: event.data.runbook_id,
          });
          break;

        case "blockFinished":
          this.emit("block-finished", {
            block_id: event.data.block_id,
            runbook_id: event.data.runbook_id,
            success: event.data.success,
          });
          break;

        case "blockFailed":
          this.emit("block-failed", {
            block_id: event.data.block_id,
            runbook_id: event.data.runbook_id,
            error: event.data.error,
          });
          break;

        case "blockCancelled":
          this.emit("block-cancelled", {
            block_id: event.data.block_id,
            runbook_id: event.data.runbook_id,
          });
          break;

        case "sshCertificateLoadFailed":
          this.emit("ssh-certificate-load-failed", {
            host: event.data.host,
            cert_path: event.data.cert_path,
            error: event.data.error,
          });
          break;

        case "sshCertificateExpired":
          this.emit("ssh-certificate-expired", {
            host: event.data.host,
            cert_path: event.data.cert_path,
            valid_until: event.data.valid_until,
          });
          break;

        case "sshCertificateNotYetValid":
          this.emit("ssh-certificate-not-yet-valid", {
            host: event.data.host,
            cert_path: event.data.cert_path,
            valid_from: event.data.valid_from,
          });
          break;

        default:
          console.warn("Grand Central: Unhandled event type:", event);
      }
    } catch (error) {
      console.error("Grand Central: Error handling backend event:", error, event);
    }
  }

  /**
   * Get the listening status
   */
  get listening(): boolean {
    return this.isListening;
  }
}

// Global instance
export const grandCentral = new GrandCentral();

// Convenience functions for PTY events
export const onPtyOpened = (handler: (data: GrandCentralEvents["pty-opened"]) => void) =>
  grandCentral.on("pty-opened", handler);

export const onPtyClosed = (handler: (data: GrandCentralEvents["pty-closed"]) => void) =>
  grandCentral.on("pty-closed", handler);

export const onSerialExecutionStarted = (
  handler: (data: GrandCentralEvents["serial-execution-started"]) => void,
) => grandCentral.on("serial-execution-started", handler);

export const onSerialExecutionCompleted = (
  handler: (data: GrandCentralEvents["serial-execution-completed"]) => void,
) => grandCentral.on("serial-execution-completed", handler);

export const onSerialExecutionCancelled = (
  handler: (data: GrandCentralEvents["serial-execution-cancelled"]) => void,
) => grandCentral.on("serial-execution-cancelled", handler);

export const onSerialExecutionFailed = (
  handler: (data: GrandCentralEvents["serial-execution-failed"]) => void,
) => grandCentral.on("serial-execution-failed", handler);

export const onSerialExecutionPaused = (
  handler: (data: GrandCentralEvents["serial-execution-paused"]) => void,
) => grandCentral.on("serial-execution-paused", handler);

export const onBlockStarted = (
  handler: (data: GrandCentralEvents["block-started"]) => void,
) => grandCentral.on("block-started", handler);

export const onBlockFinished = (
  handler: (data: GrandCentralEvents["block-finished"]) => void,
) => grandCentral.on("block-finished", handler);

export const onBlockFailed = (
  handler: (data: GrandCentralEvents["block-failed"]) => void,
) => grandCentral.on("block-failed", handler);

export const onBlockCancelled = (
  handler: (data: GrandCentralEvents["block-cancelled"]) => void,
) => grandCentral.on("block-cancelled", handler);

export const onSshCertificateLoadFailed = (
  handler: (data: GrandCentralEvents["ssh-certificate-load-failed"]) => void,
) => grandCentral.on("ssh-certificate-load-failed", handler);

export const onSshCertificateExpired = (
  handler: (data: GrandCentralEvents["ssh-certificate-expired"]) => void,
) => grandCentral.on("ssh-certificate-expired", handler);

export const onSshCertificateNotYetValid = (
  handler: (data: GrandCentralEvents["ssh-certificate-not-yet-valid"]) => void,
) => grandCentral.on("ssh-certificate-not-yet-valid", handler);
