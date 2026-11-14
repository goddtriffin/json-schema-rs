$(VERBOSE).SILENT:
.DEFAULT_GOAL := help

.PHONY: help
help: # Prints out help
	@IFS=$$'\n' ; \
	help_lines=(`fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##/:/'`); \
	printf "%-30s %s\n" "target" "help" ; \
	printf "%-30s %s\n" "------" "----" ; \
	for help_line in $${help_lines[@]}; do \
			IFS=$$':' ; \
			help_split=($$help_line) ; \
			help_command=`echo $${help_split[0]} | sed -e 's/^ *//' -e 's/ *$$//'` ; \
			help_info=`echo $${help_split[2]} | sed -e 's/^ *//' -e 's/ *$$//'` ; \
			printf '\033[36m'; \
			printf "%-30s %s" $$help_command ; \
			printf '\033[0m'; \
			printf "%s\n" $$help_info; \
	done
	@echo

.PHONY: lint
lint: ## lints the codebase
	cargo fmt
	cargo fmt --manifest-path $(BENCHMARK_ISOLATED)

# The benchmark competitors built apart from the workspace (their own workspace; see its Cargo.toml).
# `make test` lints it in each of its builds; `make benchmark` runs its generated-types build,
# `make benchmark_full` every build.
BENCHMARK_ISOLATED := json_schema_rs_benchmark/isolated/Cargo.toml

# Every feature except `arbitrary-precision`, which changes how every number is parsed and
# compared (exact beyond 64 bits). `make test` runs the workspace in both builds.
FEATURES_WITHOUT_ARBITRARY_PRECISION := json-schema-rs/uuid,json-schema-rs/chrono,json-schema-rs-macro/chrono,jsonschemars/chrono,json-schema-rs/http,json-schema-rs/indexmap,json-schema-rs-macro/uuid,json-schema-rs-macro/indexmap,jsonschemars/uuid,jsonschemars/http,jsonschemars/indexmap

.PHONY: test
test: ## runs tests twice: every feature but arbitrary-precision (64-bit numbers), then all features (exact big numbers)
	cargo fmt --check
	cargo fmt --check --manifest-path $(BENCHMARK_ISOLATED)
	cargo check
	# The macro's unit tests without `indexmap`, where `unique_items = IndexSet` is a compile error.
	cargo test -p json-schema-rs-macro --lib
	cargo clippy --tests --features $(FEATURES_WITHOUT_ARBITRARY_PRECISION)
	cargo clippy --tests --all-features
	CARGO_TARGET_DIR=$(CURDIR)/target cargo clippy --all-targets --manifest-path $(BENCHMARK_ISOLATED) --features corvus
	CARGO_TARGET_DIR=$(CURDIR)/target cargo clippy --all-targets --manifest-path $(BENCHMARK_ISOLATED) --features jsonschema-macros
	# Fixture roots set to a directory without fixtures: only the built-in startup model is generated,
	# so this lints the harness code, not the generated models (whose lint cleanliness is codegen's).
	# Each model feature on its own and both together (the full run builds them together).
	BENCH_FIXTURE_ROOTS=$(CURDIR)/target/no-benchmark-fixtures CARGO_TARGET_DIR=$(CURDIR)/target cargo clippy --all-targets --manifest-path $(BENCHMARK_ISOLATED) --features reverse-codegen
	BENCH_FIXTURE_ROOTS=$(CURDIR)/target/no-benchmark-fixtures CARGO_TARGET_DIR=$(CURDIR)/target cargo clippy --all-targets --manifest-path $(BENCHMARK_ISOLATED) --features generated-types
	BENCH_FIXTURE_ROOTS=$(CURDIR)/target/no-benchmark-fixtures CARGO_TARGET_DIR=$(CURDIR)/target cargo clippy --all-targets --manifest-path $(BENCHMARK_ISOLATED) --features reverse-codegen,generated-types
	cargo test --features $(FEATURES_WITHOUT_ARBITRARY_PRECISION)
	cargo test --all-features

.PHONY: fix
fix: ## fixes the codebase
	cargo fix --allow-dirty --allow-staged
	cargo clippy --fix --allow-dirty --allow-staged

.PHONY: vendor_specs
vendor_specs: ## download and vendor all JSON Schema specs from json-schema.org and IETF
	./specs/download.sh

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing workspace crates to crates.io (order: library, macro, CLI)
	cargo publish --package json-schema-rs --dry-run
	cargo package --package json-schema-rs --list
	cargo publish --package json-schema-rs-macro --dry-run
	cargo package --package json-schema-rs-macro --list
	cargo publish --package jsonschemars --dry-run
	cargo package --package jsonschemars --list

.PHONY: research_clone
research_clone: ## clone or update competitor repos into research/repos/<lang>/<name>/ at their latest upstream version (includes BelfordZ crate, latest from crates.io); REFRESH=1 to force re-download BelfordZ
	@./research/scripts/clone-competitors.sh

.PHONY: benchmark
benchmark: ## quick benchmark for every PR that changes the library (~3-13 min): our lib + CLI on main vs the working tree, same session; prints the PR's Performance section
	@./json_schema_rs_benchmark/scripts/quick_benchmark.sh

.PHONY: benchmark_smoke
benchmark_smoke: ## benchmark correctness check, no timing (~1 min): every cell of our lib + CLI probed once from a debug build
	@./json_schema_rs_benchmark/scripts/smoke_benchmark.sh

.PHONY: benchmark_full
benchmark_full: ## full benchmark (~60 min): every tool and competitor; refreshes the committed results
	@./json_schema_rs_benchmark/scripts/run_benchmark.sh

.PHONY: differential
differential: ## run the Bowtie differential-testing harness (our validator vs competitors on the official suite) and refresh committed results; needs bowtie + Docker + make vendor_test_suite
	@./json_schema_rs_differential/scripts/run_differential.sh

.PHONY: research_harvest_tests
research_harvest_tests: ## (stub) harvest test schemas from competitor repos; not yet implemented
	@echo "Not implemented. See research/test-harvest/README.md for the plan."
	@exit 1

.PHONY: vendor_test_suite
vendor_test_suite: ## clone the JSON Schema Test Suite at its pinned commit into research/json-schema-test-suite/
	@./research/scripts/clone-json-schema-test-suite.sh

.PHONY: test_json_schema_suite
test_json_schema_suite: ## run only the official JSON Schema Test Suite gate in both builds (also part of make test); run make vendor_test_suite first
	cargo test --features $(FEATURES_WITHOUT_ARBITRARY_PRECISION) --test json_schema_test_suite
	cargo test --all-features --test json_schema_test_suite

.PHONY: update_suite_xfail
update_suite_xfail: ## rewrite json_schema_rs/tests/suite_xfail.json (and suite_xfail_arbitrary_precision.json) to list exactly the suite tests that fail today
	JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE=1 cargo test --features $(FEATURES_WITHOUT_ARBITRARY_PRECISION) --test json_schema_test_suite
	JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE=1 cargo test --all-features --test json_schema_test_suite
