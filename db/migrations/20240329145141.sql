-- Create "user_streams" table
CREATE TABLE "public"."user_streams" ("id" uuid NOT NULL DEFAULT gen_random_uuid(), PRIMARY KEY ("id"));
-- Create "game_streams" table
CREATE TABLE "public"."game_streams" ("id" uuid NOT NULL DEFAULT gen_random_uuid(), "user_id" uuid NOT NULL, PRIMARY KEY ("id"), CONSTRAINT "game_streams_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."user_streams" ("id") ON UPDATE NO ACTION ON DELETE CASCADE);
-- Create "game_events" table
CREATE TABLE "public"."game_events" ("id" uuid NOT NULL DEFAULT gen_random_uuid(), "stream_id" uuid NOT NULL, "name" text NOT NULL, "timestamp" timestamptz NOT NULL DEFAULT clock_timestamp(), "data" jsonb NULL, PRIMARY KEY ("id"), CONSTRAINT "game_events_stream_id_fkey" FOREIGN KEY ("stream_id") REFERENCES "public"."game_streams" ("id") ON UPDATE NO ACTION ON DELETE CASCADE);
-- Create index "game_events_stream_id_timestamp_idx" to table: "game_events"
CREATE UNIQUE INDEX "game_events_stream_id_timestamp_idx" ON "public"."game_events" ("stream_id", "timestamp");
-- Create "users" table
CREATE TABLE "public"."users" ("id" uuid NOT NULL, "added" timestamptz NOT NULL, "updated" timestamptz NOT NULL, "username" text NOT NULL, "salt" text NOT NULL, "passhash" text NOT NULL, "email" text NULL, PRIMARY KEY ("id"), CONSTRAINT "users_id_fkey" FOREIGN KEY ("id") REFERENCES "public"."user_streams" ("id") ON UPDATE NO ACTION ON DELETE CASCADE);
-- Create "games" table
CREATE TABLE "public"."games" ("id" uuid NOT NULL, "user_id" uuid NOT NULL, "added" timestamptz NOT NULL, "updated" timestamptz NOT NULL, "name" text NOT NULL, "description" text NULL, PRIMARY KEY ("id"), CONSTRAINT "games_id_fkey" FOREIGN KEY ("id") REFERENCES "public"."game_streams" ("id") ON UPDATE NO ACTION ON DELETE CASCADE, CONSTRAINT "games_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."users" ("id") ON UPDATE NO ACTION ON DELETE CASCADE);
-- Create "user_events" table
CREATE TABLE "public"."user_events" ("id" uuid NOT NULL DEFAULT gen_random_uuid(), "stream_id" uuid NOT NULL, "name" text NOT NULL, "timestamp" timestamptz NOT NULL DEFAULT clock_timestamp(), "data" jsonb NULL, PRIMARY KEY ("id"), CONSTRAINT "user_events_stream_id_fkey" FOREIGN KEY ("stream_id") REFERENCES "public"."user_streams" ("id") ON UPDATE NO ACTION ON DELETE CASCADE);
-- Create index "user_events_stream_id_timestamp_idx" to table: "user_events"
CREATE UNIQUE INDEX "user_events_stream_id_timestamp_idx" ON "public"."user_events" ("stream_id", "timestamp");
