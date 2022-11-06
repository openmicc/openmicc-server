
# Generated by https://kamatera.github.io/kamateratoolbox/serverconfiggen.html?configformat=terraform
terraform {
  required_providers {
    kamatera = {
      source = "Kamatera/kamatera"
    }
  }
  backend "local" {
    path = "terraform.tfstate"
  }
}

# Kamatera

provider "kamatera" {
  api_client_id = var.kamatera_api_client_id
  api_secret    = var.kamatera_api_secret
}

resource "kamatera_server" "my_docker_server" {
  name                    = "my_docker_server"
  datacenter_id           = "US-SC"
  image_id                = "US-SC:6000C29b795d5c7d0176eaf7ad741c96" # docker-ubuntu-22.04
  cpu_type                = "A"
  cpu_cores               = 1
  ram_mb                  = 1024
  disk_sizes_gb           = [20]
  billing_cycle           = "monthly"
  ssh_pubkey              = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC2PIjYiTmqkaloxr7HdjipLdEq4fvnK+N19oYf7Ptcos8eIM1DdfCOa/fGXwq18wZ8dwUBlwZxON/EmwfeN3voNrM8KgP4fZ9Ae/6S0oH3j+gaHfnHNz2q/4JU9b/YCVpTnUwGrgQDm39iAL6rSu8ynzUzyUAwFYy5pejF5cXq8vjrrV55u1Zcmv8sZluG4K3ywcyH78XTxIZXPM1KSRcxwt6FkQTnzYzPeX1Am9FCdeDUvpSYtCykkogio5D6x+TvY7BFhdEQtFBAEgaoXiMFkn4OSDQDaud8lnJu8gIDZ7hY33SctQ3wRsYqULXoQBye7dB0EEW2iQ23gd0ak+jP oliver@oliver-x1c7-arch"
  monthly_traffic_package = "t5000"

  # this attaches a public network to the server
  # which will also allocate a public IP
  network {
    name = "wan"
  }

  # # attach a private network with auto-allocated ip from the available ips in that network
  # network {
  #   name = resource.kamatera_network.my_private_network.full_name
  # }
}