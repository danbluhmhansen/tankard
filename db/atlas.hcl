env "local" {
  src = "file://schema.hcl"
  url = "postgres://localhost:28816/tankard?sslmode=disable"
  dev = "postgres://localhost:28816/tankard_dev?sslmode=disable"
}
