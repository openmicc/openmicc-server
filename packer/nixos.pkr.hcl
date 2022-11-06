packer {
  required_plugins {
    kamatera = {
      version = ">= 0.5.0"
      source  = "github.com/kamatera/kamatera"
    }
  }
}

source "kamatera" "ubuntu" {
  datacenter = "US-SC"
  ssh_username = "root"
  image_name = "nixos-example-packer-image"
  image = "ubuntu_server_18.04_64-bit"
  script = <<-EOF
    curl https://raw.githubusercontent.com/elitak/nixos-infect/master/nixos-infect | NIX_CHANNEL=nixos-22.05 bash -x
  EOF
  api_client_id = var.kamatera_api_client_id
  api_secret = var.kamatera_api_secret
}

build {
  sources = [
    "source.kamatera.ubuntu"
  ]
}
