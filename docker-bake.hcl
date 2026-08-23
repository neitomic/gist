variable "IMAGE" {
  default = "ghcr.io/neitomic/gist"
}

variable "TAG" {
  default = "local"
}

group "default" {
  targets = ["image"]
}

target "image" {
  context    = "."
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64", "linux/arm64"]
  tags       = ["${IMAGE}:${TAG}"]
}
