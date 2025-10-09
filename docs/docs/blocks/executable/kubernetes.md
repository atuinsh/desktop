# Kubernetes

The Kubernetes block allows you to execute kubectl get commands and display the results in a formatted table directly within your runbook.

### Operating Modes

The Kubernetes block supports two different modes:

1. **Preset Mode** - Select from common kubectl get commands
2. **Custom Mode** - Execute custom kubectl get commands

### Preset Commands

Preset mode provides quick access to common kubectl get operations:

- **Pods** - List all pods in the namespace
- **Services** - List all services
- **Deployments** - List all deployments
- **ConfigMaps** - List all config maps
- **Secrets** - List all secrets
- **Nodes** - List all cluster nodes
- **Namespaces** - List all namespaces

### Custom Commands

Custom mode allows you to execute kubectl get commands with custom parameters. Commands must return JSON output for proper parsing and table display.

```bash
kubectl get pods -l app=nginx -o json
```

### Context and Namespace

Configure the Kubernetes context and namespace for your commands:

- **Context** - Target Kubernetes cluster context
- **Namespace** - Target namespace (if applicable)

### Auto-refresh

Enable automatic command execution at configurable intervals to keep data current. Useful for monitoring resources that change frequently.

### Template Usage

All input fields support [templating](../../templating.md "mention"), allowing you to use variables in your commands:

```bash
kubectl get pods -n {{var.target_namespace}}
```

### Future Enhancements

Currently, the Kubernetes block focuses on `kubectl get` operations for resource inspection and monitoring. We plan to expand Kubernetes functionality in future releases to include:

- Resource creation and modification operations
- Advanced kubectl commands (apply, delete, patch, etc.)
- Helm chart management
- Additional output formats beyond JSON
