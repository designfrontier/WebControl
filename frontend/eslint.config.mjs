import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

/** The same style rules as project-ardent: type-only imports, and array methods instead of loops. */
export const styleRules = {
  "@typescript-eslint/consistent-type-imports": "error",
  "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
  "prefer-const": "error",
  "no-var": "error",
  "no-restricted-syntax": [
    "error",
    { selector: "ForStatement", message: "Prefer array methods over for loops." },
    { selector: "ForInStatement", message: "Prefer Object.entries/keys with array methods." },
    { selector: "ForOfStatement:not([await=true])", message: "Prefer array methods over for…of loops." },
    { selector: "WhileStatement", message: "Prefer array methods over while loops." },
    { selector: "DoWhileStatement", message: "Prefer array methods over do…while loops." },
  ],
};

export default tseslint.config(
  { ignores: ["dist/**", "coverage/**", "playwright-report/**", "test-results/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
    rules: styleRules,
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },
  {
    files: ["**/*.test.{ts,tsx}", "src/test/**", "e2e/**"],
    rules: { "@typescript-eslint/no-non-null-assertion": "off" },
  },
  prettier,
);
