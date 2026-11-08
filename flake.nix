{
  description = "Roastwarden development shell (Rust server + SvelteKit dashboard)";

  # nixos-unstable at a rev with rustc 1.99, node 22.23 and cargo-audit 0.22. The firmware
  # repository (Esp32_FluidBedCoffeeRoaster) stays on e72e4f29: PlatformIO's venv in
  # ~/.platformio is built against that rev's Python and fails to load on this one.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/02c087f3fe673c9e317f206a627cb6b910336dbe";

  outputs = {nixpkgs, ...}: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forAll = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs {inherit system;}));

    # roastwarden.com's search indexer at its newest release (nixpkgs at this rev has 1.5.2), the
    # version the site's build uses.
    pagefindFor = pkgs: let
      arch = {
        "x86_64-linux" = { triple = "x86_64-unknown-linux-musl"; sha256 = "004d40c9a644c995dadc42b8031196c6f17e7a6dacf9a6c15c63286dd6ead5f2"; };
        "aarch64-linux" = { triple = "aarch64-unknown-linux-musl"; sha256 = "ad286f25e3109a2c6c740bb46998f75e1eae93df41f64c4699f0c2bc8db101ce"; };
      }.${pkgs.stdenv.hostPlatform.system};
    in
      pkgs.stdenvNoCC.mkDerivation rec {
        pname = "pagefind";
        version = "1.6.0";
        src = pkgs.fetchurl {
          url = "https://github.com/Pagefind/pagefind/releases/download/v${version}/pagefind-v${version}-${arch.triple}.tar.gz";
          inherit (arch) sha256;
        };
        sourceRoot = ".";
        installPhase = "install -Dm755 pagefind $out/bin/pagefind";
      };
  in {
    devShells = forAll (pkgs: {
      default = pkgs.mkShell {
        packages = with pkgs; [
          # Rust
          cargo
          rustc
          clippy
          rustfmt
          rust-analyzer
          cargo-audit
          cargo-deny # licences (deny.toml), as CI's Audit job
          cargo-about # third-party notices (scripts/notices.sh)
          cargo-machete # unused dependencies, as CI's Lint job (0.9.2)

          # Dashboard
          nodejs_22

          # roastwarden.com's build: the versions the site's build uses
          zola # 0.23.6, the newest release
          (pagefindFor pkgs)

          # Tooling used by the server build and local development
          pkg-config
          sqlite
          python3 # tools/mock-device
          mosquitto # local broker for manual testing
          socat # pty pairs for serial transport tests
        ];

        env.RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";

        shellHook = ''
          echo "Roastwarden dev shell: $(cargo --version), node $(node --version)"
        '';
      };
    });
  };
}
