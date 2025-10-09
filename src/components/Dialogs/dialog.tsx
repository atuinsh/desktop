import { Option, Some, None } from "@binarymuse/ts-stdlib";
import { useStore } from "@/state/store";
import { DialogAction, DialogRequest } from "@/state/store/dialog_state";
import { uuidv7 } from "uuidv7";

export type DialogIcon = DialogRequest<unknown>["icon"];

class DialogBuilderBase<T> {
  protected _title: Option<string> = None;
  protected _message: Option<React.ReactNode> = None;
  protected _actions: DialogAction<T>[] = [];
  protected _icon: Option<DialogRequest<T>["icon"]> = None;

  public title(title: string): this {
    this._title = Some(title);
    return this;
  }

  public message(message: React.ReactNode): this {
    this._message = Some<React.ReactNode>(message);
    return this;
  }

  public icon(icon: DialogIcon): this {
    this._icon = Some<DialogIcon>(icon);
    return this;
  }

  protected buildInternal(): Promise<T> {
    if (this._actions.length === 0) {
      throw new Error("Actions are required");
    }

    let resolve: (value: T) => void;
    const promise = new Promise<T>((r) => {
      resolve = r;
    });

    const request: DialogRequest<T> = {
      id: uuidv7(),
      title: this._title.unwrapOr(""),
      message: this._message.unwrapOr(<></>),
      actions: this._actions,
      icon: this._icon.unwrapOr(undefined),
      resolve: resolve!,
    };

    useStore.getState().addDialog(request);

    return promise;
  }
}

export class DialogBuilder<T> extends DialogBuilderBase<T> {
  public action(action: DialogAction<T>): DialogBuilderWithActions<T> {
    this._actions.push(action);
    return new DialogBuilderWithActions(this._title, this._message, this._actions, this._icon);
  }
}

export class DialogBuilderWithActions<T> extends DialogBuilderBase<T> {
  constructor(
    title: Option<string>,
    message: Option<React.ReactNode>,
    actions: DialogAction<T>[],
    icon: Option<DialogIcon>,
  ) {
    super();
    this._title = title;
    this._message = message;
    this._actions = actions;
    this._icon = icon;
  }

  public action(action: DialogAction<T>): DialogBuilderWithActions<T> {
    this._actions.push(action);
    return this;
  }

  public build(): Promise<T> {
    return this.buildInternal();
  }
}

export function alert(title: string, message: string): Promise<void> {
  return new DialogBuilder<void>()
    .title(title)
    .message(message)
    .action({ label: "OK", value: undefined, variant: "flat" })
    .build();
}
