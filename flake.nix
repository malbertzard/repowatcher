{
  description = "RepoWatcher — local-first VCS watcher with a Leptos UI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        version = "0.1.0";

        # These are put on PATH for both the wrapper and the dev shell. Built-in
        # drivers (libgit2) need none of them, but plugins for other VCS systems
        # can shell out safely: the tools are always present.
        vcsTools = with pkgs; [
          git
          mercurial
          subversion
          rcs
          jujutsu
          pijul
          openssh
          cacert
        ];

        rw = pkgs.rustPlatform.buildRustPackage {
          pname = "rw";
          inherit version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
          buildInputs = [ pkgs.openssl ];
          nativeCheckInputs = [ pkgs.git ];
          doCheck = true;
          # Nix sets this for openssl-sys; aws-lc-sys (pulled in by reqwest)
          # misreads it and demands a system AWS-LC that does not exist.
          preBuild = ''
            unset AWS_LC_SYS_USE_SYSTEM
            unset LIBGIT2_NO_VENDOR
            unset LIBSSH2_SYS_USE_PKG_CONFIG
          '';

          postInstall = ''
            wrapProgram $out/bin/rw \
              --prefix PATH : ${lib.makeBinPath vcsTools} \
              --set RW_UI_DIST ${ui}
          '';

          meta = with lib; {
            description = "Watch code sources across VCS systems";
            mainProgram = "rw";
            platforms = platforms.all;
          };
        };

        desktopDeps = with pkgs; [
          webkitgtk_4_1
          gtk3
          libsoup_3
          dbus
          libayatana-appindicator
          librsvg
          at-spi2-core
          pango
          cairo
          gdk-pixbuf
        ];

        desktop = pkgs.rustPlatform.buildRustPackage {
          pname = "repowatcher-desktop";
          inherit version;
          src = ./.;
          cargoLock.lockFile = ./apps/desktop/Cargo.lock;
          postPatch = ''
            cp apps/desktop/Cargo.lock Cargo.lock
          '';
          nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ] ++ desktopDeps;
          buildInputs = desktopDeps;
          doCheck = false;
          buildPhase = ''
            runHook preBuild
            cargo build --release --offline --manifest-path apps/desktop/Cargo.toml
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            mkdir -p $out/bin
            install -m755 apps/desktop/target/release/rw-desktop $out/bin/repowatcher
            wrapProgram $out/bin/repowatcher \
              --set RW_BIN ${rw}/bin/rw \
              --prefix PATH : ${lib.makeBinPath vcsTools}
            runHook postInstall
          '';
          meta = {
            description = "RepoWatcher desktop application";
            mainProgram = "repowatcher";
            platforms = lib.platforms.linux;
          };
        };

        npmDeps = pkgs.fetchNpmDeps {
          src = ./ui;
          hash = "sha256-tpsw/cr3ESg+XPIst859++4B1xU2AE1ltvynrc8wCog=";
        };

        ui = pkgs.stdenv.mkDerivation {
          pname = "rw-ui";
          inherit version;
          src = ./.;
          inherit npmDeps;
          cargoDeps = pkgs.rustPlatform.importCargoLock {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [
            pkgs.rustPlatform.cargoSetupHook
            pkgs.rustc
            pkgs.cargo
            pkgs.nodejs
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            pkgs.lld
          ];

          buildPhase = ''
            export CARGO_TARGET_DIR=$TMPDIR/rw-ui-target
            export npm_config_cache=${npmDeps}
            export HOME=$TMPDIR
            cd ui
            npm ci --offline --ignore-scripts
            patchShebangs node_modules
            npm run css
            trunk build --release
            cd ..
          '';
          installPhase = ''
            mkdir -p $out
            cp -r ui/dist/. $out/
          '';
        };
      in {
        packages = {
          default = rw;
          rw = rw;
          rw-ui = ui;
          rw-desktop = desktop;
        };

        apps.default = {
          type = "app";
          program = "${rw}/bin/rw";
          meta = { description = "Run the RepoWatcher CLI"; };
        };

        apps.desktop = {
          type = "app";
          program = "${desktop}/bin/repowatcher";
          meta = { description = "Run the RepoWatcher desktop application"; };
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.rustc
            pkgs.cargo
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            pkgs.lld
            pkgs.nodejs
            pkgs.pkg-config
          ] ++ vcsTools ++ desktopDeps;

          shellHook = ''
            echo "RepoWatcher dev shell: Rust, trunk, Tailwind, lld and VCS CLI tools are on PATH."
          '';
        };
      })
    // {
      nixosModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.repowatcher;
        in {
          options.services.repowatcher = {
            enable = lib.mkEnableOption "RepoWatcher background daemon";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.system}.rw;
              defaultText = lib.literalExpression "self.packages.<system>.rw";
              description = "RepoWatcher package to run.";
            };
            configFile = lib.mkOption {
              type = lib.types.str;
              default = "%h/.config/rw/config.yaml";
              description = "Config file passed to rw. systemd expands %h to the user's home.";
            };
            port = lib.mkOption {
              type = lib.types.port;
              default = 3030;
              description = "Loopback port for the daemon and UI.";
            };
          };

          config = lib.mkIf cfg.enable {
            systemd.user.services.repowatcher = {
              description = "RepoWatcher background daemon";
              wantedBy = [ "default.target" ];
              environment.RW_UI_DIST = "${self.packages.${pkgs.system}.rw-ui}";
              serviceConfig = {
                ExecStart = "${cfg.package}/bin/rw --config ${cfg.configFile} --port ${toString cfg.port} daemon";
                Restart = "on-failure";
              };
            };
          };
        };
    };
}
