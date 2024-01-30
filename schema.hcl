schema "public" {
  comment = "standard public schema"
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
  index "stream_id_name" {
    columns = [column.stream_id, column.name,]
  }
  index "stream_id_timestamp" {
    columns = [column.stream_id, column.timestamp,]
  }
}
