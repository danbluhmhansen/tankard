{ pkgs, lib, ... }:

{
  # https://devenv.sh/basics/
  env.DATABASE_URL = "postgres://postgres:password@localhost:5432/tankard?sslmode=disable";
  env.DATABASE_DEV_URL = "postgres://postgres:password@localhost:5432/tankard_dev?sslmode=disable";
  env.PASERK = "k4.local.MBRMvUocz642L1jhYCP7ORQ1QXHc6ryMXcASX780D-Q";

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.atlas
    pkgs.cargo-watch
    pkgs.sleek
    pkgs.bun
    pkgs.nodePackages.typescript-language-server
  ] ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk; [
    frameworks.CoreFoundation
    frameworks.Security
    frameworks.SystemConfiguration
  ]);

  # https://devenv.sh/scripts/
  scripts.watch-server.exec = "cargo watch --exec run";
  scripts.watch-bundle.exec = "bun build alpine.ts htmx.ts --splitting --watch --outdir=dist";
  scripts.watch-unocss.exec = "bun unocss --watch";
  scripts.schema-diff.exec = "atlas schema diff --env local --from $DATABASE_URL --to file://schema.hcl | bat --language sql";

  enterShell = "bun install";

  # https://devenv.sh/languages/
  languages.rust.enable = true;

  # https://devenv.sh/pre-commit-hooks/
  pre-commit.hooks = {
    actionlint.enable = true;
    cargo-check.enable = true;
    clippy.enable = true;
    rustfmt.enable = true;
    typos.enable = true;
  };

  # https://devenv.sh/processes/
  processes.watch-server.exec = "watch-server";
  processes.watch-bundle.exec = "watch-bundle";
  processes.watch-unocss.exec = "watch-unocss";

  # https://devenv.sh/services/
  services.postgres = {
    enable = true;
    package = pkgs.postgresql_16;
    listen_addresses = "127.0.0.1";
    initialScript = ''
      CREATE USER postgres SUPERUSER PASSWORD 'password';
      CREATE DATABASE tankard;
      CREATE DATABASE tankard_dev;
    '';
  };

  # https://devenv.sh/integrations/codespaces-devcontainer/
  devcontainer.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
