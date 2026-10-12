{
  description = "Pinned LDK Node bindings build and release tools";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/8edc0c72e3a38faf5434e40d1f19431125fe4b30";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/404b7a5537f0adc6e5c558a1b94612e683dec504";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      eachSystem = nixpkgs.lib.genAttrs systems;
      environments = eachSystem (
        system:
        import ./nix/environment.nix {
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
            config.android_sdk.accept_license = true;
            config.allowUnfree = true;
          };
          inherit self;
        }
      );
    in
    {
      lib.mavenPublication =
        {
          system,
          source,
          platform,
        }:
        environments.${system}.mavenPublication { inherit source platform; };
      packages = eachSystem (system: environments.${system}.packages);
      devShells = eachSystem (system: environments.${system}.shells);
      apps = eachSystem (system: {
        release = {
          type = "app";
          program = "${self.packages.${system}.release}/bin/release";
        };
      });
      formatter = eachSystem (system: (import nixpkgs { inherit system; }).nixfmt);
    };
}
