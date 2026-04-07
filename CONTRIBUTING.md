# Contributing to ObsFS

Thank you for your interest in contributing to ObsFS!

## Getting Started

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Linux with FUSE support (for full builds)
- macOS works for `obsfs-core` and `obsfs-plugins` only

### Development Environment

We recommend using Nix for a reproducible dev environment:

```bash
# Install Nix (if not already installed)
curl -L https://nixos.org/nix/install | sh

# Enable flakes
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf

# Enter dev shell
nix develop
```

Or manually install dependencies:

```bash
# Ubuntu/Debian
sudo apt-get install libfuse-dev fuse3 pkg-config

# Then use cargo directly
cargo build
```

### Building

```bash
make build          # Debug build
make build-release  # Release build
make check          # Fast compile check
```

### Testing

```bash
make test           # Run all tests
make test-core      # Test only core crates (works on macOS)
cargo test -p obsfs-core test_name  # Run specific test
```

### Linting

```bash
make fmt            # Format code
make lint           # Run clippy
make ci             # Run all checks (fmt, lint, test)
```

## Making Changes

### Code Style

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- All public items should have doc comments (`///`)
- Comments should be in English

### Adding a New Plugin

1. Create a new directory under `crates/obsfs-plugins/src/`
2. Implement the `Plugin` trait
3. Export in `crates/obsfs-plugins/src/lib.rs`
4. Add to the plugins vector in `crates/obsfs-cli/src/main.rs`
5. Add documentation in `docs/plugins/`
6. Add tests

See `docs/development/creating-plugins.md` for a detailed tutorial.

### Commit Messages

- Use clear, descriptive commit messages
- Start with a verb (Add, Fix, Update, Remove, Refactor)
- Keep the first line under 72 characters

Examples:
```
Add GPU metrics plugin
Fix memory leak in connection tracking
Update documentation for plugin system
```

## Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run `make ci` to ensure all checks pass
5. Commit your changes
6. Push to your fork
7. Open a Pull Request

### PR Guidelines

- Describe what the PR does and why
- Link related issues
- Include tests for new functionality
- Update documentation if needed
- Keep PRs focused - one feature/fix per PR

## Reporting Issues

- Check existing issues first
- Use a clear, descriptive title
- Include steps to reproduce
- Include system information (OS, Rust version)
- Include relevant logs or error messages

## Questions?

Open an issue with the `question` label or start a discussion.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
