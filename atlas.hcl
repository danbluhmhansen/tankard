env "local" {
  src = "file://schema.hcl"
  url = getenv("DATABASE_URL")
  dev = getenv("DATABASE_DEV_URL")
}
