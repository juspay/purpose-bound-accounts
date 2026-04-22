{
  description = "pba-service — Purpose-Bound Account Service dev environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };

        smithy-cli = pkgs.callPackage ./nix/smithy-cli.nix { };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            # Rust
            rustToolchain

            # Build essentials
            pkgs.pkg-config
            pkgs.openssl

            # Zig 0.14.x (required by tigerbeetle-unofficial-sys crate)
            pkgs.zig_0_14

            # PostgreSQL
            pkgs.postgresql_16

            # TigerBeetle
            pkgs.tigerbeetle

            # Tools
            pkgs.just
            pkgs.sqlx-cli
            pkgs.cargo-watch
            pkgs.cocogitto

            # Smithy CLI
            smithy-cli
          ];

          env = {
            DB_HOST = "/tmp";
            DB_NAME = "pba_service";
            TIGERBEETLE_ADDRESSES = "3000";
            TIGERBEETLE_CLUSTER_ID = "0";
            RUST_LOG = "pba_service=debug";
            PG_DATA = ".pg_data";
            # Point TB sys crate to Nix-provided Zig instead of downloading its own
            ZIG_PATH = "${pkgs.zig_0_14}/bin/zig";
          };

          shellHook = ''
            echo "pba-service dev shell"
            echo "  rust   : $(rustc --version)"
            echo "  cargo  : $(cargo --version)"
            echo "  zig    : $(zig version)"
            echo "  psql   : $(psql --version)"
            echo ""
            echo "Run 'just setup' to initialize local Postgres + TigerBeetle."
            echo "Run 'just run' to start the service."
          '';
        };
      });
}
