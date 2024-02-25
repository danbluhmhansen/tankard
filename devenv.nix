{ pkgs, lib, ... }: {
  # https://devenv.sh/basics/
  env.DATABASE_URL = "postgres://postgres:password@localhost:5432/tankard?sslmode=disable";
  env.PASERK = "k4.local.MBRMvUocz642L1jhYCP7ORQ1QXHc6ryMXcASX780D-Q";
  env.AMQP_URL = "amqp://localhost:5672";

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.cargo-watch
    pkgs.bun
    pkgs.grass-sass
    pkgs.nodePackages.typescript-language-server
  ] ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk; [
    frameworks.CoreFoundation
    frameworks.Security
    frameworks.SystemConfiguration
  ]);

  # https://devenv.sh/scripts/
  scripts.watch-server.exec = "cargo watch --exec run";
  scripts.watch-bundle.exec = ''
    bun build modules/alpine.ts modules/htmx.ts --minify --splitting --watch --outdir=dist --sourcemap=external
  '';

  scripts.build-style.exec = ''
    grass --load-path=node_modules/@picocss/pico/scss/ --style=compressed style/site.scss dist/site.css
  '';

  scripts.db-init.exec = ''
    psql --dbname=tankard --file=./data/init.sql
    psql --dbname=tankard --file=./data/users/events.sql
    psql --dbname=tankard --file=./data/users/snaps.sql
    psql --dbname=tankard --file=./data/users/views.sql
    psql --dbname=tankard --file=./data/users/commands.sql
    psql --dbname=tankard --file=./data/auth.sql
    psql --dbname=tankard --file=./data/games/events.sql
    psql --dbname=tankard --file=./data/games/snaps.sql
    psql --dbname=tankard --file=./data/games/views.sql
    psql --dbname=tankard --file=./data/games/commands.sql
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
    typos.enable = true;
  };

  # https://devenv.sh/processes/
  processes.watch-server.exec = "watch-server";
  processes.watch-bundle.exec = "watch-bundle";

  # https://devenv.sh/services/
  services.postgres = {
    enable = true;
    package = pkgs.postgresql_16;
    listen_addresses = "127.0.0.1";
    initialDatabases = [
      {name = "tankard";}
    ];
    initialScript = ''
      create user postgres superuser password 'password';
    '';
  };

  services.rabbitmq.enable = true;

  # https://devenv.sh/integrations/codespaces-devcontainer/
  devcontainer.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
