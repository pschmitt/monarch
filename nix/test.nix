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
          settings = {
            public_url = "http://machine:8080";
            allow_exec_channels = true;
          };
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
              email = "collector@example.com";
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
    machine.succeed(api + f"-X PATCH -d '{{\"username\":\"alicia\"}}' http://127.0.0.1:8080/api/users/{alice} | jq -e '.username == \"alicia\" and .sso == false'")
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

    # Event kinds can be muted globally; unknown ones are refused.
    kinds = json.loads(machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/events/kinds"))
    assert any(k["kind"] == "exist" for k in kinds), kinds
    machine.succeed(api + "-X PATCH -d '{\"disabled_events\":[\"uptime\"]}' http://127.0.0.1:8080/api/settings | jq -e '.disabled_events == [\"uptime\"]'")
    assert machine.succeed(api + "-o /dev/null -w '%{http_code}' -X PATCH -d '{\"disabled_events\":[\"bogus\"]}' http://127.0.0.1:8080/api/settings") == "400"

    # Channels: secrets are masked, and an exec channel really runs its command.
    machine.succeed(api + "-d '{\"name\":\"ap\",\"kind\":\"apprise\",\"config\":{\"apprise_url\":\"http://127.0.0.1:9/notify/x\"},\"filter\":{\"events\":[\"status\"]}}' http://127.0.0.1:8080/api/channels | jq -e '.config.apprise_url == \"********\" and .filter.events == [\"status\"]'")
    assert machine.succeed(api + "-o /dev/null -w '%{http_code}' -d '{\"name\":\"bad\",\"kind\":\"email\",\"filter\":{\"events\":[\"bogus\"]}}' http://127.0.0.1:8080/api/channels") == "400"
    machine.succeed(api + "-d '{\"name\":\"wp\",\"kind\":\"webpush\"}' http://127.0.0.1:8080/api/channels | jq -e '.kind == \"webpush\"'")
    ex = json.loads(machine.succeed(api + "-d '{\"name\":\"ex\",\"kind\":\"exec\",\"config\":{\"command\":\"echo \\\"$MONARCH_TITLE\\\" > /var/lib/monarch/exec-out\"}}' http://127.0.0.1:8080/api/channels"))
    machine.succeed(api + f"-X POST http://127.0.0.1:8080/api/channels/{ex['id']}/test | jq -e '.ok == true'")
    machine.succeed("grep -q 'Status failed' /var/lib/private/monarch/exec-out")

    # At most one default channel; "no events" and "all events" are different filters.
    d1 = json.loads(machine.succeed(api + "-d '{\"name\":\"d1\",\"kind\":\"webhook\",\"config\":{\"url\":\"http://127.0.0.1:9\"},\"default\":true}' http://127.0.0.1:8080/api/channels"))
    assert d1["default"] is True and d1["filter"]["events"] is None, d1
    d2 = json.loads(machine.succeed(api + "-d '{\"name\":\"d2\",\"kind\":\"webhook\",\"config\":{\"url\":\"http://127.0.0.1:9\"},\"default\":true,\"filter\":{\"events\":[]}}' http://127.0.0.1:8080/api/channels"))
    assert d2["filter"]["events"] == [], d2
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/channels | jq -e '[.[]|select(.default)|.name] == [\"d2\"]'")

    # Notifications are grouped for a configurable window.
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/settings | jq -e '.group_minutes == 10'")
    machine.succeed(api + "-X PATCH -d '{\"group_minutes\":3}' http://127.0.0.1:8080/api/settings | jq -e '.group_minutes == 3'")
    assert machine.succeed(api + "-o /dev/null -w '%{http_code}' -X PATCH -d '{\"group_minutes\":5000}' http://127.0.0.1:8080/api/settings") == "400"

    # Browser push: the VAPID key is served, subscriptions can be added and removed.
    pk = json.loads(machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/push/key"))["public_key"]
    assert len(pk) == 87, pk
    sub = json.loads(machine.succeed(api + "-d '" + json.dumps({"endpoint": "https://push.example.invalid/x", "keys": {"p256dh": pk, "auth": "AAAAAAAAAAAAAAAAAAAAAA"}}) + "' http://127.0.0.1:8080/api/push/subscriptions"))
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/push/subscriptions | jq -e 'length == 1'")
    assert machine.succeed(api + "-o /dev/null -w '%{http_code}' -d '{\"endpoint\":\"http://insecure/x\",\"keys\":{\"p256dh\":\"x\",\"auth\":\"y\"}}' http://127.0.0.1:8080/api/push/subscriptions") == "400"
    machine.succeed(api + f"-X DELETE http://127.0.0.1:8080/api/push/subscriptions/{sub['id']}")

    # Accounts have an email address (declared, or set later); bad ones are refused.
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/users | jq -e '[.[]|select(.username==\"collector\")][0].email == \"collector@example.com\"'")
    machine.succeed(api + f"-X PATCH -d '{{\"email\":\"alicia@example.com\"}}' http://127.0.0.1:8080/api/users/{alice} | jq -e '.email == \"alicia@example.com\"'")
    assert machine.succeed(api + f"-o /dev/null -w '%{{http_code}}' -X PATCH -d '{{\"email\":\"not-an-address\"}}' http://127.0.0.1:8080/api/users/{alice}") == "400"

    # The monit agent registers itself and reports all of its services.
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts | jq -e 'length == 1 and .[0].services.total == 4'",
        timeout=60,
    )
    host = json.loads(machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts/1"))
    assert host["can_act"], host
    assert {s["type"] for s in host["services"]} == {"system", "process", "filesystem", "program"}

    # The services overview lists every check, filterable by state and name.
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/services | jq -e '(.services | length) == 4 and (.counts | add) == 4'")
    machine.succeed("curl -sf -b /tmp/cj 'http://127.0.0.1:8080/api/services?state=failed' | jq -e '.services | all(.state == \"failed\")'")
    machine.succeed("curl -sf -b /tmp/cj 'http://127.0.0.1:8080/api/services?q=hello' | jq -e '(.services | length) == 1 and .services[0].name == \"hello\" and .services[0].host_id == 1'")

    # Actions are relayed to the agent's HTTP interface.
    machine.succeed(
        "curl -sf -b /tmp/cj -H 'content-type: application/json' -d '{\"action\":\"unmonitor\"}' "
        + "http://127.0.0.1:8080/api/hosts/1/services/hello/action"
    )
    machine.wait_until_succeeds(
        "curl -sf -b /tmp/cj http://127.0.0.1:8080/api/hosts/1/services/hello | jq -e '.state == \"unmonitored\"'",
        timeout=30,
    )

    # Per-check alert settings override the generic ones and show up in the overview.
    ca = "http://127.0.0.1:8080/api/hosts/1/services/hello/alerts"
    machine.succeed(api + f"-X PUT -d '{{\"muted\":false,\"events\":[\"status\"],\"channels\":null}}' {ca} | jq -e '.events == [\"status\"]'")
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/checks/alerts | jq -e 'length == 1 and .[0].service == \"hello\" and .[0].host != null'")
    assert machine.succeed(api + f"-o /dev/null -w '%{{http_code}}' -X PUT -d '{{\"events\":[\"bogus\"]}}' {ca}") == "400"
    assert machine.succeed(api + "-o /dev/null -w '%{http_code}' -X PUT -d '{\"muted\":true}' http://127.0.0.1:8080/api/hosts/1/services/nope/alerts") == "404"
    machine.succeed(api + f"-X PUT -d '{{\"muted\":false,\"events\":null,\"channels\":null}}' {ca} | jq -e '.events == null'")
    machine.succeed("curl -sf -b /tmp/cj http://127.0.0.1:8080/api/checks/alerts | jq -e 'length == 0'")

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
    # Pull polls over SSH can time out when the build host is overloaded; the explicit
    # poll above covers that path, so only ingest (database) failures are fatal.
    machine.fail("journalctl -u monarch.service | grep -q 'ingest failed'")

    # The web UI is served.
    machine.succeed("curl -sf http://127.0.0.1:8080/ | grep -qi monarch")
  '';
}
