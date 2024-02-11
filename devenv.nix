{ pkgs, lib, ... }:

{
  # https://devenv.sh/basics/
  env.DATABASE_URL = "postgres://postgres:password@localhost:5432/tankard?sslmode=disable";
  env.DATABASE_DEV_URL = "postgres://postgres:password@localhost:5432/tankard_dev?sslmode=disable";
  env.PASERK = "k4.local.MBRMvUocz642L1jhYCP7ORQ1QXHc6ryMXcASX780D-Q";

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.cargo-watch
    pkgs.bun
    pkgs.nodePackages.typescript-language-server
    pkgs.nodePackages.sql-formatter
    pkgs.rustywind
  ] ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk; [
    frameworks.CoreFoundation
    frameworks.Security
    frameworks.SystemConfiguration
  ]);

  # https://devenv.sh/scripts/
  scripts.watch-server.exec = "cargo watch --exec run";
  scripts.watch-bundle.exec = "bun build modules/alpine.ts modules/htmx.ts --splitting --watch --outdir=dist";
  scripts.watch-unocss.exec = "bun unocss --watch";

  scripts.db-init.exec = ''
    psql --dbname=tankard --file=./data/events.sql
    psql --dbname=tankard --file=./data/init.sql
    psql --dbname=tankard --file=./data/users.sql
    psql --dbname=tankard --file=./data/games.sql
    psql --dbname=tankard --file=./data/auth.sql
  '';

  enterShell = "bun install";

  # https://devenv.sh/languages/
  languages.rust.enable = true;

  # https://devenv.sh/pre-commit-hooks/
  pre-commit.hooks = {
    actionlint.enable = true;
    cargo-check.enable = true;
    clippy.enable = true;
    rustfmt.enable = true;
    rustywind = {
      enable = true;
      entry = "rustywind --write";
      files = "\.rs$";
    };
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
    '';
  };

  # https://devenv.sh/integrations/codespaces-devcontainer/
  devcontainer.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
