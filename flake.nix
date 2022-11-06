{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nocargo = {
      url = "github:oxalica/nocargo";
      inputs.nixpkgs.follows = "nixpkgs";

      # See below.
      inputs.registry-crates-io.follows = "registry-crates-io";
    };

    # Optionally, you can explicitly import crates.io-index here.
    # So you can `nix flake update` at any time to get cutting edge version of crates,
    # instead of waiting `nocargo` to dump its dependency.
    # Otherwise, you can simply omit this to use the locked registry from `nocargo`,
    # which is updated periodically.
    registry-crates-io = {
      url = "github:rust-lang/crates.io-index";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, nocargo, registry-crates-io }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustPlatform = pkgs.rustPlatform;
        patched-mediasoup = pkgs.stdenv.mkDerivation rec {
          pname = "mediasoup-patched";
          version = "rust-0.11.1";

          src = pkgs.fetchFromGitHub {
            owner = "versatica";
            repo = "mediasoup";
            rev = version;
            sha256 = "sha256-V4gVOL3wTetuaLf2IQx/zhDUC0lMCgyZXRsXltz+JF4=";
          };

          phases = "unpackPhase patchPhase installPhase";
          patches = [ ./patches/mediasoup-sys.patch ];
          installPhase = "cp -r . $out";
        };

        mkSubstituteCmd = file: pre: post: ''
          substituteInPlace "${file}" --replace "${pre}" "${post}"
        '';

        # aided (but manually edited) by the following command:
        # rg _url --json | jq -sr '[.[] | select(.type == "match")] | group_by(.data.path.text)[] | { name: (. | first | .data.path.text), replacements: [.[] | .data.lines.text | scan("^(.*?)_url = (.*)") | { url: .[1], sha256: "" }]}'
        wraps = (import ./patches/wraps.nix);

        patchDerivation = src: patchPhase:
          pkgs.stdenv.mkDerivation {
            inherit src patchPhase;
            name = "patch-derivation";
            phases = "unpackPhase patchPhase installPhase";
            installPhase = "cp -r . $out";
          };

        mkWrapPatchPhase = wraps:
          let
            getFilename = wrapName: "worker/subprojects/${wrapName}.wrap";
            mkCommand = name: replacement:
              let
                filename = getFilename name;
                newPath = pkgs.fetchurl replacement;
                newUrl = "file://${newPath}";
              in (mkSubstituteCmd filename replacement.url newUrl);
            mkWrapCommands = wrap: map (mkCommand wrap.name) wrap.replacements;
            allCommands = builtins.concatMap mkWrapCommands wraps;
          in pkgs.lib.concatStrings allCommands;

        mediasoup-wrap-patched =
          patchDerivation patched-mediasoup (mkWrapPatchPhase wraps);

        patched-cargo-lock = pkgs.stdenv.mkDerivation {
          name = "patched-cargo-lock";
          src = ./.;
          phases = "unpackPhase patchPhase installPhase";
          patches = [ ./patches/update-cargo-lock.patch ];
          installPhase = "cp Cargo.lock $out";
        };
        patched-mediasoup-rel-path = "./patched-mediasoup-src";
        cargo-toml-patch-lines = pkgs.writeText "cargo-toml-patch-lines.txt" ''
          [patch.crates-io]
          mediasoup-sys = { path = "${patched-mediasoup-rel-path}/worker" }
        '';
        patched-src = pkgs.stdenv.mkDerivation {
          name = "patched-src";
          src = ./.;
          phases = "unpackPhase buildPhase installPhase";
          buildPhase = ''
            cat ${cargo-toml-patch-lines} >> Cargo.toml
          '';
          installPhase = "cp -r . $out";
        };
        nocargo-pkg = nocargo.lib.${system}.mkRustPackageOrWorkspace {
          # The root directory, which contains `Cargo.lock` and top-level `Cargo.toml`
          # (the one containing `[workspace]` for workspace).
          src = ./.;

          # If you use registries other than crates.io, they should be imported in flake inputs,
          # and specified here. Note that registry should be initialized via `mkIndex`,
          # with an optional override.
          extraRegistries = {
            # "https://example-registry.org" = nocargo.lib.${system}.mkIndex inputs.example-registry {};
          };

          # If you use crates from git URLs, they should be imported in flake inputs,
          # and specified here.
          gitSrcs = {
            # "https://github.com/some/repo" = inputs.example-git-source;
          };

          # If some crates in your dependency closure require packages from nixpkgs.
          # You can override the argument for `stdenv.mkDerivation` to add them.
          #
          # Popular `-sys` crates overrides are maintained in `./crates-io-override/default.nix`
          # to make them work out-of-box. PRs are welcome.
          buildCrateOverrides = with pkgs;
            {
              # Use package id format `pkgname version (registry)` to reference a direct or transitive dependency.
              # "zstd-sys 2.0.1+zstd.1.5.2 (registry+https://github.com/rust-lang/crates.io-index)" =
              #   old: {
              #     nativeBuildInputs = [ pkg-config ];
              #     propagatedBuildInputs = [ zstd ];
              #   };

              # Use package name to reference local crates.
              # "mypkg1" = old: { nativeBuildInputs = [ git ]; };
            };

          # We use the rustc from nixpkgs by default.
          # But you can override it, for example, with a nightly version from https://github.com/oxalica/rust-overlay
          # rustc = rust-overlay.packages.${system}.rust-nightly_2022-07-01;
        };

      in rec {
        packages.mediasoup = patched-mediasoup;
        packages.cargoLock = patched-cargo-lock;
        packages.src = patched-src;
        packages.patched = mediasoup-wrap-patched;

        packages.docker = pkgs.dockerTools.buildImage {
          name = "openmicc-server-docker";
          tag = "latest";
          # Config options reference:
          # https://github.com/moby/moby/blob/master/image/spec/v1.2.md#image-json-field-descriptions
          config = { Cmd = [ "${defaultPackage}/bin/openmicc-server" ]; };
          contents = with pkgs; [
            bash # bash
            coreutils # ls, cat, etc
            inetutils # ip, ifconfig, etc.
            iana-etc # /etc/protocols
            netcat-gnu # nc
            defaultPackage # openmicc-server
          ];
        };

        packages.nocargo = nocargo-pkg.release.openmicc-server;

        defaultPackage = rustPlatform.buildRustPackage {
          pname = "openmicc-server";
          version = "0.1.0";

          nativeBuildInputs = with pkgs; [ lld pkgconfig udev meson ninja ];

          buildInputs = with pkgs; [ openssl zstd ];

          dontUseNinjaBuild = true;
          dontUseNinjaCheck = true;
          dontUseNinjaInstall = true;

          cargoLock = { lockFile = patched-cargo-lock; };
          # cargoLock = { lockFile = ./Cargo.lock; };

          postPatch = ''
            cp ${patched-cargo-lock} Cargo.lock
          '';

          buildType = "debug";
          doCheck = false;

          preBuild = ''
            cp -r ${mediasoup-wrap-patched} ${patched-mediasoup-rel-path}
            chmod u+w -R ${patched-mediasoup-rel-path}
            # chmod u+w -R ${patched-src}
          '';

          # # patches = [ cargo-toml-patch ];

          src = patched-src;
        };

        devShell = pkgs.mkShell {
          name = "mediasoup-nix-shell";
          src = ./.;

          # build-time deps
          nativeBuildInputs = with pkgs; [
            python
            pythonPackages.pip
            rustc
            cargo
            lld
            pkgconfig
            udev
            meson
            ninja
          ];

          buildInputs = with pkgs; [ openssl zstd ];
        };
      });
}
