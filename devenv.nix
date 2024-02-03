{ pkgs, lib, ... }:

{
  # https://devenv.sh/basics/
  env.DATABASE_URL = "postgres://postgres:password@localhost:5432/tankard?sslmode=disable";
  env.DATABASE_DEV_URL = "postgres://postgres:password@localhost:5432/tankard_dev?sslmode=disable";
  env.ACCESS_TOKEN_PRIVATE_KEY = builtins.readFile ./private_key.pem;
  env.ACCESS_TOKEN_PUBLIC_KEY = builtins.readFile ./public_key.pem;

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.atlas
    pkgs.cargo-watch
    pkgs.sleek
    pkgs.tailwindcss
  ] ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk; [
    frameworks.CoreFoundation
    frameworks.Security
    frameworks.SystemConfiguration
  ]);

  # https://devenv.sh/scripts/
  scripts.schema_diff.exec = "atlas schema diff --env local --from $DATABASE_URL --to file://schema.hcl | bat --language sql";
  scripts.tw_watch.exec = "tailwindcss --input tailwind.css --output static/site.css --watch";

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
  processes.watch.exec = "cargo watch --exec run";
  # processes.tw_watch.exec = "tw_watch";

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
