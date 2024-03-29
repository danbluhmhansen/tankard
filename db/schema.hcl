schema "public" {
  comment = "standard public schema"
}

table "user_streams" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  primary_key {
    columns = [column.id]
  }
}

table "game_streams" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "user_id" {
    type = uuid
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key {
    columns = [column.user_id]
    ref_columns = [table.user_streams.column.id]
    on_delete = CASCADE
  }
}

table "user_events" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "stream_id" {
    type = uuid
  }
  column "name" {
    type = text
  }
  column "timestamp" {
    type = timestamptz
    default = sql("clock_timestamp()")
  }
  column "data" {
    type = jsonb
    null = true
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key {
    columns = [column.stream_id]
    ref_columns = [table.user_streams.column.id]
    on_delete = CASCADE
  }
  index {
    unique = true
    columns = [column.stream_id, column.timestamp]
  }
}

table "game_events" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "stream_id" {
    type = uuid
  }
  column "name" {
    type = text
  }
  column "timestamp" {
    type = timestamptz
    default = sql("clock_timestamp()")
  }
  column "data" {
    type = jsonb
    null = true
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key {
    columns = [column.stream_id]
    ref_columns = [table.game_streams.column.id]
    on_delete = CASCADE
  }
  index {
    unique = true
    columns = [column.stream_id, column.timestamp]
  }
}

table "users" {
  schema = schema.public
  column "id" {
    type = uuid
  }
  column "added" {
    type = timestamptz
  }
  column "updated" {
    type = timestamptz
  }
  column "username" {
    type = text
  }
  column "salt" {
    type = text
  }
  column "passhash" {
    type = text
  }
  column "email" {
    type = text
    null = true
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key {
    columns = [column.id]
    ref_columns = [table.user_streams.column.id]
    on_delete = CASCADE
  }
}

table "games" {
  schema = schema.public
  column "id" {
    type = uuid
  }
  column "user_id" {
    type = uuid
  }
  column "added" {
    type = timestamptz
  }
  column "updated" {
    type = timestamptz
  }
  column "name" {
    type = text
  }
  column "description" {
    type = text
    null = true
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key {
    columns = [column.id]
    ref_columns = [table.game_streams.column.id]
    on_delete = CASCADE
  }
  foreign_key {
    columns = [column.user_id]
    ref_columns = [table.users.column.id]
    on_delete = CASCADE
  }
}
