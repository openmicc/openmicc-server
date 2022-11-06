{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";

    nocargo = {
      url = "github:oxalica/nocargo";
      # url = "/home/oliver/local/src/nocargo";
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

  outputs = { self, nixpkgs, flake-utils, crane, nocargo, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rustPlatform = pkgs.rustPlatform;
        craneLib = crane.lib.${system};
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
          # Use cleanCargoSource to avoid rebuilds when unrelated files change
          src = craneLib.cleanCargoSource ./.;
          # If some crates in your dependency closure require packages from nixpkgs.
          # You can override the argument for `stdenv.mkDerivation` to add them.
          #
          # Popular `-sys` crates overrides are maintained in `./crates-io-override/default.nix`
          # to make them work out-of-box. PRs are welcome.
          buildCrateOverrides = with pkgs; {
            # Use package id format `pkgname version (registry)` to reference a direct or transitive dependency.
            "zstd-sys 2.0.1+zstd.1.5.2 (registry+https://github.com/rust-lang/crates.io-index)" =
              old: {
                nativeBuildInputs = [ pkg-config ];
                propagatedBuildInputs = [ zstd ];
              };

            # Use package name to reference local crates.
            # "mypkg1" = old: { nativeBuildInputs = [ git ]; };

            "mediasoup-sys 0.5.1 (registry+https://github.com/rust-lang/crates.io-index)" =
              old: {
                src = "${mediasoup-wrap-patched}/worker";

                nativeBuildInputs = with pkgs; [ meson ninja doxygen cargo ];
                # buildInputs = with pkgs; [ openssl zstd ];

                dontConfigure = false;
                preConfigure = "echo PRECONFIGURE";
                postConfigure = "echo POSTCONFIGURE";
                dontUseMesonConfigure = true;

                mesonWrapMode = "default";
                dontUseNinjaBuild = true;
                dontUseNinjaCheck = true;
                dontUseNinjaInstall = true;
              };

            "paste 0.1.18 (registry+https://github.com/rust-lang/crates.io-index)" =
              old: {
                procMacro = false;
              };
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
          config = { Cmd = [ "/bin/openmicc_server" ]; };
          copyToRoot = pkgs.buildEnv {
            name = "image-root";
            paths = with pkgs; [
              bashInteractive # bash
              coreutils # ls, cat, etc
              inetutils # ip, ifconfig, etc.
              iana-etc # /etc/protocols
              netcat-gnu # nc
              defaultPackage # openmicc-server
            ];
            pathsToLink = [ "/bin" ];
          };
        };

        packages.nocargo = nocargo-pkg.release.openmicc-server.bin;

        packages.default = packages.nocargo;
        defaultPackage = packages.default;

        packages.rustPackage = rustPlatform.buildRustPackage {
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
