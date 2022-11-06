terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "4.42.0"
    }
  }
}

provider "google" {
  project = "openmi-cc"
  region  = "us-west1"
}

resource "google_cloud_run_service" "default" {
  name     = "cloudrun-srv"
  location = "us-west1"

  template {
    spec {
      containers {
        image = "us-docker.pkg.dev/cloudrun/container/hello"
      }
    }
  }
}
