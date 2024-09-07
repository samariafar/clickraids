SHELL := $(shell command -v bash)

.SILENT:
.ONESHELL:
.DEFAULT_GOAL := help
.PHONY: help start

help: ## Show available commands
	echo -e "\nUsage:\n"
	grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-26s\033[0m %s\n", $$1, $$2}'
	echo

start: ## Start dev servers (back-end + front-end)
	mprocs "SKIP_CLIENT_BUILD=1 cargo run" "bun run dev"

%:
	:
