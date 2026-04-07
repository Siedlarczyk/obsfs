# =============================================================================
# ObsFS Makefile
# =============================================================================

.PHONY: help check build build-release watch watch-test test test-verbose \
        test-core lint fmt fmt-check ci mount unmount status doc clean setup \
        install install-service uninstall-service \
        docker docker-alpine docker-run docker-compose-up docker-compose-down

# Default command - show help
help:
	@echo "Available commands:"
	@echo "  check         - Check code compiles (fast)"
	@echo "  build         - Build debug binary"
	@echo "  build-release - Build release binary"
	@echo "  watch         - Auto-rebuild on file changes"
	@echo "  watch-test    - Auto-run tests on file changes"
	@echo "  test          - Run all tests"
	@echo "  test-verbose  - Run tests with output"
	@echo "  test-core     - Run only core tests (works on macOS)"
	@echo "  lint          - Run clippy linter"
	@echo "  fmt           - Format code"
	@echo "  fmt-check     - Check formatting without changing files"
	@echo "  ci            - Run all checks (CI simulation)"
	@echo "  mount         - Mount ObsFS at /tmp/obs (Linux only)"
	@echo "  unmount       - Unmount ObsFS from /tmp/obs"
	@echo "  status        - Show status"
	@echo "  doc           - Generate and open documentation"
	@echo "  clean         - Remove build artifacts"
	@echo "  setup         - Install development tools"
	@echo ""
	@echo "Installation:"
	@echo "  install         - Install binary to /usr/local/bin"
	@echo "  install-service - Install systemd service"
	@echo "  uninstall-service - Remove systemd service"
	@echo ""
	@echo "Docker:"
	@echo "  docker          - Build Docker image (Debian)"
	@echo "  docker-alpine   - Build Docker image (Alpine)"
	@echo "  docker-run      - Run container (privileged)"
	@echo "  docker-compose-up   - Start with docker compose"
	@echo "  docker-compose-down - Stop docker compose"

# =============================================================================
# Development
# =============================================================================

check:
	cargo check --all-targets

build:
	cargo build

build-release:
	cargo build --release

watch:
	cargo watch -x check

watch-test:
	cargo watch -x test

# =============================================================================
# Testing
# =============================================================================

test:
	cargo test

test-verbose:
	cargo test -- --nocapture

test-core:
	cargo test -p obsfs-core

# =============================================================================
# Code Quality
# =============================================================================

lint:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

ci: fmt-check lint test

# =============================================================================
# Running (Linux only)
# =============================================================================

mount:
	sudo mkdir -p /tmp/obs
	sudo cargo run -- mount /tmp/obs

unmount:
	sudo fusermount -u /tmp/obs || sudo umount /tmp/obs

status:
	cargo run -- status

# =============================================================================
# Documentation
# =============================================================================

doc:
	cargo doc --open --no-deps

# =============================================================================
# Cleanup
# =============================================================================

clean:
	cargo clean

# =============================================================================
# Setup
# =============================================================================

setup:
	cargo install cargo-watch cargo-edit cargo-expand

# =============================================================================
# Installation
# =============================================================================

install: build-release
	sudo install -m 755 target/release/obsfs /usr/local/bin/obsfs

install-service: install
	sudo mkdir -p /etc/obsfs
	sudo cp config/default.toml /etc/obsfs/config.toml
	sudo cp pkg/systemd/obsfs.service /etc/systemd/system/
	sudo systemctl daemon-reload
	@echo "Service installed. Run 'sudo systemctl enable --now obsfs' to start."

uninstall-service:
	sudo systemctl stop obsfs || true
	sudo systemctl disable obsfs || true
	sudo rm -f /etc/systemd/system/obsfs.service
	sudo systemctl daemon-reload
	@echo "Service uninstalled. Binary and config remain in place."

# =============================================================================
# Docker
# =============================================================================

docker:
	docker build -t obsfs:latest -f pkg/docker/Dockerfile .

docker-alpine:
	docker build -t obsfs:alpine -f pkg/docker/Dockerfile.alpine .

docker-run: docker
	docker run --rm -it --privileged \
		-v /proc:/host/proc:ro \
		-e RUST_LOG=info \
		obsfs:latest

docker-compose-up:
	docker compose up -d

docker-compose-down:
	docker compose down
