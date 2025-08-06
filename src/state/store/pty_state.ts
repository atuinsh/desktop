import { invoke } from "@tauri-apps/api/core";
import { platform } from "@tauri-apps/plugin-os";
import { FitAddon } from "@xterm/addon-fit";
import { IDisposable, Terminal } from "@xterm/xterm";
import { Settings } from "../settings";
import { WebglAddon } from "@xterm/addon-webgl";
import Logger from "@/lib/logger";
import { StateCreator } from "zustand";
import { templateString } from "../templates";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import Emittery from "emittery";

const logger = new Logger("PtyStore");
const endMarkerRegex = /\x1b\]633;ATUIN_COMMAND_END;(\d+)\x1b\\/;

export class TerminalData extends Emittery {
  channel: any;
  terminal: Terminal;
  fitAddon: FitAddon;
  pty: string;

  disposeResize: IDisposable;
  disposeOnData: IDisposable;
  unlisten: UnlistenFn | null;
  channelUnsubscribe: (() => void) | null;

  // Execution state - TerminalData is now the single source of truth
  isRunning: boolean = false;
  exitCode: number | null = null;
  executionId: string | null = null;
  commandDuration: number | null = null;
  startTime: number | null = null;

  constructor(pty: string, terminal: Terminal, fit: FitAddon) {
    super();

    this.terminal = terminal;
    this.fitAddon = fit;
    this.pty = pty;
    this.startTime = null;
    this.unlisten = null;
    this.channelUnsubscribe = null;

    this.disposeResize = this.terminal.onResize((e) => this.onResize(e));
    this.disposeOnData = this.terminal.onData((e) => this.onData(e));
  }

  handleBlockOutput(channel: any) {
    this.channel = channel;

    if (this.channelUnsubscribe) {
      this.channelUnsubscribe();
    }

    console.log("TerminalData: Setting up BlockOutput channel handler for pty:", this.pty);

    channel.onmessage = (data: any) => {
      console.log("TerminalData: Received BlockOutput:", data);

      // Handle binary data - write directly to terminal
      if (data.binary) {
        console.log("TerminalData: Writing binary data to terminal, length:", data.binary.length);
        const bytes = new Uint8Array(data.binary);
        this.terminal.write(bytes);
      }

      // Handle lifecycle events and update execution state
      if (data.lifecycle) {
        console.log("TerminalData: Handling lifecycle event:", data.lifecycle.type);
        switch (data.lifecycle.type) {
          case 'started':
            this.isRunning = true;
            this.startTime = performance.now();
            this.emit("execution_started");
            this.emit("command_start"); // Keep for backward compatibility
            break;
          case 'finished':
            this.isRunning = false;
            if (data.lifecycle.data) {
              this.exitCode = data.lifecycle.data.exit_code || 0;
            }
            if (this.startTime) {
              this.commandDuration = performance.now() - this.startTime;
              this.startTime = null;
            }
            this.emit("execution_finished", { 
              exitCode: this.exitCode, 
              duration: this.commandDuration 
            });
            this.emit("command_end", { 
              exitCode: this.exitCode, 
              duration: this.commandDuration 
            }); // Keep for backward compatibility
            break;
          case 'cancelled':
            this.isRunning = false;
            this.emit("execution_cancelled");
            break;
          case 'error':
            this.isRunning = false;
            if (data.lifecycle.data) {
              this.emit("execution_error", { message: data.lifecycle.data.message });
            }
            break;
        }
      }
    };

    this.channelUnsubscribe = () => {
      this.channel.onmessage = null;
    };
  }

  setExecutionId(executionId: string) {
    this.executionId = executionId;
  }

  async listen() {
    this.unlisten = await listen(`pty-${this.pty}`, (event: any) => {
      if (event.payload.indexOf("ATUIN_COMMAND_START") >= 0) {
        this.emit("command_start");
        this.startTime = performance.now();
      }

      const endMatch = endMarkerRegex.exec(event.payload);

      let duration = null;
      if (endMatch) {
        if (this.startTime) {
          duration = performance.now() - this.startTime;
          this.startTime = null;
        }

        this.emit("command_end", { exitCode: parseInt(endMatch[1], 10), duration: duration || 0 });
      }

      this.terminal.write(event.payload);
    });
  }

  async onData(event: any) {
    logger.debug("onData", event);
    await invoke("pty_write", { pid: this.pty, data: event });
  }

  async onResize(size: { cols: number; rows: number }) {
    if (!this || !this.pty) return;
    await invoke("pty_resize", {
      pid: this.pty,
      cols: size.cols,
      rows: size.rows,
    });
  }

  async write(block_id: string, data: string, doc: any, runbook: string | null) {
    // Template the string before we execute it
    logger.debug(`templating ${runbook} with doc`, doc);
    let templated = await templateString(block_id, data, doc, runbook);

    let isWindows = platform() == "windows";
    let cmdEnd = isWindows ? "\r\n" : "\n";
    let val = !templated.endsWith("\n") ? templated + cmdEnd : templated;

    await invoke("pty_write", { pid: this.pty, data: val });
  }

  dispose() {
    this.disposeResize.dispose();
    this.disposeOnData.dispose();
    this.terminal.dispose();

    if (this.unlisten) {
      this.unlisten();
      this.unlisten = null;
    }

    if (this.channelUnsubscribe) {
      this.channelUnsubscribe();
      this.channelUnsubscribe = null;
    }
  }

  relisten(pty: string) {
    this.pty = pty;

    this.dispose();

    this.disposeResize = this.terminal.onResize(this.onResize);
    this.disposeOnData = this.terminal.onData(this.onData);
  }
}

export interface RunbookPtyInfo {
  id: string;
  block: string;
}

export interface AtuinPtyState {
  terminals: { [pty: string]: TerminalData };

  setPtyTerm: (pty: string, terminal: any) => void;
  newPtyTerm: (pty: string, channel?: any) => Promise<TerminalData>;
  cleanupPtyTerm: (pty: string) => void;
}

export const persistPtyKeys: (keyof AtuinPtyState)[] = [];

export const createPtyState: StateCreator<AtuinPtyState> = (set, get, _store): AtuinPtyState => ({
  terminals: {},

  setPtyTerm: (pty: string, terminal: TerminalData) => {
    set({
      terminals: { ...get().terminals, [pty]: terminal },
    });
  },

  cleanupPtyTerm: (pty: string) => {
    set((state: AtuinPtyState) => {
      const terminals = Object.keys(state.terminals).reduce(
        (terms: { [pty: string]: TerminalData }, key) => {
          if (key !== pty) {
            terms[key] = state.terminals[key];
          }
          return terms;
        },
        {},
      );

      return { terminals };
    });
  },

  newPtyTerm: async (pty: string, channel?: any) => {
    // FiraCode is included as part of our build
    // We could consider including a few different popular fonts, and providing a dropdown
    // for the user to select from.
    let font = (await Settings.terminalFont()) || Settings.DEFAULT_FONT;
    let fontSize = (await Settings.terminalFontSize()) || Settings.DEFAULT_FONT_SIZE;
    let gl = await Settings.terminalGL();

    let terminal = new Terminal({
      fontFamily: `${font}, monospace`,
      fontSize: fontSize,
      rescaleOverlappingGlyphs: true,
      letterSpacing: 0,
      lineHeight: 1,
    });

    // TODO: fallback to canvas, also some sort of setting to allow disabling webgl usage
    // probs fine for now though, it's widely supported. maybe issues on linux.
    if (gl) {
      // May have font issues
      terminal.loadAddon(new WebglAddon());
    }

    let fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    let td = new TerminalData(pty, terminal, fitAddon);

    set({
      terminals: { ...get().terminals, [pty]: td },
    });

    // If a channel is provided, set it up for BlockOutput handling
    if (channel) {
      td.handleBlockOutput(channel);
    } else {
      // Only listen to old PTY events if no channel is provided
      await td.listen();
    }

    return td;
  },
});
