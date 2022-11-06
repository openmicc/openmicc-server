
# terraform {
#   required_providers {
#     kamatera = {
#       source = "Kamatera/kamatera"
#     }
#   }
#   required_providers {
#     docker = {
#       source  = "kreuzwerker/docker"
#       version = "2.22.0"
#     }
#   }
# }

module "docker" {
  source                 = "./kamatera-docker"
  kamatera_api_client_id = var.kamatera_api_client_id
  kamatera_api_secret    = var.kamatera_api_secret
}
