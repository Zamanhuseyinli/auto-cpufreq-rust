{
  description = "Automatic CPU speed & power optimizer for Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Common inputs for both GUI and non-GUI builds
        commonBuildInputs = with pkgs; [
          pkg-config
          openssl
        ];

        commonNativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
        ];

        # GUI-specific dependencies
        guiBuildInputs = with pkgs; [
          gtk4
          glib
          cairo
          gdk-pixbuf
          pango
          gobject-introspection
        ];

        # Build auto-cpufreq without GUI
        auto-cpufreq-base = pkgs.rustPlatform.buildRustPackage {
          pname = "auto-cpufreq";
          version = "2.0.0-rust";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = commonNativeBuildInputs;
          buildInputs = commonBuildInputs;

          # Don't build GUI features
          buildNoDefaultFeatures = true;
          
          # Tests require root access
          doCheck = false;

          postInstall = ''
            # Install scripts
            mkdir -p $out/share/auto-cpufreq/scripts
            install -Dm755 scripts/cpufreqctl.sh $out/bin/cpufreqctl.auto-cpufreq
            install -Dm644 scripts/style.css $out/share/auto-cpufreq/scripts/style.css
            
            # Install systemd service
            mkdir -p $out/lib/systemd/system
            substitute scripts/auto-cpufreq.service $out/lib/systemd/system/auto-cpufreq.service \
              --replace "/usr/local/bin/auto-cpufreq" "$out/bin/auto-cpufreq"
            
            # Install images
            mkdir -p $out/share/pixmaps
            install -Dm644 images/icon.png $out/share/pixmaps/auto-cpufreq.png
            
            # Install polkit policy
            mkdir -p $out/share/polkit-1/actions
            substitute scripts/org.auto-cpufreq.pkexec.policy \
              $out/share/polkit-1/actions/org.auto-cpufreq.pkexec.policy \
              --replace "/usr/local/bin/auto-cpufreq" "$out/bin/auto-cpufreq"
          '';

          meta = with pkgs.lib; {
            description = "Automatic CPU speed & power optimizer for Linux";
            homepage = "https://github.com/AdnanHodzic/auto-cpufreq";
            license = licenses.lgpl3Plus;
            platforms = platforms.linux;
            maintainers = [ ];
            mainProgram = "auto-cpufreq";
          };
        };

        # Build auto-cpufreq with GUI
        auto-cpufreq-gui = pkgs.rustPlatform.buildRustPackage {
          pname = "auto-cpufreq-gui";
          version = "2.0.0-rust";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = commonNativeBuildInputs ++ [
            pkgs.wrapGAppsHook
            pkgs.gobject-introspection
          ];
          
          buildInputs = commonBuildInputs ++ guiBuildInputs;

          # Build with GUI features
          buildFeatures = [ "gui" ];
          
          # Tests require root access
          doCheck = false;

          postInstall = ''
            # Install scripts
            mkdir -p $out/share/auto-cpufreq/scripts
            install -Dm755 scripts/cpufreqctl.sh $out/bin/cpufreqctl.auto-cpufreq
            install -Dm644 scripts/style.css $out/share/auto-cpufreq/scripts/style.css
            
            # Install systemd service
            mkdir -p $out/lib/systemd/system
            substitute scripts/auto-cpufreq.service $out/lib/systemd/system/auto-cpufreq.service \
              --replace "/usr/local/bin/auto-cpufreq" "$out/bin/auto-cpufreq"
            
            # Install images
            mkdir -p $out/share/pixmaps
            mkdir -p $out/share/auto-cpufreq/images
            install -Dm644 images/icon.png $out/share/pixmaps/auto-cpufreq.png
            install -Dm644 images/icon.png $out/share/auto-cpufreq/images/icon.png
            
            # Install desktop file
            mkdir -p $out/share/applications
            substitute scripts/auto-cpufreq-gtk.desktop \
              $out/share/applications/auto-cpufreq-gtk.desktop \
              --replace "/usr/local/bin/auto-cpufreq-gtk" "$out/bin/auto-cpufreq-gtk" \
              --replace "/usr/share/pixmaps/auto-cpufreq.png" "$out/share/pixmaps/auto-cpufreq.png"
            
            # Install polkit policy
            mkdir -p $out/share/polkit-1/actions
            substitute scripts/org.auto-cpufreq.pkexec.policy \
              $out/share/polkit-1/actions/org.auto-cpufreq.pkexec.policy \
              --replace "/usr/local/bin/auto-cpufreq" "$out/bin/auto-cpufreq"
          '';

          meta = with pkgs.lib; {
            description = "Automatic CPU speed & power optimizer for Linux (with GUI)";
            homepage = "https://github.com/AdnanHodzic/auto-cpufreq";
            license = licenses.lgpl3Plus;
            platforms = platforms.linux;
            maintainers = [ ];
            mainProgram = "auto-cpufreq";
          };
        };

      in
      {
        packages = {
          default = auto-cpufreq-base;
          auto-cpufreq = auto-cpufreq-base;
          auto-cpufreq-gui = auto-cpufreq-gui;
        };

        # Development shell
        devShells.default = pkgs.mkShell {
          buildInputs = commonBuildInputs ++ guiBuildInputs ++ [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.clippy
            pkgs.rustfmt
          ];

          nativeBuildInputs = commonNativeBuildInputs ++ [
            pkgs.wrapGAppsHook
            pkgs.gobject-introspection
          ];

          shellHook = ''
            echo "auto-cpufreq development environment"
            echo "Rust version: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build                    # Build without GUI"
            echo "  cargo build --features gui     # Build with GUI"
            echo "  cargo test                     # Run tests"
            echo "  cargo clippy                   # Run linter"
            echo ""
          '';
        };
      }
    ) // {
      # NixOS module
      nixosModules.default = import ./nix/module.nix self;
    };
}
