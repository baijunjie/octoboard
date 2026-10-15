import { webFiles, type RepositoryExample } from "./types";

/** The four client apps that complete the eight-project sample. Source is display-only. */
function client(spec: {
  id: "mobile" | "tablet" | "pos" | "kitchen";
  branch: string;
  component: string;
  domainAfter: string;
}): RepositoryExample {
  const domainBefore = "export function describeRole(value: string): string {\n  return value;\n}\n";
  return {
    id: spec.id,
    branch: spec.branch,
    files: {
      ...webFiles(spec.id, spec.component, spec.component),
      [`src/${spec.component}.tsx`]: `import { useState } from "react";\nimport { loadLabel } from "./api/client";\nimport { Panel } from "./components/Panel";\nimport { describeRole } from "./domain/role";\n\nexport function ${spec.component}() {\n  const [value, setValue] = useState("sample");\n  return (\n    <main>\n      <Panel title={describeRole(value)} />\n      <button type="button" onClick={() => setValue(loadLabel(value))}>\n        Refresh\n      </button>\n    </main>\n  );\n}\n`,
      "src/api/client.ts":
        "export function loadLabel(value: string): string {\n  return value.trim();\n}\n",
      "src/components/Panel.tsx":
        "export function Panel({ title }: { title: string }) {\n  return <section>{title}</section>;\n}\n",
      "src/domain/role.ts": spec.domainAfter,
      "tests/role.test.ts":
        'import { expect, it } from "vitest";\nimport { describeRole } from "../src/domain/role";\n\nit("describes the sample role", () => {\n  expect(describeRole("sample")).toContain("sample");\n});\n',
    },
    edited: { "src/domain/role.ts": domainBefore },
    added: ["tests/role.test.ts"],
  };
}

export const mobile = client({
  id: "mobile",
  branch: "feat/mobile-redemption",
  component: "MobileApp",
  domainAfter: "export function describeRole(value: string): string {\n  const code = value.replace(/[^a-z0-9]/gi, \"\").slice(-6).toUpperCase();\n  return `Mobile redemption ${value} · ${code}`;\n}\n",
});

export const tablet = client({
  id: "tablet",
  branch: "feat/table-orders",
  component: "TabletApp",
  domainAfter: "export function describeRole(value: string): string {\n  return `Table ${value}: meal deal added, order synced`;\n}\n",
});

export const pos = client({
  id: "pos",
  branch: "feat/checkout",
  component: "PosApp",
  domainAfter: "export function describeRole(value: string): string {\n  return `Checkout ${value}: discount checked, ready to redeem`;\n}\n",
});

export const kitchen = client({
  id: "kitchen",
  branch: "feat/kitchen-tickets",
  component: "KitchenApp",
  domainAfter: "export function describeRole(value: string): string {\n  return `Ticket ${value}: sent to the pass`;\n}\n",
});
