terraform {
  required_providers {
    linode = {
      source  = "linode/linode"
      version = "1.27.1"
    }
  }
}

provider "linode" {
  token = var.linode_token
}

resource "linode_instance" "terraform-web" {
  image           = "private/17810396"
  label           = "Terraform-Web-Example"
  group           = "Terraform"
  region          = "us-west"
  type            = "g6-nanode-1"
  authorized_keys = [var.ssh_key]
  root_pass       = var.root_pass
}

variable "linode_token" {
  description = "Linode API token"
  type        = string
  sensitive   = true
}

variable "root_pass" {
  description = "Linode root password"
  type        = string
  sensitive   = true
}

variable "ssh_key" {
  description = "SSH key for Linode"
  type        = string
  sensitive   = true
}
