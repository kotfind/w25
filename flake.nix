{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    flake-utils,
    fenix,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};

      inherit (pkgs) mkShell;

      rustToolchain = with fenix.packages.${system};
        combine (with stable; [
          rustc
          cargo
          rustfmt
          clippy
          rust-std
        ]);

      shell = mkShell {
        name = "w25-shell";

        buildInputs =
          [rustToolchain]
          ++ (with pkgs; [
            rust-analyzer

            cargo-machete
          ]);
      };
    in {
      devShells.default = shell;
    });
}
