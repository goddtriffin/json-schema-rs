#!/usr/bin/env node
// Thin CLI over the ajv validator so the benchmark's CLI tier can time it like every other tool.
// ajv is a library; its separate `ajv-cli` package has had no release since 2021, takes one draft
// for the whole run (`--spec`) instead of reading `$schema`, and its strict mode rejects schemas
// the spec allows. Mirrors `jsonschemars validate` and `boon_validate`:
//
//   node ajv_validate.mjs -s <schema> [-r <resource>]... -p <payload>
//
// Exit codes: 0 valid, 1 invalid, 2 usage, I/O or schema error.
//
// The draft comes from the schema's `$schema` (2020-12 when absent, as in our library and boon).
// Every `-r` document is registered under its root `$id`, so `$ref`s to it resolve without the
// network. ajv's options stay at their defaults (first error only, no format assertion), except
// `strict: false`: strict mode is ajv's own lint and rejects valid schemas (unknown keywords,
// unknown formats), which no other tool here does. The logger is silenced so warnings about
// ignored formats do not reach the terminal.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

// Each ajv class is loaded only when a schema needs it, so the run pays for one dialect's code,
// as an application that uses ajv does.
const require = createRequire(import.meta.url);

const OPTIONS = { strict: false, logger: false };

function usage(message) {
  process.stderr.write(`ajv_validate: ${message}\nusage: ajv_validate -s <schema> [-r <resource>]... -p <payload>\n`);
  process.exit(2);
}

function parseArgs(argv) {
  const args = { schema: null, payload: null, resources: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (value === undefined) usage(`${flag} needs a value`);
    if (flag === "-s") args.schema = value;
    else if (flag === "-p") args.payload = value;
    else if (flag === "-r") args.resources.push(value);
    else usage(`unknown argument ${flag}`);
    i += 1;
  }
  if (args.schema === null || args.payload === null) usage("-s and -p are required");
  return args;
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

// The ajv class for a `$schema` URI (trailing `#` ignored), or null for an unknown dialect.
function ajvFor(dialect) {
  const uri = (dialect ?? "https://json-schema.org/draft/2020-12/schema").replace(/#$/, "");
  switch (uri) {
    case "https://json-schema.org/draft/2020-12/schema":
      return new (require("ajv/dist/2020").default)(OPTIONS);
    case "https://json-schema.org/draft/2019-09/schema":
      return new (require("ajv/dist/2019").default)(OPTIONS);
    case "http://json-schema.org/draft-07/schema":
      return new (require("ajv").default)(OPTIONS);
    case "http://json-schema.org/draft-06/schema": {
      const ajv = new (require("ajv").default)(OPTIONS);
      ajv.addMetaSchema(require("ajv/dist/refs/json-schema-draft-06.json"));
      return ajv;
    }
    case "http://json-schema.org/draft-04/schema":
      return new (require("ajv-draft-04").default)(OPTIONS);
    default:
      return null;
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  let validate;
  let payload;
  try {
    const schema = readJson(args.schema);
    const dialect = typeof schema === "object" && schema !== null ? schema.$schema : undefined;
    const ajv = ajvFor(dialect);
    if (ajv === null) {
      process.stderr.write(`ajv_validate: unsupported $schema ${dialect}\n`);
      process.exit(2);
    }
    for (const resource of args.resources) {
      ajv.addSchema(readJson(resource));
    }
    validate = ajv.compile(schema);
    payload = readJson(args.payload);
  } catch (error) {
    process.stderr.write(`ajv_validate: error: ${error.message}\n`);
    process.exit(2);
  }
  if (validate(payload)) {
    process.exit(0);
  }
  process.stderr.write(`invalid: ${JSON.stringify(validate.errors)}\n`);
  process.exit(1);
}

main();
