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

.PHONY: test
test: ## runs tests
	cargo fmt --check
	cargo check
	cargo clippy --tests
	cargo test

.PHONY: fix
fix: ## fixes the codebase
	cargo fix --allow-dirty --allow-staged
	cargo clippy --fix --allow-dirty --allow-staged

.PHONY: vendor_specs
vendor_specs: ## download and vendor all JSON Schema specs from json-schema.org and IETF
	./specs/download.sh

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing libraries to crates.io
	cargo publish --package json-schema-rs --dry-run
	cargo package --list

.PHONY: research_clone
research_clone: ## clone or update competitor repos into research/repos/<lang>/<name>/ (includes BelfordZ crate); REFRESH=1 to force re-download BelfordZ
	@./research/scripts/clone-competitors.sh

.PHONY: research_benchmark
research_benchmark: ## (stub) run benchmarks against research/benchmark/fixtures; not yet implemented
	@echo "Not implemented. Add fixture schemas to research/benchmark/fixtures/ and runner scripts, then use e.g. Hyperfine to measure each tool."
	@exit 1

.PHONY: research_harvest_tests
research_harvest_tests: ## (stub) harvest test schemas from competitor repos; not yet implemented
	@echo "Not implemented. See research/test-harvest/README.md for the plan."
	@exit 1

.PHONY: vendor_test_suite
vendor_test_suite: ## clone or update JSON Schema Test Suite into research/json-schema-test-suite/
	@./research/scripts/clone-json-schema-test-suite.sh

.PHONY: test_json_schema_suite
test_json_schema_suite: ## run the official JSON Schema Test Suite (ignored test); run make vendor_test_suite first
	cargo test --test json_schema_test_suite json_schema_test_suite -- --ignored
