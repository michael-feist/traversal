{
  description = "traversal – tag cross-referencer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/1e78637806c14b81a1e8dccadf00be7e93dda457";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        toolchain = builtins.fromTOML (builtins.readFile (self + "/rust-toolchain.toml"));
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustup
            nodejs
            cacert
          ];

          RUSTC_VERSION = toolchain.toolchain.channel;

          shellHook = ''
            export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export NIX_SSL_CERT_FILE="$SSL_CERT_FILE"
            export PATH="$PATH:''${CARGO_HOME:-$HOME/.cargo}/bin"
            export PATH="$PATH:''${RUSTUP_HOME:-$HOME/.rustup}/toolchains/$RUSTC_VERSION-${pkgs.stdenv.hostPlatform.rust.rustcTarget}/bin"
          '';
        };
      });
}
