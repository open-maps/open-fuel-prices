{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }:
    let
      forAll = f: nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: rec {
        default = open-fuel-prices;
        open-fuel-prices = pkgs.callPackage ./nix/package.nix { };
      });

      nixosModules.default = { pkgs, lib, ... }: {
        imports = [ ./nix/module.nix ];
        services.open-fuel-prices.package = lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.open-fuel-prices;
      };

      checks = forAll (pkgs: {
        module = (nixpkgs.lib.nixosSystem {
          inherit (pkgs.stdenv.hostPlatform) system;
          modules = [
            self.nixosModules.default
            {
              boot.loader.grub.enable = false;
              fileSystems."/" = { device = "none"; fsType = "tmpfs"; };
              system.stateVersion = "25.11";
              services.open-fuel-prices = {
                enable = true;
                virtualHost = "fuel.example.org";
              };
            }
          ];
        }).config.system.build.toplevel;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer pkg-config openssl ];
        };
      });
    };
}
