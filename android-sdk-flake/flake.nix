{
  description = "Android SDK for Dioxus mobile builds (using tadfisher/android-nixpkgs for proper Apple Silicon emulator support)";

  inputs = {
    android-nixpkgs = {
      url = "github:tadfisher/android-nixpkgs/stable";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      android-nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ]
      (system: {
        packages = rec {
          android-sdk = android-nixpkgs.sdk.${system} (
            sdkPkgs: with sdkPkgs; [
              # Command line tools (required for a working SDK)
              cmdline-tools-latest
              platform-tools

              # Build tools — AGP 8.7.0 requires 34.0.0, Dioxus also pulls 35.0.1
              build-tools-34-0-0
              build-tools-35-0-1

              # Platform — Dioxus 0.7 uses compileSdk = 33
              platforms-android-33

              # NDK — same version as previously used via devbox
              ndk-29-0-14206865

              # Emulator with ARM64 system images (Google APIs, no Play Store)
              emulator
              system-images-android-33-google-apis-arm64-v8a
            ]
          );
          default = android-sdk;
        };
      });
}
