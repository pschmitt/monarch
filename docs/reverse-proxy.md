# Reverse proxy

Monarch works behind nginx or similar. Forward `X-Forwarded-For` (used to find
agents' addresses) and `X-Forwarded-Proto` (for secure cookies), and disable
buffering for `/api/stream`:

```nginx
location / {
  proxy_pass http://127.0.0.1:8080;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto $scheme;
  proxy_buffering off;
}
```

On NixOS, the module sets up an ACME-enabled nginx virtual host with these
settings:

```nix
services.monarch.nginx = {
  enable = true;
  domain = "monarch.example.com";
};
```
