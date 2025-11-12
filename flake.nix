{
  description = "A Nix-flake-based Rust development environment with pre-commit shell hook";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    git-hooks-nix.inputs.nixpkgs.follows = "nixpkgs";
    git-hooks-nix.url = "github:cachix/git-hooks.nix";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs @ {
    self,
    flake-parts,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [inputs.git-hooks-nix.flakeModule];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem = {
        pkgs,
        config,
        ...
      }: let
        /*
        Toolchain selection:
        - This uses nixpkgs' Rust (rustc/cargo) so it's reproducible across machines.
        - If you want a specific channel/version, you can override nixpkgs or
          swap to fenix/rustup in the future.
        */
        rustc = pkgs.rustc;
        cargo = pkgs.cargo;
        rustfmt = pkgs.rustfmt;
        clippy = pkgs.clippy;
        analyzer = pkgs.rust-analyzer;

        # Helpful extras—adjust to taste
        cargoComponents = with pkgs; [
          cargo-edit
          cargo-watch
          cargo-nextest
          cargo-audit
          pkg-config
        ];

        # Displayed in the shell greeting
        rustVersion = rustc.version;
      in {
        # Pre-commit hooks configuration (mirrors your Python setup’s style)
        pre-commit.settings = {
          # Anything listed as a hook will be made available to the devShell
          # via `enabledPackages`, which we include in packages below.
          hooks = {
            # Keep Nix files tidy
            alejandra.enable = true;

            # Rust format/lints
            rustfmt = {
              enable = true;
              args = ["--manifest-path" "project/Cargo.toml"];
            };
            clippy = {
              enable = true;
              args = ["--manifest-path" "project/Cargo.toml"];
            };

            # Optional: run a fast build check (uncomment if desired)
            # cargo-check.enable = true;

            # Optional: security audit (can be slower on large workspaces)
            # cargo-audit.enable = true;
          };
        };

        # Dev shell with shellHook installing pre-commit
        devShells.default = pkgs.mkShell {
          shellHook = ''
            ${config.pre-commit.installationScript}
            echo 1>&2 "Welcome to the development shell (Rust ${rustVersion})!"
            echo 1>&2 "  - rustc:   $(${pkgs.coreutils}/bin/printf '%s' "$(${rustc}/bin/rustc --version)")"
            echo 1>&2 "  - cargo:   $(${pkgs.coreutils}/bin/printf '%s' "$(${cargo}/bin/cargo --version)")"
            echo 1>&2 "  - clippy:  $(${pkgs.coreutils}/bin/printf '%s' "$(${clippy}/bin/cargo-clippy --version 2>/dev/null || echo 'available via cargo clippy')")"
            echo 1>&2 "  - rustfmt: $(${pkgs.coreutils}/bin/printf '%s' "$(${rustfmt}/bin/rustfmt --version)")"
          '';

          # Tools needed for your workflow, plus anything required by hooks.
          packages =
            config.pre-commit.settings.enabledPackages
            ++ [
              rustc
              cargo
              rustfmt
              clippy
              analyzer
            ]
            ++ cargoComponents ++ (with pkgs; [openssl grpc-tools libseccomp]);
        };
      };

      flake = {};
    };
}
