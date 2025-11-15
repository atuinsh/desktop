import { DialogBuilder, DialogBuilderWithActions } from "@/components/Dialogs/dialog";
import { ClientPrompt } from "@/rs-bindings/ClientPrompt";
import { ClientPromptResult } from "@/rs-bindings/ClientPromptResult";
import { DialogAction } from "@/state/store/dialog_state";
import { Input, Select, SelectItem, SharedSelection, Textarea } from "@heroui/react";

export async function handleClientPrompt(prompt: ClientPrompt): Promise<ClientPromptResult> {
  let value: any = null;
  let message: any = prompt.prompt;

  if (prompt.input?.type === "string") {
    message = (
      <div className="flex flex-col gap-2 w-full">
        <p>{prompt.prompt}</p>
        <StringInput
          onValueChange={(v) => {
            value = v;
          }}
        />
      </div>
    );
  } else if (prompt.input?.type === "text") {
    message = (
      <div className="flex flex-col gap-2 w-full">
        <p>{prompt.prompt}</p>
        <TextInput
          onValueChange={(v) => {
            value = v;
          }}
        />
      </div>
    );
  } else if (prompt.input?.type === "dropdown") {
    message = (
      <div className="flex flex-col gap-2 w-full">
        <p>{prompt.prompt}</p>
        <DropdownInput
          options={prompt.input.data}
          onValueChange={(v) => {
            value = v;
          }}
        />
      </div>
    );
  }

  let builder = new DialogBuilder().title(prompt.title).message(message);
  if (prompt.icon) {
    builder = builder.icon(prompt.icon.type);
  }
  for (const option of prompt.options) {
    const options: DialogAction<string> = { label: option.label, value: option.value };
    if (option.variant) {
      options.variant = option.variant.type;
    }
    if (option.color) {
      options.color = option.color.type;
    }
    builder = builder.action(options);
  }
  const result = await (builder as DialogBuilderWithActions<string>).build();
  return {
    button: result,
    value: value,
  };
}

interface StringInputProps {
  onValueChange: (value: string) => void;
}

function StringInput(props: StringInputProps) {
  function handleValueChange(value: string) {
    props.onValueChange(value);
  }

  return (
    <div className="flex flex-col gap-2 w-full">
      <Input onValueChange={handleValueChange} />
    </div>
  );
}

interface TextInputProps {
  onValueChange: (value: string) => void;
}

function TextInput(props: TextInputProps) {
  function handleValueChange(value: string) {
    props.onValueChange(value);
  }

  return (
    <div>
      <Textarea onValueChange={handleValueChange} />
    </div>
  );
}

interface DropdownInputProps {
  options: [string, string][];
  onValueChange: (value: string | null) => void;
}

// TODO: This dropdown doesn't quite work because the popover
// that gets rendered at the bottom of the document has a z-index
// that seems to be lower than the z-index of the modal.
function DropdownInput(props: DropdownInputProps) {
  console.log(props.options);
  function handleValueChange(value: SharedSelection) {
    props.onValueChange(value.currentKey ?? null);
  }

  return (
    <div className="flex flex-col gap-2 w-full">
      <Select
        onSelectionChange={handleValueChange}
        popoverProps={{
          classNames: {
            base: "z-[999999]",
          },
        }}
      >
        {props.options.map(([display, value]) => (
          <SelectItem key={value}>{display}</SelectItem>
        ))}
      </Select>
    </div>
  );
}
