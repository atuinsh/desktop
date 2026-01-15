import { describe, expect, test } from "vitest";
import { parseOption } from "./parseOption";

describe("parseOption", () => {
  describe("with default colon delimiter", () => {
    test("parses simple label:value pair", () => {
      const result = parseOption("Label:value");
      expect(result).toEqual({ label: "Label", value: "value", hasKeyValue: true });
    });

    test("returns same label and value when no delimiter", () => {
      const result = parseOption("just_a_value");
      expect(result).toEqual({ label: "just_a_value", value: "just_a_value", hasKeyValue: false });
    });

    test("handles value containing colons (uses first colon)", () => {
      const result = parseOption("Label:http://example.com:8080");
      expect(result).toEqual({
        label: "Label",
        value: "http://example.com:8080",
        hasKeyValue: true,
      });
    });

    test("trims whitespace from input", () => {
      const result = parseOption("  Label : value  ");
      expect(result).toEqual({ label: "Label", value: "value", hasKeyValue: true });
    });

    test("rejects delimiter at start (empty label)", () => {
      const result = parseOption(":value");
      expect(result).toEqual({ label: ":value", value: ":value", hasKeyValue: false });
    });

    test("rejects delimiter at end (empty value)", () => {
      const result = parseOption("Label:");
      expect(result).toEqual({ label: "Label:", value: "Label:", hasKeyValue: false });
    });
  });

  describe("with custom single-char delimiter", () => {
    test("parses with pipe delimiter", () => {
      const result = parseOption("Label|value", "|");
      expect(result).toEqual({ label: "Label", value: "value", hasKeyValue: true });
    });

    test("allows colons in value when using pipe delimiter", () => {
      const result = parseOption("My Label|http://example.com:8080", "|");
      expect(result).toEqual({
        label: "My Label",
        value: "http://example.com:8080",
        hasKeyValue: true,
      });
    });

    test("rejects pipe at end (empty value)", () => {
      const result = parseOption("Label|", "|");
      expect(result).toEqual({ label: "Label|", value: "Label|", hasKeyValue: false });
    });
  });

  describe("with multi-char delimiter", () => {
    test("parses with :: delimiter", () => {
      const result = parseOption("Label::value", "::");
      expect(result).toEqual({ label: "Label", value: "value", hasKeyValue: true });
    });

    test("parses with -> delimiter", () => {
      const result = parseOption("Display Name->actual_value", "->");
      expect(result).toEqual({ label: "Display Name", value: "actual_value", hasKeyValue: true });
    });

    test("rejects :: at end - boundary check for multi-char delimiter", () => {
      // This is the key test case the review bot questioned
      // String "ab::" (length 4), delimiter "::" (length 2)
      // delimiterIndex = 2
      // Check: 2 < 4 - 2 → 2 < 2 → false (correctly rejected)
      const result = parseOption("ab::", "::");
      expect(result).toEqual({ label: "ab::", value: "ab::", hasKeyValue: false });
    });

    test("accepts :: with content after - boundary check positive case", () => {
      // String "ab::c" (length 5), delimiter "::" (length 2)
      // delimiterIndex = 2
      // Check: 2 < 5 - 2 → 2 < 3 → true (correctly accepted)
      const result = parseOption("ab::c", "::");
      expect(result).toEqual({ label: "ab", value: "c", hasKeyValue: true });
    });

    test("rejects -> at end", () => {
      const result = parseOption("Label->", "->");
      expect(result).toEqual({ label: "Label->", value: "Label->", hasKeyValue: false });
    });

    test("allows single colon in value when using :: delimiter", () => {
      const result = parseOption("URL::http://example.com:8080", "::");
      expect(result).toEqual({
        label: "URL",
        value: "http://example.com:8080",
        hasKeyValue: true,
      });
    });
  });

  describe("edge cases", () => {
    test("empty string returns empty", () => {
      const result = parseOption("");
      expect(result).toEqual({ label: "", value: "", hasKeyValue: false });
    });

    test("whitespace only returns empty after trim", () => {
      const result = parseOption("   ");
      expect(result).toEqual({ label: "", value: "", hasKeyValue: false });
    });

    test("delimiter only returns delimiter (no split)", () => {
      const result = parseOption(":");
      expect(result).toEqual({ label: ":", value: ":", hasKeyValue: false });
    });

    test("multi-char delimiter only returns delimiter (no split)", () => {
      const result = parseOption("::", "::");
      expect(result).toEqual({ label: "::", value: "::", hasKeyValue: false });
    });

    test("single character label and value works", () => {
      const result = parseOption("a:b");
      expect(result).toEqual({ label: "a", value: "b", hasKeyValue: true });
    });
  });
});
