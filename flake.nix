{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
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

      in {
        packages.mediasoup = patched-mediasoup;
        packages.cargoLock = patched-cargo-lock;
        packages.src = patched-src;
        packages.patched = mediasoup-wrap-patched;

        defaultPackage = rustPlatform.buildRustPackage {
          pname = "openmicc-server";
          version = "0.1.0";

          nativeBuildInputs = with pkgs; [ lld pkgconfig udev meson ninja ];
          dontUseNinjaBuild = true;
          dontUseNinjaCheck = true;
          dontUseNinjaInstall = true;

          cargoLock = { lockFile = patched-cargo-lock; };
          # cargoLock = { lockFile = ./Cargo.lock; };

          postPatch = ''
            cp ${patched-cargo-lock} Cargo.lock
          '';

          preBuild = ''
            cp -r ${mediasoup-wrap-patched} ${patched-mediasoup-rel-path}
            chmod u+w -R ${patched-mediasoup-rel-path}
          '';

          # # patches = [ cargo-toml-patch ];

          src = patched-src;
        };

        devShell = pkgs.mkShell {
          name = "mediasoup-nix-shell";
          src = ./.;

          # build-time deps
          nativeBuildInputs = (with pkgs; [
            python
            pythonPackages.pip
            rustc
            cargo
            lld
            pkgconfig
            udev
            meson
            ninja
          ]);
        };
      });
}
