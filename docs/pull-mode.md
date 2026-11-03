# Pull mode

In pull mode Monarch polls an agent's Monit HTTP interface instead of waiting
for reports. The connection can go through SSH, so `set httpd` can stay bound
to localhost on the agent.

Add a host under *Hosts → Add host → Monarch pulls from agent*, or
declaratively on NixOS:

```nix
services.monarch = {
  ssh.privateKeyFile = "/run/secrets/monarch-ssh-key";
  targets = [
    {
      name = "router";
      url = "http://127.0.0.1:2812";        # as seen from the SSH host
      username = "monit";
      passwordFile = "/run/secrets/monit-httpd";
      ssh.destination = "root@router.lan";
      interval = 30;
    }
  ];
};
```

Pulled status documents carry no events, so Monarch derives them from state
changes between polls. Service actions use the same connection.
