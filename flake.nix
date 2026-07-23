{
  description = "disk-hog-backup - back up to external disk, making best use of the available space (prebuilt binary from GitHub releases)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      version = "0.8.1";

      assets = {
        x86_64-linux = {
          name = "disk-hog-backup-linux-x86_64.tar.gz";
          sha256 = "4b08da3e748e4981e17221c59e6dc0ba41954dbd173833722bde0d2d67ef0bc0";
        };
      };

      systems = builtins.attrNames assets;
      forAllSystems = nixpkgs.lib.genAttrs systems;

      mkPackage = system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          asset = assets.${system};
        in
        pkgs.stdenvNoCC.mkDerivation {
          pname = "disk-hog-backup";
          inherit version;

          src = pkgs.fetchurl {
            url = "https://github.com/timabell/disk-hog-backup/releases/download/v${version}/${asset.name}";
            sha256 = asset.sha256;
          };

          dontUnpack = true;

          installPhase = ''
            runHook preInstall
            mkdir -p $out/bin
            tar -xzf $src -C $out/bin disk-hog-backup
            chmod +x $out/bin/disk-hog-backup
            runHook postInstall
          '';

          meta = with pkgs.lib; {
            description = "Back up to external disk, making best use of the available space - CLI tool";
            homepage = "https://github.com/timabell/disk-hog-backup";
            license = licenses.agpl3Only;
            platforms = systems;
            mainProgram = "disk-hog-backup";
          };
        };
    in
    {
      packages = forAllSystems (system: {
        default = mkPackage system;
        disk-hog-backup = mkPackage system;
      });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${mkPackage system}/bin/disk-hog-backup";
        };
      });
    };
}
