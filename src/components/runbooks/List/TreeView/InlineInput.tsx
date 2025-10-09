import { useCallback, useState } from "react";

export interface InlineInputProps {
  value: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
  className?: string;
}

export default function InlineInput(props: InlineInputProps) {
  const [value, setValue] = useState(props.value);

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    switch (e.key) {
      case "Enter":
        props.onSubmit(value);
        break;
      case "Escape":
        props.onCancel();
        break;
    }
  }

  const focus = useCallback((node: HTMLInputElement) => {
    if (node) {
      node.focus();
      node.select();
    }
  }, []);

  return (
    <input
      ref={focus}
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={handleKeyDown}
      className={props.className}
    />
  );
}
