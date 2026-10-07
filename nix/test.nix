{ self }:
{
  name = "monarch";

  nodes.machine =
    { pkgs, ... }:
    let
      # test-only key pair for the SSH pull connection
      sshKey = pkgs.runCommand "monarch-test-ssh-key" { nativeBuildInputs = [ pkgs.openssh ]; } ''
        mkdir $out
        ssh-keygen -q -t ed25519 -N "" -C monarch-test -f $out/id_ed25519
      '';
    in
    {
      imports = [ ./module.nix ];

      users.users.root.openssh.authorizedKeys.keyFiles = [ "${sshKey}/id_ed25519.pub" ];

      services = {
        openssh.enable = true;

        monarch = {
          enable = true;
          package = self.packages.${pkgs.stdenv.hostPlatform.system}.monarch;
          settings.public_url = "http://machine:8080";
          initialAdmin.passwordFile = pkgs.writeText "pw" "supersecret";
          ssh.privateKeyFile = "${sshKey}/id_ed25519";
          targets = [
            {
              name = "self-via-ssh";
              url = "http://127.0.0.1:2812";
              username = "monit";
              passwordFile = pkgs.writeText "monit-pw" "monitpass";
              ssh.destination = "root@127.0.0.1";
              interval = 5;
            }
          ];
          ensureUsers = [
            {
              username = "collector";
              passwordFile = pkgs.writeText "collector-pw" "collectorpass";
            }
          ];
        };

        monit = {
          enable = true;
          config = ''
            set daemon 5
            set mmonit http://collector:collectorpass@127.0.0.1:8080/collector
            set httpd port 2812 address 127.0.0.1
              allow monit:monitpass
            check system $HOST
            check process monarch matching "monarch"
            check filesystem root with path /
            check program hello with path "${pkgs.coreutils}/bin/echo hello"
              if status != 0 then alert
          '';
        };
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

    # Users can be renamed (by an admin, or themselves); duplicates are refused.
    api = "curl -s -b /tmp/cj -H 'content-type: application/json' "
    alice = json.loads(machine.succeed(api + "-d '{\"username\":\"alice\",\"password\":\"alicepass1\",\"role\":\"viewer\"}' http://127.0.0.1:8080/api/users"))["id"]
    machine.succeed(api + f"-X PATCH -d '{{\"username\":\"alicia\"}}' http://127.0.0.1:8080/api/users/{alice} | jq -e '.username == \"alicia\"'")
    assert machine.succeed(api + f"-o /dev/null -w '%{{http_code}}' -X PATCH -d '{{\"username\":\"ADMIN\"}}' http://127.0.0.1:8080/api/users/{alice}") == "409"
    machine.succeed("curl -sf -c /tmp/cj2 -H 'content-type: application/json' -d '{\"username\":\"alicia\",\"password\":\"alicepass1\"}' http://127.0.0.1:8080/api/auth/login")
    machine.succeed("curl -sf -b /tmp/cj2 -H 'content-type: application/json' -X PATCH -d '{\"username\":\"ali\"}' http://127.0.0.1:8080/api/users/me | jq -e '.username == \"ali\"'")

    # API tokens authenticate as their owner, and cannot manage tokens themselves.
    tok = json.loads(machine.succeed(api + "-d '{\"name\":\"test\",\"expires_days\":1}' http://127.0.0.1:8080/api/tokens"))
    assert tok["token"].startswith("mnr_"), tok
    machine.succeed(f"curl -sf -H 'Authorization: Bearer {tok['token']}' http://127.0.0.1:8080/api/users | jq -e 'length >= 3'")
    assert machine.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'Authorization: Bearer {tok['token']}' http://127.0.0.1:8080/api/tokens") == "403"
    assert machine.succeed("curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer mnr_bogus' http://127.0.0.1:8080/api/users") == "401"
    machine.succeed(api + f"-X DELETE http://127.0.0.1:8080/api/tokens/{tok['id']}")
    assert machine.succeed(f"curl -s -o /dev/null -w '%{{http_code}}' -H 'Authorization: Bearer {tok['token']}' http://127.0.0.1:8080/api/users") == "401"

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

    # Pull mode through SSH works from within the hardened unit.
    machine.wait_for_unit("sshd.service")
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj http://127.0.0.1:8080/api/targets | jq -e '.[0].last_status == \"ok\" and .[0].managed'",
        timeout=60,
    )
    machine.succeed(
        "curl -sf -b /tmp/cj -X POST http://127.0.0.1:8080/api/targets/1/poll | jq -e '.host_id == 1'"
    )

    # Concurrent push and pull reports never fail to ingest.
    machine.fail("journalctl -u monarch.service | grep -q 'ingest failed\\|poll failed'")

    # The web UI is served.
    machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -qi monarch")
  '';
}
