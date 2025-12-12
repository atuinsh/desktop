---
description: Workspaces are the highest level container for runbooks
---

Workspaces are the highest level container for runbooks. They can be found in the left sidebar in the Desktop app.

<figure class="img-light">
  <picture>
    <img src="../images/workspaces-light.png" alt="Collaborations">
  </picture>
  <figcaption>Operations, Internal Infra, and Meetings are all workspaces</figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../images/workspaces-dark.png" alt="Collaborations">
  </picture>
  <figcaption>Operations, Internal Infra, and Meetings are all workspaces</figcaption>
</figure>

You can create a new workspace one of three ways:

1. Choose <code>File → Workspaces → New Workspace</code> from the application menu
2. Choose "New Workspace" from the "New Runbook" dropdown arrow
3. Right-click in an empty area of the sidebar and choose "New Workspace"

Each workspace is either <strong>online</strong> or <strong>offline</strong>.

## Online Workspaces

Online workspaces are synced to [Atuin Hub](../hub/getting-started), and all runbooks added to them are automatically saved to the Hub. You must be online and logged in to the Hub to create or edit online workspaces, so [check out the getting started guide for more information](../hub/getting-started).

Runbooks in online workspaces can be edited at the same time by multiple collaborators or team members; see [the collaborative editing guide](../hub/collaborative-editing) for more information.

## Offline Workspaces

Offline workspaces are not synced to the Hub, and all runbooks added to them are saved locally on your device. When you create an offline workspace, you will be prompted to choose a folder on your device in which to store the runbooks.

While offline runbooks cannot be edited collaboratively inside the editor like online runbooks can be, the workspace folder can be managed by Git or another VCS for version control, VCS-based collaboration, and backup.
