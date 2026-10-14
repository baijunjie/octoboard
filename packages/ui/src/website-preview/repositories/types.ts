/** Display-only repositories: source text is read by the real viewer, never executed. */
export interface RepositoryExample {
  id: "admin" | "merchant" | "customer" | "server";
  branch: string;
  files: Record<string, string>;
  edited: Record<string, string>;
  added: string[];
}

export function webFiles(
  name: string,
  component: string,
  module: string,
): Record<string, string> {
  return {
    "package.json":
      JSON.stringify(
        {
          name: `@bistro/${name}`,
          private: true,
          type: "module",
          scripts: {
            dev: "vite",
            build: "tsc --noEmit && vite build",
            test: "vitest run",
          },
          dependencies: {
            react: "^19.0.0",
            "react-dom": "^19.0.0",
            zod: "^4.0.0",
          },
          devDependencies: {
            vite: "^7.0.0",
            vitest: "^3.0.0",
            typescript: "^5.9.0",
            "@types/react": "^19.0.0",
            "@types/react-dom": "^19.0.0",
          },
        },
        null,
        2,
      ) + "\n",
    "tsconfig.json":
      JSON.stringify(
        {
          compilerOptions: {
            target: "ES2022",
            module: "ESNext",
            moduleResolution: "Bundler",
            jsx: "react-jsx",
            strict: true,
            noEmit: true,
            lib: ["ES2022", "DOM"],
          },
          include: ["src", "tests"],
        },
        null,
        2,
      ) + "\n",
    "vite.config.ts":
      'import { defineConfig } from "vite";\n\nexport default defineConfig({ server: { proxy: { "/api": "http://localhost:3000" } } });\n',
    "index.html":
      '<!doctype html>\n<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Bistro</title></head>\n<body><div id="root"></div><script type="module" src="/src/main.tsx"></script></body></html>\n',
    "src/main.tsx": `import { createRoot } from "react-dom/client";\nimport { ${component} } from "./${module}";\nimport "./styles.css";\n\ncreateRoot(document.getElementById("root")!).render(<${component} />);\n`,
    "src/styles.css":
      'body { margin: 0; background: #f7f8fa; color: #172032; font: 16px/1.6 system-ui; }\nmain { max-width: 52rem; margin: auto; padding: 2rem; }\nform, article { display: grid; gap: 1rem; padding: 1.5rem; background: white; border-radius: 1rem; }\nlabel { display: grid; gap: .25rem; }\nbutton, input, select { font: inherit; padding: .5rem .75rem; }\n[role="alert"] { color: #a4262c; }\n',
  };
}
