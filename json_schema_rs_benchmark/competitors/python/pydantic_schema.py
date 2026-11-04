"""Thin CLI over pydantic's JSON Schema generation so the benchmark's CLI tier can time it.

pydantic is a library with no command line. The benchmark's `reverse_codegen` operation turns
language-native models into JSON Schema; for pydantic the models are generated from each fixture
by datamodel-code-generator before timing (never timed), and this wrapper is what is timed:

    python pydantic_schema.py <models.py> <ClassName>

It imports the generated module from its path, then prints
`json.dumps(<ClassName>.model_json_schema())` to stdout (pydantic's defaults: draft 2020-12,
validation mode, `$defs` references).

Exit codes: 0 schema printed, 2 usage, import or generation error.
"""

import importlib.util
import json
import sys


def usage(message: str) -> None:
    sys.stderr.write(
        f"pydantic_schema: {message}\nusage: pydantic_schema <models.py> <ClassName>\n"
    )
    sys.exit(2)


def main() -> None:
    if len(sys.argv) != 3:
        usage("expected exactly two arguments")
    path, class_name = sys.argv[1], sys.argv[2]
    try:
        spec = importlib.util.spec_from_file_location("benchmark_models", path)
        if spec is None or spec.loader is None:
            raise ImportError(f"cannot import {path}")
        module = importlib.util.module_from_spec(spec)
        # Registered before running it: pydantic resolves postponed annotations through
        # sys.modules[cls.__module__].
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        model = getattr(module, class_name)
        schema = model.model_json_schema()
    except Exception as error:  # noqa: BLE001 - any failure is a tool error (exit 2)
        sys.stderr.write(f"pydantic_schema: error: {type(error).__name__}: {error}\n")
        sys.exit(2)
    sys.stdout.write(json.dumps(schema))
    sys.stdout.write("\n")


main()
