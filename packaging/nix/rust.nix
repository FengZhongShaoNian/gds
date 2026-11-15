{ lib
, stdenv
, fetchurl
, autoPatchelfHook
, bash
, gcc
, zlib
, writeText
, makeRustPlatform
}:
let
  version = "1.100.0";
  targets = {
    x86_64-linux = {
      triple = "x86_64-unknown-linux-gnu";
      sha256 = "98ee6bec240e1cfe944f17e7092412dc5ef6e008484bb668c6936d35a7d15acd";
    };
    aarch64-linux = {
      triple = "aarch64-unknown-linux-gnu";
      sha256 = "a21f6a21818df07c1e2e7addb570af5444b3fa9dd61e3a207f4fb1c5a5e665c9";
    };
  };
  target = targets.${stdenv.hostPlatform.system}
    or (throw "Unsupported Rust host system: ${stdenv.hostPlatform.system}");

  setupHook = writeText "rust-setup-hook.sh" ''
    if [[ -z ''${IN_NIX_SHELL-} && -z ''${CARGO_HOME-} ]]; then
      export CARGO_HOME=$TMPDIR
    fi
  '';

  toolchain = stdenv.mkDerivation {
    pname = "rust-toolchain";
    inherit version;
    src = fetchurl {
      url = "https://static.rust-lang.org/dist/2026-11-12/rust-${version}-${target.triple}.tar.xz";
      inherit (target) sha256;
    };
    nativeBuildInputs = [ autoPatchelfHook ];
    buildInputs = [ bash gcc.cc.lib zlib ];
    depsHostHostPropagated = [ stdenv.cc ];
    propagatedBuildInputs = [ stdenv.cc ];
    setupHooks = [ setupHook ];
    dontStrip = true;
    postPatch = ''
      patchShebangs .
    '';
    installPhase = ''
      ./install.sh --prefix=$out \
        --components=rustc,cargo,clippy-preview,rustfmt-preview,rust-std-${target.triple}
    '';
    passthru = {
      targetPlatforms = lib.platforms.linux;
      tier1TargetPlatforms = lib.platforms.linux;
      badTargetPlatforms = [ ];
    };
    meta = {
      description = "Official Rust ${version} toolchain";
      homepage = "https://www.rust-lang.org/";
      license = [ lib.licenses.mit lib.licenses.asl20 ];
      platforms = [ "x86_64-linux" "aarch64-linux" ];
    };
  };
in
{
  inherit toolchain;
  rustPlatform = makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
}
