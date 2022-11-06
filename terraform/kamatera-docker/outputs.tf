output "public_ip" {
  value = one(kamatera_server.my_docker_server.public_ips)
}
