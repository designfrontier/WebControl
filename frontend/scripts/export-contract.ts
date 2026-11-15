import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildContract } from "../src/contract/build";

const contractDir = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "contract");
const version = (await readFile(join(contractDir, "VERSION"), "utf8")).trim();
const files = buildContract(version);

await rm(join(contractDir, "schemas"), { recursive: true, force: true });
await Promise.all(
  Object.entries(files).map(async ([path, text]) => {
    const target = join(contractDir, path);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, text);
  }),
);
console.log(`Wrote ${Object.keys(files).length} files to ${contractDir} (version ${version})`);
