#!/usr/bin/env node
// Thin CLI over zod v4's JSON Schema generation so the benchmark's CLI tier can time it.
// zod is a library with no command line. The benchmark's `reverse_codegen` operation turns
// language-native models into JSON Schema; for zod the schemas are generated from each fixture by
// quicktype (`--lang typescript-zod`) before timing (never timed), and this wrapper is what is
// timed:
//
//   node zod_schema.mjs <models.ts> <exportName>
//
// It imports the generated module (Node strips its TypeScript types natively) and prints
// `JSON.stringify(z.toJSONSchema(<exportName>))` to stdout, with zod's defaults (draft 2020-12,
// output mode). The module must live where Node resolves this directory's node_modules, since it
// imports "zod" itself.
//
// Exit codes: 0 schema printed, 2 usage, import or generation error.

import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import * as z from "zod";

function usage(message) {
  process.stderr.write(`zod_schema: ${message}\nusage: zod_schema <models.ts> <exportName>\n`);
  process.exit(2);
}

async function main() {
  const argv = process.argv.slice(2);
  if (argv.length !== 2) usage("expected exactly two arguments");
  const [path, exportName] = argv;
  let schema;
  try {
    const module = await import(pathToFileURL(resolve(path)).href);
    if (!(exportName in module)) throw new Error(`${path} has no export ${exportName}`);
    schema = z.toJSONSchema(module[exportName]);
  } catch (error) {
    process.stderr.write(`zod_schema: error: ${error.message}\n`);
    process.exit(2);
  }
  process.stdout.write(`${JSON.stringify(schema)}\n`);
}

main();
