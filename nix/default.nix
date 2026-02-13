{ pkgs ? import <nixpkgs> { } }:

let
  rustPlatform = pkgs.rustPlatform;
  lib = pkgs.lib;
in

rustPlatform.buildRustPackage rec {
  pname = "auto-cpufreq";
  version = "2.0.0-rust";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = with pkgs; [
    pkg-config
    rustc
    cargo
  ];

  buildInputs = with pkgs; [
    openssl
  ];

  # Don't build GUI features by default
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

  meta = with lib; {
    description = "Automatic CPU speed & power optimizer for Linux";
    homepage = "https://github.com/AdnanHodzic/auto-cpufreq";
    license = licenses.lgpl3Plus;
    platforms = platforms.linux;
    maintainers = [ ];
    mainProgram = "auto-cpufreq";
  };
}
