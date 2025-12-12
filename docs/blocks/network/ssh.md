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

Tailscale SSH works as expected. Password-protected keys must be added to your SSH agent with `ssh-add`, though soon we will support full config in the UI.

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
