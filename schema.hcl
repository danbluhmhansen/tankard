schema "public" {
  comment = "standard public schema"
}

table "stream_types" {
  schema = schema.public
  column "id" {
    type = int
    identity {
      generated = ALWAYS
    }
  }
  column "name" {
    type = text
  }
  primary_key {
    columns = [column.id]
  }
  index "name" {
    columns = [column.name]
    unique = true
  }
}

table "event_streams" {
  schema = schema.public
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "type" {
    type = int
  }
  primary_key {
    columns = [column.id]
  }
  foreign_key "type" {
    columns = [column.type]
    ref_columns = [table.stream_types.column.id]
    on_delete = CASCADE
  }
  index "type" {
    columns = [column.type]
  }
}

table "events" {
  schema = schema.public
  column "stream_id" {
    type = uuid
  }
  column "id" {
    type = uuid
    default = sql("gen_random_uuid()")
  }
  column "data" {
    type = jsonb
    null = true
  }
  column "timestamp" {
    type = timestamptz
    default = sql("clock_timestamp()")
  }
  primary_key "id" {
    columns = [column.id]
  }
  foreign_key "stream_id" {
    columns = [column.stream_id]
    ref_columns = [table.event_streams.column.id]
    on_delete = CASCADE
  }
  index "stream_id" {
    columns = [column.stream_id]
  }
  index "stream_id_timestamp" {
    columns = [
      column.stream_id,
      column.timestamp,
    ]
  }
}
