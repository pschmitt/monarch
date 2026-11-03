# Single sign-on

Monarch supports sign-in through OpenID Connect providers such as Authelia,
Authentik or Keycloak, with Monarch roles mapped from the provider's groups.

Register a confidential client with the redirect URI
`https://<public_url>/api/auth/oidc/callback`, then configure it. On NixOS:

```nix
services.monarch = {
  oidc.clientSecretFile = "/run/secrets/monarch-oidc-secret";
  settings.oidc = {
    issuer = "https://auth.example.com";
    client_id = "monarch";
    display_name = "Authelia";
    admin_groups = [ "admins" ];
    operator_groups = [ "ops" ];
    default_role = "viewer";   # or "none" to deny everyone else
    link_local_users = true;   # first sign-in adopts the local account with the same username
  };
  # settings.disable_password_login = true;
};
```

The same keys go under `[oidc]` in the [configuration file](configuration.md).
