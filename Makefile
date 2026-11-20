# What CI and a laptop run, under the same names: a change passes CI exactly
# when these pass here.

.DEFAULT_GOAL := help
.PHONY: help install fmt lint test build queries

UV := env -u VIRTUAL_ENV uv run
# zizmor's online audits ask GitHub; the token is gh's own.
export GH_TOKEN ?= $(shell gh auth token 2> /dev/null)

SCRIPTS = $(shell find . -name '*.sh' -not -path './.venv/*' -not -path './target/*')
WORKFLOWS = $(wildcard .github/workflows/*.yml)

help:     ## list the targets
	@grep -hE '^[a-z-]+:+.*##' $(MAKEFILE_LIST) | sed -E 's/:+.*## /\t/'

install:  ## the tools and the git hooks: once after a clone
	uv sync
	cargo fetch --locked
	cargo install --locked cargo-deny
	$(UV) pre-commit install

fmt:      ## format the Rust code in place
	cargo fmt --all

lint:     ## the static checks: the CI lint job
	cargo fmt --all --check
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo deny check
	$(UV) yamllint --strict .
ifneq ($(SCRIPTS),)
	$(UV) shellcheck --external-sources $(SCRIPTS)
endif
# actionlint reads workflows only; it has nothing to read until one exists.
ifneq ($(WORKFLOWS),)
	$(UV) actionlint $(WORKFLOWS)
endif
	$(UV) zizmor .
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
