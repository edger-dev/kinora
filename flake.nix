{
  description = "Kinora — an agent-native knowledge system where ideas move, connect, and compose";

  inputs.jig.url = "github:edger-dev/jig";

  outputs = { self, jig }:
    jig.lib.mkWorkspace
      {
        pname = "kinora";
        src = ./.;
        # extraDevPackages = pkgs: [ ];
      }
      {
        rust = {
          # buildPackages = [ "kinora-cli" ];  # omit to build whole workspace
          # wasm = true;
        };
      };
}
