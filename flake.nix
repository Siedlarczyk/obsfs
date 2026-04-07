{
  description = "ObsFS - Observability Filesystem";

  # ==========================================================================
  # INPUTS
  # ==========================================================================
  # These are the external dependencies for our flake.
  # Think of them like dependencies in Cargo.toml, but for the dev environment.

  inputs = {
    # Nixpkgs - the main package repository
    # We use nixos-unstable for latest Rust toolchain
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Rust overlay - provides latest Rust versions and easy toolchain management
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Flake-utils - helper functions for multi-system support
    flake-utils.url = "github:numtide/flake-utils";
  };

  # ==========================================================================
  # OUTPUTS
  # ==========================================================================
  # This defines what our flake provides: dev shells, packages, etc.

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        # Import nixpkgs with the Rust overlay applied
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Define the Rust toolchain we want
        # This gives us the latest stable Rust with additional components
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"      # Rust source code (for IDE support)
            "rust-analyzer" # LSP server for IDEs
            "clippy"        # Linter
            "rustfmt"       # Formatter
          ];
        };

        # Platform-specific dependencies
        # FUSE is only available on Linux
        linuxOnlyDeps = pkgs.lib.optionals pkgs.stdenv.isLinux [
          pkgs.fuse3       # FUSE library
        ];

        # Common dependencies for all platforms
        commonDeps = [
          # Rust toolchain
          rustToolchain

          # Build essentials
          pkgs.pkg-config  # Helps find system libraries

          # Development tools
          pkgs.cargo-watch   # Auto-rebuild on file changes
          pkgs.cargo-edit    # Add/remove dependencies easily
          pkgs.cargo-expand  # Expand macros for debugging
          pkgs.cargo-audit   # Security vulnerability scanner

          # Terminal support
          pkgs.ncurses       # For terminfo (fixes xterm-ghostty issues)

          # Useful utilities
          pkgs.gnumake      # Build automation
          pkgs.git
        ];

      in {
        # ======================================================================
        # DEVELOPMENT SHELL
        # ======================================================================
        # This is what you get when you run `nix develop`

        devShells.default = pkgs.mkShell {
          name = "obsfs-dev";

          # Packages available in the shell
          buildInputs = commonDeps ++ linuxOnlyDeps;

          # Environment variables
          env = {
            # Help rust-analyzer find the Rust source
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };

          # Shell hook - runs when entering the shell
          shellHook = ''
            # Fix for terminals not in terminfo (e.g., Ghostty)
            if ! infocmp "$TERM" &>/dev/null; then
              export TERM=xterm-256color
            fi

            echo ""
            echo "🔭 ObsFS Development Environment"
            echo "================================="
            echo ""
            echo "Rust:        $(rustc --version)"
            echo "Cargo:       $(cargo --version)"
            echo "Platform:    ${system}"
            ${if pkgs.stdenv.isLinux then ''
            echo "FUSE:        $(pkg-config --modversion fuse3 2>/dev/null || echo 'available')"
            echo ""
            echo "Ready to build! Run:"
            echo "  cargo build    - Build the project"
            echo "  cargo test     - Run tests"
            echo "  cargo watch -x check  - Auto-check on save"
            '' else ''
            echo "FUSE:        ⚠️  Not available on macOS"
            echo ""
            echo "Note: FUSE requires Linux. You can:"
            echo "  - Test core crate: cargo check -p obsfs-core"
            echo "  - Use OrbStack:    orb shell, then nix develop"
            ''}
            echo ""
          '';
        };

        # ======================================================================
        # PACKAGES
        # ======================================================================
        # These are buildable outputs (the actual obsfs binary)

        packages = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "obsfs";
            version = "0.1.0";
            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.fuse3 ];

            meta = {
              description = "Observability Filesystem - Observe everything as files";
              homepage = "https://github.com/Siedlarczyk/obsfs";
              license = pkgs.lib.licenses.mit;
            };
          };
        };
      });
}
