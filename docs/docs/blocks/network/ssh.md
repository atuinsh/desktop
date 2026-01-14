# SSH

SSH is another "contextual" block - just like setting a directory or environment variable.

<figure class="img-light">
  <picture>
    <img src="../../../images/ssh-1-light.png" alt="SSH Block">
  </picture>
  <figcaption>Example SSH usage</figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/ssh-1-dark.png" alt="SSH Block">
  </picture>
  <figcaption>Example SSH usage</figcaption>
</figure>

Insert an SSH block, and fill in the host. All following [terminal](../executable/terminal.md "mention") or [script](../executable/script.md "mention") blocks will now execute on the remote machine. If the block is going to execute on a remote machine, it will be outlined in blue.

### Settings

Click the gear icon on any SSH block to open the settings modal. This provides fine-grained control over connection and authentication settings.

#### Connection Settings

Override the connection details for this block. When set, these take precedence over the quick input field:

- **User** - The SSH username (e.g., `root`, `ubuntu`)
- **Hostname** - The remote host to connect to
- **Port** - Custom SSH port (defaults to 22)

These settings are synced with the runbook, so collaborators will see the same connection configuration.

#### Identity Key

Specify a private key for authentication. When configured, this key is tried **first**, before SSH agent or config defaults. If the specified key fails, authentication will not fall back to other methods.

Three modes are available:

- **Use SSH config/agent (default)** - Standard authentication order (agent, config, default keys)
- **Specify key path** - Select a key from `~/.ssh/` or browse to a custom location
- **Paste key content** - Paste the private key content directly

!!! note "Identity key storage"
    Identity key settings are stored **locally on your machine only** and are not synced to other users. This means each team member can use their own credentials for the same runbook. The security posture is equivalent to having key files on disk.

### Authentication

SSH authentication works like the standard `ssh` command, trying methods in this order:

1. **SSH Agent** - All keys loaded in your SSH agent
2. **SSH Config** - Identity files specified in `~/.ssh/config`
3. **Default Keys** - Standard SSH keys in `~/.ssh/`:
   - `id_rsa`
   - `id_ecdsa`
   - `id_ecdsa_sk` (FIDO/U2F security key)
   - `id_ed25519`
   - `id_ed25519_sk` (FIDO/U2F security key)
   - `id_xmss`
   - `id_dsa`

Tailscale SSH works as expected. Password-protected keys must be added to your SSH agent with `ssh-add`, or you can specify a key path in the [settings modal](#settings).

Your SSH config is fully respected, including `ProxyJump`, `ProxyCommand`, `IdentityAgent`, and other options.

### Running locally

If you wish to revert back to local execution, insert a "host" block.

<figure class="img-light">
  <picture>
    <img src="../../../images/ssh-2-light.png" alt="SSH Block">
  </picture>
  <figcaption>Example SSH usage</figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/ssh-2-dark.png" alt="SSH Block">
  </picture>
  <figcaption>Example SSH usage</figcaption>
</figure>

In the future, the host block will support toggling between different connected machines. Right now, it only supports localhost

### Connection pooling

Our SSH integration will only open a single SSH connection, and multiplex multiple sessions through it. This means that once connected, block execution will feel fast + low latency
