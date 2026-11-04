"""Thin CLI over python-jsonschema so the benchmark's CLI tier can time it like every other tool.

python-jsonschema's own `jsonschema` command is deprecated, so this calls the library the way
its documentation recommends. Mirrors `jsonschemars validate` and `boon_validate`:

    python jsonschema_validate.py -s <schema> [-r <resource>]... -p <payload>

Exit codes: 0 valid, 1 invalid, 2 usage, I/O or schema error.

`jsonschema.validate` picks the validator from the schema's `$schema` (the latest draft when
absent), checks the schema against its meta-schema, and reports the best error. Every `-r`
document is registered under its root `$id` in a `referencing` registry, which never retrieves
anything over the network.
"""

import json
import sys

import jsonschema
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012


def usage(message: str) -> None:
    sys.stderr.write(
        f"jsonschema_validate: {message}\n"
        "usage: jsonschema_validate -s <schema> [-r <resource>]... -p <payload>\n"
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
        registry: Registry = Registry()
        for path in resource_paths:
            contents = read_json(path)
            resource = Resource.from_contents(contents, default_specification=DRAFT202012)
            registry = registry.with_resource(contents["$id"], resource)
        payload = read_json(payload_path)
        jsonschema.validate(payload, schema, registry=registry)
    except jsonschema.ValidationError as error:
        sys.stderr.write(f"invalid: {error.message}\n")
        sys.exit(1)
    except Exception as error:  # noqa: BLE001 - any other failure is a tool error (exit 2)
        sys.stderr.write(f"jsonschema_validate: error: {error}\n")
        sys.exit(2)


main()
