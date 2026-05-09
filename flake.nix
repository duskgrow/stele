{
  description = "stele — agent knowledge storage/retrieval infrastructure";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
        release = import ./nix/sources.nix;
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let baseName = builtins.baseNameOf path;
            in !(builtins.elem baseName [".git" "target" "result"]);
        };
      in {
        packages.default = pkgs.stdenv.mkDerivation {
          pname = "stele";
          version = release.version;

          src = pkgs.fetchurl {
            url = "https://github.com/duskgrow/stele/releases/download/v${release.version}/stele-${system}";
            hash = release.hashes.${system};
          };

          dontUnpack = true;
          dontBuild = true;

          installPhase = ''
            runHook preInstall
            install -Dm755 $src $out/bin/stele
            runHook postInstall
          '';

          meta = with pkgs.lib; {
            description = "stele — agent knowledge storage/retrieval infrastructure";
            license = licenses.mit;
            mainProgram = "stele";
          };
        };

        packages.dev = pkgs.rustPlatform.buildRustPackage {
          pname = "stele-dev";
          version = release.version;
          inherit src;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [
            rustToolchain
            pkgs.pkg-config
          ];

          buildInputs = [
            pkgs.openssl
          ];

          doCheck = false;

          meta = with pkgs.lib; {
            description = "stele — agent knowledge storage/retrieval infrastructure";
            license = licenses.mit;
            mainProgram = "stele";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo-watch
            pkgs.cargo-edit
            pkgs.cargo-llvm-cov
            pkgs.pkg-config
            pkgs.openssl
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        devShells.ci = let
          muslToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "x86_64-unknown-linux-musl" ];
          };
          muslPkgs = pkgs.pkgsStatic;
          muslGcc = "${pkgs.musl.dev}/bin/musl-gcc";
          muslOpenssl = muslPkgs.openssl;
        in pkgs.mkShell {
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ muslToolchain ];

          # muslOpenssl is NOT in buildInputs — referencing it only via env
          # vars prevents the pkgsStatic overlay from replacing the host cc
          # with the musl cross-compiler, which broke host build scripts.
          "CC_x86_64_unknown_linux_musl" = muslGcc;
          "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER" = muslGcc;
          "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS" = "-C relocation-model=static";
          OPENSSL_STATIC = "1";
          OPENSSL_DIR = "${muslOpenssl.dev}";
          OPENSSL_LIB_DIR = "${muslOpenssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${muslOpenssl.dev}/include";
        };

      }
    );
}
