"""Thin CLI over fastjsonschema so the benchmark's CLI tier can time it like every other tool.

fastjsonschema ships no validation command (its `python -m fastjsonschema` only prints the
generated code). Mirrors `jsonschemars validate` and `boon_validate`:

    python fastjsonschema_validate.py -s <schema> [-r <resource>]... -p <payload>

Exit codes: 0 valid, 1 invalid, 2 usage, I/O or schema error.

`fastjsonschema.compile` generates Python source for the schema and `exec`s it, choosing the draft
from `$schema` (drafts 4, 6 and 7; anything else falls back to draft 7). Every `-r` document is
served by a URI handler keyed by its root `$id`, so `$ref`s to it resolve without the network; a
`$ref` to any other remote URI fails instead of being downloaded.
"""

import json
import sys

import fastjsonschema


def usage(message: str) -> None:
    sys.stderr.write(
        f"fastjsonschema_validate: {message}\n"
        "usage: fastjsonschema_validate -s <schema> [-r <resource>]... -p <payload>\n"
    )
    sys.exit(2)


def parse_args(argv: list[str]) -> tuple[str, list[str], str]:
    schema: str | None = None
    payload: str | None = None
    resources: list[str] = []
    if len(argv) % 2:
        usage("every flag needs a value")
    for flag, value in zip(argv[::2], argv[1::2]):
        if flag == "-s":
            schema = value
        elif flag == "-p":
            payload = value
        elif flag == "-r":
            resources.append(value)
        else:
            usage(f"unknown argument {flag}")
    if schema is None or payload is None:
        usage("-s and -p are required")
    return schema, resources, payload


def read_json(path: str) -> object:
    with open(path, encoding="utf-8") as file:
        return json.load(file)


def main() -> None:
    schema_path, resource_paths, payload_path = parse_args(sys.argv[1:])
    try:
        schema = read_json(schema_path)
        documents: dict[str, object] = {}
        for path in resource_paths:
            contents = read_json(path)
            documents[contents["$id"].rstrip("#")] = contents

        def offline(uri: str) -> object:
            document = documents.get(uri.split("#", 1)[0])
            if document is None:
                raise LookupError(f"{uri} is not a registered resource (offline run)")
            return document

        handlers = {"http": offline, "https": offline}
        validate = fastjsonschema.compile(schema, handlers=handlers)
        payload = read_json(payload_path)
        validate(payload)
    except fastjsonschema.JsonSchemaValueException as error:
        sys.stderr.write(f"invalid: {error.message}\n")
        sys.exit(1)
    except Exception as error:  # noqa: BLE001 - any other failure is a tool error (exit 2)
        sys.stderr.write(f"fastjsonschema_validate: error: {error}\n")
        sys.exit(2)


main()
