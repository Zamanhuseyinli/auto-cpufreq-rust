inputs: {
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.auto-cpufreq;
  inherit (pkgs.stdenv.hostPlatform) system;
  
  # Use the package from the flake
  defaultPackage = inputs.self.packages.${system}.auto-cpufreq;
  guiPackage = inputs.self.packages.${system}.auto-cpufreq-gui;
  
  cfgFilename = "auto-cpufreq.conf";
  cfgFile = format.generate cfgFilename cfg.settings;

  inherit (lib) types;
  inherit (lib.modules) mkIf mkForce;
  inherit (lib.options) mkOption mkEnableOption;

  format = pkgs.formats.ini {};
in {
  options.programs.auto-cpufreq = {
    enable = mkEnableOption "Automatic CPU speed & power optimizer for Linux";

    enableGui = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Enable GUI support for auto-cpufreq.
        This will install auto-cpufreq-gtk and auto-cpufreq-tray binaries.
      '';
    };

    settings = mkOption {
      description = ''
        Configuration for `auto-cpufreq`.

        See its [example configuration file] for supported settings.
        [example configuration file]: https://github.com/AdnanHodzic/auto-cpufreq/blob/master/auto-cpufreq.conf-example
      '';

      default = {};
      type = types.submodule {
        freeformType = format.type;
        
        options = {
          charger = mkOption {
            type = types.submodule {
              freeformType = format.type;
              options = {
                governor = mkOption {
                  type = types.nullOr (types.enum [ "performance" "powersave" "userspace" "ondemand" "conservative" "schedutil" ]);
                  default = null;
                  description = "CPU governor to use when on AC power";
                };
                
                scaling_min_freq = mkOption {
                  type = types.nullOr types.int;
                  default = null;
                  description = "Minimum CPU frequency when on AC power (in kHz)";
                };
                
                scaling_max_freq = mkOption {
                  type = types.nullOr types.int;
                  default = null;
                  description = "Maximum CPU frequency when on AC power (in kHz)";
                };
                
                turbo = mkOption {
                  type = types.nullOr (types.enum [ "auto" "always" "never" ]);
                  default = null;
                  description = "Turbo boost setting when on AC power";
                };
              };
            };
            default = {};
            description = "Settings for when the system is plugged into AC power";
          };
          
          battery = mkOption {
            type = types.submodule {
              freeformType = format.type;
              options = {
                governor = mkOption {
                  type = types.nullOr (types.enum [ "performance" "powersave" "userspace" "ondemand" "conservative" "schedutil" ]);
                  default = null;
                  description = "CPU governor to use when on battery power";
                };
                
                scaling_min_freq = mkOption {
                  type = types.nullOr types.int;
                  default = null;
                  description = "Minimum CPU frequency when on battery (in kHz)";
                };
                
                scaling_max_freq = mkOption {
                  type = types.nullOr types.int;
                  default = null;
                  description = "Maximum CPU frequency when on battery (in kHz)";
                };
                
                turbo = mkOption {
                  type = types.nullOr (types.enum [ "auto" "always" "never" ]);
                  default = null;
                  description = "Turbo boost setting when on battery";
                };
                
                enable_thresholds = mkOption {
                  type = types.nullOr types.bool;
                  default = null;
                  description = "Enable battery charging thresholds";
                };
                
                start_threshold = mkOption {
                  type = types.nullOr (types.ints.between 0 100);
                  default = null;
                  description = "Battery charging start threshold (0-100)";
                };
                
                stop_threshold = mkOption {
                  type = types.nullOr (types.ints.between 0 100);
                  default = null;
                  description = "Battery charging stop threshold (0-100)";
                };
              };
            };
            default = {};
            description = "Settings for when the system is running on battery";
          };
        };
      };
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ 
      (if cfg.enableGui then guiPackage else defaultPackage)
    ];

    systemd = {
      packages = [ defaultPackage ];
      services.auto-cpufreq = {
        wantedBy = [ "multi-user.target" ];
        path = with pkgs; [ bash coreutils gawk ];
        overrideStrategy = "asDropin";

        serviceConfig = {
          WorkingDirectory = "";
          ExecStart = mkForce [
            ""
            "${defaultPackage}/bin/auto-cpufreq --daemon ${
              if cfg.settings != {} 
              then "--config ${cfgFile}"
              else ""
            }"
          ];
        };
      };
    };

    # Polkit rule to allow auto-cpufreq to run with elevated privileges
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (action.id == "org.auto-cpufreq.pkexec" &&
            subject.isInGroup("wheel")) {
          return polkit.Result.YES;
        }
      });
    '';

    assertions = [
      {
        assertion = !config.services.power-profiles-daemon.enable;
        message = ''
          You have set services.power-profiles-daemon.enable = true;
          which conflicts with auto-cpufreq. Please disable it or don't enable auto-cpufreq.
        '';
      }
      {
        assertion = !config.services.tlp.enable;
        message = ''
          You have set services.tlp.enable = true;
          which may conflict with auto-cpufreq. Please disable TLP or don't enable auto-cpufreq.
        '';
      }
    ];
  };
}
