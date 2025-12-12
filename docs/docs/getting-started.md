---
description: Create your first executable runbook with blocks, templating, and serial execution.
---

# :material-rocket-launch: Getting Started with Atuin Desktop

Welcome to Atuin Desktop! This guide will help you understand runbooks and get you up and running quickly.

## What are Runbooks?

A runbook is a living document that combines documentation with executable automation. Think of it as your operations playbook that can actually *do* things, not just tell you what to do.

Start with plain English and evolve your runbooks over time - from simple checklists and documentation, to workflows mixing manual steps with automated scripts, all the way to fully automated processes requiring minimal human intervention.

Runbooks adapt to your workflow, not the other way around. Whether you're documenting incident response procedures, deployment workflows, or system maintenance tasks, Atuin Desktop scales with your needs.

## :material-file-document: Creating Your First Runbook

### Runbook Structure

Every runbook starts with a **name** - this is simply the first text in your document, whether it's a heading or paragraph:

```markdown
# Database Maintenance Workflow
```

or

```markdown
This runbook handles our weekly database cleanup tasks.
```

### :material-language-markdown: Markdown Support  

Our editor supports most standard Markdown syntax, so you can create rich documentation alongside your executable blocks:

- **Headers** for organization
- **Lists** for checklists  
- **Code blocks** for examples
- **Links** for references
- **Tables** for structured data

## :material-cube: Working with Blocks

Blocks are the heart of Atuin Desktop - they're interactive components that can execute commands, query databases, make HTTP requests, and much more.

### Creating Blocks

To create a new block, simply type `/` anywhere in your runbook. This opens the block picker where you can choose from our extensive library of blocks organized into categories:

- **:material-play: Executable**: Scripts, terminals, and command execution
- **:material-database: Databases**: Query MySQL, PostgreSQL, ClickHouse, SQLite
- **:material-network: Network**: HTTP requests, SSH connections  
- **:material-monitor: Monitoring**: Prometheus metrics and system monitoring

### Template Variables

Make your runbooks dynamic with template variables:

```handlebars
{{var.environment}} - References a variable named "environment"
{{env.DATABASE_URL}} - References an environment variable
```

For example, you can create flexible database queries:

```sql
SELECT * FROM users 
WHERE created_at > '{{var.start_date}}'
AND status = '{{var.user_status}}';
```

## :material-play: Serial Execution

One of Atuin Desktop's most powerful features is **serial execution** - run your entire runbook automatically by clicking the play button in the top right.

!!! warning "Terminal Block Caveat"
    Terminal blocks must explicitly exit (include `exit` in your commands) for serial execution to continue automatically.

## Next Steps

Ready to dive deeper? Explore our comprehensive block documentation:

- [All Blocks Overview](blocks/index.md) - Understand how blocks work and fit together
- [Database Blocks](blocks/databases/index.md) - Connect to and query databases
- [Executable Blocks](blocks/executable/README.md) - Run scripts and commands  
- [Network Blocks](blocks/network/README.md) - Make HTTP requests and SSH connections
- [Templating System](templating.md) - Create dynamic, reusable runbooks
- [Secrets Management](secrets.md) - Handle credentials securely

You now have everything you need to create your first runbook. Start simple with some documentation and a few blocks, then gradually add more automation as you become comfortable with the system.
