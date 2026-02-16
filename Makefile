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

.PHONY: vendor-specs
vendor-specs: ## download and vendor all JSON Schema specs from json-schema.org and IETF
	./specs/download.sh

.PHONY: publish_dry_run
publish_dry_run: ## dry run of publishing libraries to crates.io
	cargo publish --package json-schema-rs --dry-run
	cargo package --list

.PHONY: research-clone
research-clone: ## clone or update competitor repos into research/repos/<lang>/<name>/
	@mkdir -p research/repos/rust
	@if [ -d research/repos/rust/Stranger6667-jsonschema-rs/.git ]; then (cd research/repos/rust/Stranger6667-jsonschema-rs && git pull); else git clone https://github.com/Stranger6667/jsonschema-rs research/repos/rust/Stranger6667-jsonschema-rs; fi
	@if [ -d research/repos/rust/Marwes-schemafy/.git ]; then (cd research/repos/rust/Marwes-schemafy && git pull); else git clone https://github.com/Marwes/schemafy research/repos/rust/Marwes-schemafy; fi
	@if [ -d research/repos/rust/oxidecomputer-typify/.git ]; then (cd research/repos/rust/oxidecomputer-typify && git pull); else git clone https://github.com/oxidecomputer/typify research/repos/rust/oxidecomputer-typify; fi

.PHONY: research-benchmark
research-benchmark: ## (stub) run benchmarks against research/benchmark/fixtures; not yet implemented
	@echo "Not implemented. Add fixture schemas to research/benchmark/fixtures/ and runner scripts, then use e.g. Hyperfine to measure each tool."
	@exit 1

.PHONY: research-harvest-tests
research-harvest-tests: ## (stub) harvest test schemas from competitor repos; not yet implemented
	@echo "Not implemented. See research/test-harvest/README.md for the plan."
	@exit 1
