variable "REGISTRY" { default = "ghcr.io/hexze/coral" }
variable "SHA"      { default = "" }

group "default" {
  targets = ["coral-api", "coral-bot", "coral-admin", "coral-verify", "coral-postgres"]
}

target "coral-api" {
  target = "coral-api"
  tags   = compact(["${REGISTRY}/coral-api:latest", SHA != "" ? "${REGISTRY}/coral-api:${SHA}" : ""])
}

target "coral-bot" {
  target = "coral-bot"
  tags   = compact(["${REGISTRY}/coral-bot:latest", SHA != "" ? "${REGISTRY}/coral-bot:${SHA}" : ""])
}

target "coral-admin" {
  target = "coral-admin"
  tags   = compact(["${REGISTRY}/coral-admin:latest", SHA != "" ? "${REGISTRY}/coral-admin:${SHA}" : ""])
}

target "coral-verify" {
  target = "coral-verify"
  tags   = compact(["${REGISTRY}/coral-verify:latest", SHA != "" ? "${REGISTRY}/coral-verify:${SHA}" : ""])
}

target "coral-postgres" {
  target = "coral-postgres"
  tags   = compact(["${REGISTRY}/coral-postgres:latest", SHA != "" ? "${REGISTRY}/coral-postgres:${SHA}" : ""])
}
