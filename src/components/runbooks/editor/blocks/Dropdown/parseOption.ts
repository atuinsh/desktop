/**
 * Parse a dropdown option string into label and value.
 * Supports configurable delimiters (default: colon).
 *
 * Examples with default ":" delimiter:
 * - "Label:value" → { label: "Label", value: "value", hasKeyValue: true }
 * - "just_a_value" → { label: "just_a_value", value: "just_a_value", hasKeyValue: false }
 * - "Label:http://example.com:8080" → uses first colon, value keeps other colons
 *
 * With custom delimiter "|":
 * - "Label|http://example.com:8080" → { label: "Label", value: "http://example.com:8080", hasKeyValue: true }
 */
export const parseOption = (option: string, delimiter: string = ":") => {
  const trimmed = option.trim();
  const delimiterIndex = trimmed.indexOf(delimiter);
  // Only split if delimiter is not at start (empty label) or end (empty value)
  // For multi-char delimiters, ensure there's content after the delimiter
  if (delimiterIndex > 0 && delimiterIndex < trimmed.length - delimiter.length) {
    const label = trimmed.substring(0, delimiterIndex).trim();
    const value = trimmed.substring(delimiterIndex + delimiter.length).trim();
    return { value, label, hasKeyValue: true };
  }
  return { value: trimmed, label: trimmed, hasKeyValue: false };
};
