{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust toolchain
    rustc
    cargo
    rust-analyzer
    clippy
    rustfmt
    
    # Build dependencies
    pkg-config
    openssl
    
    # GUI dependencies (optional)
    gtk4
    glib
    cairo
    gdk-pixbuf
    pango
    libappindicator-gtk3
    gobject-introspection
    
    # System utilities
    dmidecode
    
    # Development tools
    git
  ];

  nativeBuildInputs = with pkgs; [
    pkg-config
    wrapGAppsHook
  ];

  shellHook = ''
    echo "==================================="
    echo "auto-cpufreq development environment"
    echo "==================================="
    echo ""
    echo "Rust version: $(rustc --version)"
    echo "Cargo version: $(cargo --version)"
    echo ""
    echo "Available commands:"
    echo "  cargo build                    # Build without GUI"
    echo "  cargo build --features gui     # Build with GUI"
    echo "  cargo run -- --help            # Run with help flag"
    echo "  cargo test                     # Run tests (requires root)"
    echo "  cargo clippy                   # Run linter"
    echo "  cargo fmt                      # Format code"
    echo ""
    echo "To test installation:"
    echo "  sudo cargo run -- --monitor"
    echo ""
  '';

  # Environment variables for build
  RUST_BACKTRACE = "1";
}
