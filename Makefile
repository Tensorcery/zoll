# What CI and a laptop run, under the same names: a change passes CI exactly
# when these pass here.

.DEFAULT_GOAL := help
.PHONY: help install fmt lint test build queries

UV := env -u VIRTUAL_ENV uv run

# The repository's own: not a roast's or a build's.
SCRIPTS = $(shell git ls-files '*.sh')
WORKFLOWS = $(wildcard .github/workflows/*.yml)

help:     ## list the targets
	@grep -hE '^[a-z-]+:+.*##' $(MAKEFILE_LIST) | sed -E 's/:+.*## /\t/'

install:  ## the tools and the git hooks: once after a clone
	uv sync
	cargo fetch --locked
	cargo install --locked cargo-deny cargo-insta
	$(UV) pre-commit install

fmt:      ## format the Rust code in place
	cargo fmt --all

lint:     ## the static checks: the CI lint job
	cargo fmt --all --check
	cargo clippy --locked --all-targets --all-features -- -D warnings
# A build of fewer languages, as a user may choose one (design.md, section 5).
	cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
	cargo deny check
	$(UV) yamllint --strict .
ifneq ($(SCRIPTS),)
	$(UV) shellcheck --external-sources $(SCRIPTS)
endif
# actionlint reads workflows only; it has nothing to read until one exists.
ifneq ($(WORKFLOWS),)
	$(UV) actionlint $(WORKFLOWS)
endif
# zizmor's online audits ask GitHub. The token is gh's own, fetched by the
# shell for zizmor alone - not for every build script cargo runs - and never
# part of the command make echoes.
	@echo '$(UV) zizmor .'
	@GH_TOKEN="$${GH_TOKEN:-$$(gh auth token 2> /dev/null)}" $(UV) zizmor .
# Not from inside a hook: pre-commit is what is running this.
ifndef PRE_COMMIT
	$(UV) pre-commit run --all-files --show-diff-on-failure
endif

test:     ## the tests; a changed snapshot fails until `cargo insta review` takes it
	cargo test --locked --all-features

queries:  ## Helix's injection queries into queries/, at the commit sync_queries pins
	cargo run --locked -p zoll_core --example sync_queries

build:    ## the wheel for this platform, into dist/
	uv build --wheel
