{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs { inherit system; };

        nativeBuildInputs = with pkgs; [
          cargo
          clang
          cmake
          git
          makeWrapper
          pkg-config
          rustc
        ];

        buildInputs = with pkgs; [
          alsa-lib
          fontconfig
          freetype
          libgit2
          openssl
          sqlite
          vulkan-loader
          wayland
          libx11
          libxcb
          libxkbcommon
          zstd
        ];

        termy = pkgs.rustPlatform.buildRustPackage {
          name = "termy";
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          inherit nativeBuildInputs buildInputs;

          doCheck = false;
          env = {
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          };

          postInstall = ''
            wrapProgram $out/bin/termy \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath buildInputs}
          '';

          meta = {
            description = "A fast, minimal terminal emulator built with GPUI and alacritty_terminal";
            homepage = "https://github.com/termy-org/termy";
            license = pkgs.lib.licenses.mit;
            mainProgram = "termy";
            platforms = [ "x86_64-linux" ];
          };
        };
      in
      {
        packages = {
          default = termy;
          termy = termy;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = termy; };
          termy = flake-utils.lib.mkApp { drv = termy; };
        };

        devShells.default = pkgs.mkShell {
          packages =
            with pkgs;
            [
              cargo-watch
              just
              rust-analyzer
            ]
            ++ nativeBuildInputs
            ++ buildInputs;

          env = {
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
          };
        };
      }
    );
}
