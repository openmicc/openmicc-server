{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, fenix, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustPlatform = pkgs.rustPlatform;

        deps = with pkgs; [
          python
          lld
          pkgconfig
          udev
          # ninja
        ];
        pyDeps = with pkgs.python3Packages;
          [
            pip
            # meson
          ];
        allDeps = deps ++ pyDeps;
      in {
        defaultPackage = rustPlatform.buildRustPackage {
          pname = "openmicc-server";
          version = "0.1.0";

          nativeBuildInputs = allDeps;

          cargoLock = { lockFile = ./Cargo.lock; };

          src = ./.;
        };

        devShell = pkgs.mkShell {
          name = "openmicc-server-shell";
          src = ./.;

          # build-time deps
          nativeBuildInputs = (with pkgs; [ openssl rustc cargo ]) ++ allDeps;
        };
      });
}
