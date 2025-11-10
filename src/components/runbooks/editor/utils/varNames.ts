const VALID_VAR_NAME_PATTERN = /^([a-zA-Z0-9_]*({{[\s\S]*?}})[a-zA-Z0-9_]*?)*[a-zA-Z0-9_]*$/;

export default function isValidVarName(name: string): boolean {
  return VALID_VAR_NAME_PATTERN.test(name);
}

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe("Variable name validation", () => {
    it("accepts simple valid names", () => {
      const names = ["foo", "foo_bar", "foo123", "_foobar", "ABC_DEF", "FoO_baR_123"];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(true);
      }
    });

    it("rejects names with invalid characters", () => {
      const names = [
        "foo-bar",
        "foo.bar",
        "foo bar",
        "foo@bar",
        "foo/bar",
        "var$name",
        "foo,bar",
        "foo:bar",
        "foo;bar",
        "foo!bar",
        "foo#bar",
        "foo$bar",
      ];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(false);
      }
    });

    it("accepts names with template syntax", () => {
      const names = [
        "{{var.myvar}}",
        "foo_{{context.bar}}",
        "{{thing.stuff.foo | upper}}_bar",
        "x{{foo}}y",
        "{{foo}}{{bar}}",
        "foo_{{a}}{{b}}_baz",
        "foo{{nested{{template}}}}bar",
      ];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(true);
      }
    });

    it("accepts empty string as valid (should be caught separately)", () => {
      expect(isValidVarName("")).toBe(true);
    });

    it("rejects names with templates but with extra invalid chars", () => {
      const names = [
        "-{{foo}}",
        "{{foo}}-bar",
        "foo-{{bar}}",
        "foo.{{bar}}baz",
        "foo{{bar}}!",
        "foo{{bar}} baz",
      ];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(false);
      }
    });

    it("accepts mixed alphanumeric and underscores", () => {
      const names = [
        "foo_bar_baz",
        "A1_B2_C3",
        "_leading_underscore",
        "trailing_underscore_",
        "with123numbers456",
      ];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(true);
      }
    });

    it("accepts templates with underscores and numbers inside", () => {
      const names = ["foo_{{bar_123}}_baz", "{{foo_1}}{{bar_2}}"];
      for (const name of names) {
        expect(isValidVarName(name)).toBe(true);
      }
    });
  });
}
