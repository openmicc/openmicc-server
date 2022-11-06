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

        nocargo-pkg = nocargo.lib.${system}.mkRustPackageOrWorkspace {
          # Use cleanCargoSource to avoid rebuilds when unrelated files change
          src = craneLib.cleanCargoSource ./.;

          buildCrateOverrides = with pkgs; {
            # Use package id format `pkgname version (registry)` to reference a direct or transitive dependency.
            "zstd-sys 2.0.1+zstd.1.5.2 (registry+https://github.com/rust-lang/crates.io-index)" =
              _old: {
                nativeBuildInputs = [ pkg-config ];
                propagatedBuildInputs = [ zstd ];
              };

            "mediasoup-sys 0.5.1 (registry+https://github.com/rust-lang/crates.io-index)" =
              _old: {
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
              _old: {
                procMacro = false;
              };
          };
        };

      in rec {
        packages.default = nocargo-pkg.release.openmicc-server.bin;
        defaultPackage = packages.default;

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

            sops
            gh
            terraform
          ];

          buildInputs = with pkgs; [ openssl zstd ];
        };
      });
}
