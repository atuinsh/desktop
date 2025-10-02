import { getTemplateVar } from "@/state/templates";
import { Some } from "@binarymuse/ts-stdlib";
import { listen } from "@tauri-apps/api/event";
import Emittery from "emittery";

export default class RunbookBus extends Emittery {
  static instances: Map<string, WeakRef<RunbookBus>> = new Map();

  static initialize() {
    listen<string>("variable-changed", async (event) => {
      const [runbook, variable] = event.payload.split(":");
      const bus = RunbookBus.get(runbook);
      const value = await getTemplateVar(runbook, variable);
      bus.emitVariableChanged(variable, value);
    });
  }

  static get(runbookId: string): RunbookBus {
    return Some(RunbookBus.instances.get(runbookId))
      .andThen((ref) => Some(ref.deref()))
      .unwrapOrElse(() => {
        const bus = new RunbookBus();
        RunbookBus.instances.set(runbookId, new WeakRef(bus));
        return bus;
      });
  }

  constructor() {
    super();
  }

  public emitVariableChanged(name: string, value: string, source?: any) {
    return this.emit("variable-changed", { name, value, source });
  }

  public onVariableChanged(callback: (name: string, value: string, source?: any) => void) {
    return this.on("variable-changed", (evt: { name: string; value: string; source?: any }) => {
      return callback(evt.name, evt.value, evt.source);
    });
  }
}
