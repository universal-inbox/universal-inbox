{
  description = "Android SDK for Dioxus mobile builds";

  inputs = {
    # Pin to the same nixpkgs commit that devbox uses
    nixpkgs.url = "github:NixOS/nixpkgs/01b6809f7f9d1183a2b3e081f0a1e6f8f415cb09";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            config = {
              allowUnfree = true;
              android_sdk.accept_license = true;
            };
          };

          androidComposition = pkgs.androidenv.composeAndroidPackages {
            # Dioxus 0.7 uses compileSdk = 33, AGP 8.7.0
            platformVersions = [ "33" ];
            buildToolsVersions = [
              "34.0.0"
              "35.0.1"
            ];
            platformToolsVersion = "36.0.0";
            cmdLineToolsVersion = "latest";

            # NDK is managed separately via devbox (androidenv.androidPkgs.ndk-bundle)
            includeNDK = false;

            includeEmulator = false;
            includeSystemImages = false;
            includeSources = false;
          };
        in
        {
          android-sdk = androidComposition.androidsdk;
          default = androidComposition.androidsdk;
        }
      );
    };
}
