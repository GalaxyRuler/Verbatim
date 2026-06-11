# Home-manager module for Verbatim speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ verbatim.homeManagerModules.default ];
#        services.verbatim.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.verbatim;
in
{
  options.services.verbatim = {
    enable = lib.mkEnableOption "Verbatim speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "verbatim.packages.\${system}.verbatim";
      description = "The Verbatim package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.verbatim = {
      Unit = {
        Description = "Verbatim speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/verbatim";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
