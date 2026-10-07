{ self }:
{
  name = "monarch";

  nodes.machine =
    { pkgs, ... }:
    {
      imports = [ ./module.nix ];

      services.monarch = {
        enable = true;
        package = self.packages.${pkgs.stdenv.hostPlatform.system}.monarch;
        settings.public_url = "http://machine:8080";
        initialAdmin.passwordFile = pkgs.writeText "pw" "supersecret";
      };

      services.monit = {
        enable = true;
        config = ''
          set daemon 5
          set mmonit http://admin:supersecret@127.0.0.1:8080/collector
          set httpd port 2812 address 127.0.0.1
            allow monit:monitpass
          check system $HOST
          check process monarch matching "monarch"
          check filesystem root with path /
          check program hello with path "${pkgs.coreutils}/bin/echo hello"
            if status != 0 then alert
        '';
      };

      environment.systemPackages = [
        pkgs.curl
        pkgs.jq
      ];
    };

  testScript = ''
    import json

    machine.wait_for_unit("monarch.service")
    machine.wait_for_open_port(8080)
    machine.wait_for_unit("monit.service")

    login = "curl -sf -c /tmp/cj -H 'content-type: application/json' -d '{\"username\":\"admin\",\"password\":\"supersecret\"}' http://127.0.0.1:8080/api/auth/login"
    machine.succeed(login)

    # The monit agent registers itself and reports all of its services.
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts | jq -e 'length == 1 and .[0].services.total == 4'",
        timeout=60,
    )
    host = json.loads(machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts/1"))
    assert host["can_act"], host
    assert {s["type"] for s in host["services"]} == {"system", "process", "filesystem", "program"}

    # Actions are relayed to the agent's HTTP interface.
    machine.succeed(
        "curl -sf -b /tmp/cj -H 'content-type: application/json' -d '{\"action\":\"unmonitor\"}' "
        + "http://127.0.0.1:8080/api/hosts/1/services/hello/action"
    )
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts/1/services/hello | jq -e '.state == \"unmonitored\"'",
        timeout=30,
    )

    # Metrics are recorded.
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj 'http://127.0.0.1:8080/api/metrics?host=1&service=machine&metrics=cpu,load1' | jq -e '.series[0].points | length > 0'",
        timeout=30,
    )

    # The web UI is served.
    machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -qi monarch")
  '';
}
