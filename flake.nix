{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
    android-nixpkgs = {
      url = "github:tadfisher/android-nixpkgs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      flake-utils,
      naersk,
      nixpkgs,
      nixpkgs-mozilla,
      android-nixpkgs,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [ (import nixpkgs-mozilla) ];
        };
        toolchain =
          (pkgs.rustChannelOf {
            channel = "1.76.0";
            sha256 = "sha256-e4mlaJehWBymYxJGgnbuCObVlqMlQSilZ8FljG9zPHY=";
          }).rust;
        naersk' = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };
        androidSdk = android-nixpkgs.sdk.${system} (
          sdkPkgs: with sdkPkgs; [
            build-tools-35-0-0
            build-tools-34-0-0
            cmdline-tools-latest
            platform-tools
            platforms-android-34
            platforms-android-35
            ndk-25-1-8937393
            ndk-26-1-10909125
            cmake-3-22-1
          ]
        );
      in
      with pkgs;
      rec {
        defaultPackage = naersk'.buildPackage {
          src = ./.;
          buildInputs = [
            pkg-config
            glib
            gtk3
            xdg-utils
            pam
          ];
          nativeBuildInputs = [
            pkg-config
            wrapGAppsHook
            makeWrapper
          ];
          postInstall = ''
            cp -r $src/share $out/share
            wrapProgram "$out/bin/openaws-vpn-client" \
              --set-default OPENVPN_FILE "${openvpn-patched}/bin/openvpn" \
              --set-default SHARED_DIR "$out/share"
          '';
        };
        overlays.default = final: prev: {
          openaws-vpn-client = self.outputs.defaultPackage.${prev.system};
        };
        openvpn-patched = import ./openvpn.nix { inherit (pkgs) fetchpatch openvpn; };
        devShell = mkShell {
          buildInputs = [
            androidSdk
            autoconf
            automake
            libtool
            pkg-config
            glib
            openvpn-patched
            openssl
            lz4
            lzo
            pam
          ];
          nativeBuildInputs = [
            pkg-config
            wrapGAppsHook
            makeWrapper
            toolchain
          ];
        };
      }
    );
}
