{ config, lib, pkgs, ... }:

{
  system.stateVersion = "22.11";
  environment.systemPackages = with pkgs; [ cowsay ];
}
