{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, nixpkgs, nixos-generators, ... }:
    let system = "x86_64-linux";
    in {
      packages.${system} = {
        linode = nixos-generators.nixosGenerate {
          inherit system;
          modules = [ ./configuration.nix ];
          format = "linode";
        };
      };
    };
}
