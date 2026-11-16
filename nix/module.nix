{ config, lib, pkgs, ... }:

let
  cfg = config.services.open-fuel-prices;
  dataDir = "/var/lib/open-fuel-prices";
  settings = {
    user_agent = cfg.userAgent;
    out_dir = "${dataDir}/out";
    state_dir = "${dataDir}/state";
    max_price_age_days = cfg.maxPriceAgeDays;
    source = cfg.sources;
  };
  configFile = (pkgs.formats.toml { }).generate "open-fuel-prices.toml" settings;
in
{
  options.services.open-fuel-prices = {
    enable = lib.mkEnableOption "collecting per-station fuel prices from whole-country files";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The open-fuel-prices package.";
    };

    userAgent = lib.mkOption {
      type = lib.types.str;
      default = "OpenFuelPrices/0.1";
      description = "User-Agent sent to publishers.";
    };

    maxPriceAgeDays = lib.mkOption {
      type = lib.types.ints.positive;
      default = 30;
      description = "Prices reported longer ago than this are left out.";
    };

    sources = lib.mkOption {
      type = lib.types.listOf (lib.types.attrsOf lib.types.anything);
      default = (builtins.fromTOML (builtins.readFile ../config.toml)).source;
      defaultText = lib.literalExpression "the sources in config.toml";
      description = "Price sources, in the format of `[[source]]` entries in config.toml.";
    };

    virtualHost = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Nginx virtual host serving the published files. It may be shared with other services. Null disables nginx.";
    };

    path = lib.mkOption {
      type = lib.types.str;
      default = "/fuel/";
      description = "Location on the virtual host, ending in a slash.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.open-fuel-prices = {
      isSystemUser = true;
      group = "open-fuel-prices";
      home = dataDir;
    };
    users.groups.open-fuel-prices = { };

    systemd.tmpfiles.settings.open-fuel-prices.${dataDir}.d = {
      user = "open-fuel-prices";
      group = "open-fuel-prices";
      mode = "0755";
    };

    systemd.services.open-fuel-prices = {
      description = "Fuel price collection";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      environment.OFP_CONFIG = configFile;
      serviceConfig = {
        User = "open-fuel-prices";
        Group = "open-fuel-prices";
        WorkingDirectory = dataDir;
        ExecStart = "${lib.getExe cfg.package} run";
        Restart = "on-failure";
        RestartSec = 30;
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        ReadWritePaths = [ dataDir ];
      };
    };

    services.nginx = lib.mkIf (cfg.virtualHost != null) {
      enable = true;
      virtualHosts.${cfg.virtualHost}.locations.${cfg.path} = {
        alias = "${dataDir}/out/";
        extraConfig = ''
          types {
            application/json json;
          }
          gzip on;
          gzip_types application/json;
          add_header Cache-Control "public, max-age=300";
          add_header Access-Control-Allow-Origin "*";
        '';
      };
    };
  };
}
