schema "public" {
  comment = "standard public schema"
}

table "user_streams" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  primary_key { columns = [column.id] }
}

table "user_events" {
  schema = schema.public
  column "stream_id" { type = uuid }
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "name" { type = text }
  column "timestamp" {
    type = timestamptz
    default = sql("clock_timestamp()")
  }
  column "data" {
    type = jsonb
    null = true
  }
  primary_key { columns = [column.id] }
  foreign_key {
    columns = [column.stream_id]
    ref_columns = [table.user_streams.column.id]
    on_delete = CASCADE
  }
  index { columns = [column.stream_id] }
  index { columns = [column.stream_id, column.name] }
  index { columns = [column.stream_id, column.timestamp] }
}

table "game_streams" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "user_id" { type = uuid }
  primary_key { columns = [column.id] }
  foreign_key {
    columns = [column.user_id]
    ref_columns = [table.user_streams.column.id]
    on_delete = CASCADE
  }
  index { columns = [column.user_id] }
}

table "game_events" {
  schema = schema.public
  column "stream_id" { type = uuid }
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "name" { type = text }
  column "timestamp" {
    type = timestamptz
    default = sql("clock_timestamp()")
  }
  column "data" {
    type = jsonb
    null = true
  }
  primary_key { columns = [column.id] }
  foreign_key {
    columns = [column.stream_id]
    ref_columns = [table.game_streams.column.id]
    on_delete = CASCADE
  }
  index { columns = [column.stream_id] }
  index { columns = [column.stream_id, column.name] }
  index { columns = [column.stream_id, column.timestamp] }
}
