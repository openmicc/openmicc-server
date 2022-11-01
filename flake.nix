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
      in {
        defaultPackage = rustPlatform.buildRustPackage {
          pname = "openmicc-server";
          version = "0.1.0";

          nativeBuildInputs = with pkgs; [ openssl lld pkgconfig udev ];

          cargoLock = { lockFile = ./Cargo.lock; };

          src = ./.;
        };

        apps.dockerBuild = let
          buildah = "${pkgs.buildah}/bin/buildah";
          script = pkgs.writeShellScript "build.sh" ''
            ${buildah} build .
            # TODO: Tag & push
          '';
        in {
          type = "app";
          program = (pkgs.lib.traceVal script.outPath);
        };

        devShell = pkgs.mkShell {
          name = "openmicc-server-shell";
          src = ./.;

          # build-time deps
          nativeBuildInputs =
            (with pkgs; [ openssl rustc cargo lld pkgconfig udev ]);
        };
      });
}
