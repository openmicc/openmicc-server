terraform {
  required_version = ">= 0.14.0"
  required_providers {
    sops = {
      source  = "carlpett/sops"
      version = "~> 0.5"
    }
    github = {
      source  = "integrations/github"
      version = "5.7.0"
    }
  }
}

data "sops_file" "secrets" {
  source_file = "../../secrets.yaml"
}

provider "github" {
  token = data.sops_file.secrets.data["github.token"]
  owner = "openmicc"
}

data "external" "github-repo-name" {
  program = ["gh", "repo", "view", "--json", "nameWithOwner"]
}

data "github_repository" "github-origin" {
  full_name = data.external.github-repo-name.result["nameWithOwner"]
}

output "github-repo" {
  value = data.github_repository.github-origin.id
}

resource "github_actions_secret" "cachix-auth-token" {
  repository      = data.github_repository.github-origin.id
  secret_name     = "CACHIX_AUTH_TOKEN"
  plaintext_value = data.sops_file.secrets.data["cachix.auth_token"]
}
