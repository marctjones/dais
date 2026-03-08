.PHONY: help build up down clean logs test test-clean shell seed

# Detect docker-compose or podman-compose
COMPOSE := $(shell command -v podman-compose 2> /dev/null || command -v docker-compose 2> /dev/null)

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build container images
	$(COMPOSE) build

up: ## Start all services
	$(COMPOSE) up -d
	@echo "Waiting for services to be healthy..."
	@sleep 5
	@$(COMPOSE) ps

down: ## Stop all services
	$(COMPOSE) down

clean: ## Stop services and remove volumes (fresh start)
	$(COMPOSE) down -v
	@echo "All containers and volumes removed"

logs: ## Tail logs from all services
	$(COMPOSE) logs -f

logs-worker: ## Tail logs from a specific worker (e.g., make logs-worker WORKER=webfinger)
	$(COMPOSE) logs -f $(WORKER)

test: ## Run all tests in containers
	./scripts/test-containers.sh

test-clean: ## Clean rebuild and run all tests
	./scripts/test-containers.sh --clean

shell: ## Open a shell in the CLI container
	$(COMPOSE) exec cli /bin/bash

shell-worker: ## Open a shell in a worker container (e.g., make shell-worker WORKER=webfinger)
	$(COMPOSE) exec $(WORKER) /bin/sh

seed: ## Seed the database with test data
	./scripts/seed-container-db.sh

restart: down up ## Restart all services

rebuild: clean build up ## Clean rebuild and start

status: ## Show status of all services
	$(COMPOSE) ps

# Development workflow targets
dev-tmux: ## Start tmux-based development environment (fast iteration)
	./scripts/dev-start.sh

dev-stop: ## Stop tmux-based development
	./scripts/dev-stop.sh

# Quick test workflows
quick-test: up seed ## Quick test: start + seed + run tests
	./scripts/test-containers.sh

ci-test: clean build up seed ## CI-style test: clean build + full test suite
	./scripts/test-containers.sh --cleanup
