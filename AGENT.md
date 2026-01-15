# Atuin Desktop Agent Guide

## Build & Test Commands
- Dev: `bun install && ./script/dev [--profile profile_name]`
- Build: `bun run tauri build`
- Frontend tests: `bun test`
- Run single test: `bun test-once <test-name>`
- Development: `./script/dev` (recommended)
- Test single file: `bun test src/path/to/test.ts`
- Tauri commands: `bun run tauri`

## Code Style Guidelines
- **Branches**: ALWAYS create a branch before making changes. NEVER commit directly to main. Use format `<username>/<issue-id>/short-description` (e.g., `ellie/ATU-194/script-default-shell`)
- **Commits**: Follow conventional commits (feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)
  - Use concise, single-line commit messages
  - Format: `type(scope): brief description` (e.g., `feat(terminal): add custom shell selection`)
  - Split different features or fixes into separate commits
  - Keep changes focused and atomic
- **Comments**: Avoid redundant comments; only add WHY not WHAT
- **Issue Management**: Never add comments to Linear issues, only update status when requested
- **Rust**: Use Rust 1.84, async/await pattern with proper error handling via eyre/thiserror
- **TypeScript**: Use TypeScript for frontend with React components
- **Styling**: Use Tailwind CSS with shadcn components
- **Error Handling**: Use Result types in Rust, try/catch in TypeScript
- **Naming**: Use descriptive names, PascalCase for components, snake_case for Rust

Prioritize clear, maintainable code with minimal comments and proper error handling.

## Terminal Implementation
- PTY handling uses portable_pty crate in backend/src/pty.rs with CommandBuilder for shell management
- Default shell is provided by CommandBuilder::new_default_prog()
- Custom shells can be specified in terminal settings (settings.runbooks.terminal.shell key)
- Frontend code passes shell setting to backend via Tauri invoke command "pty_open"
- Error handling for shell execution uses toast notifications (pattern matches SSH error handling)
- Terminal settings state is managed in src/state/settings.ts using KVStore
- Terminal settings UI is in src/components/Settings/Settings.tsx in RunbookSettings component

## Project Structure
- Frontend: React + Vite + TypeScript + Tailwind
- Backend: Tauri + Rust
- Tests: Vitest for frontend, cargo test for backend

## Tracking & Analytics
- **Implementation**: src/tracking.ts with PostHog + Sentry integration
- **Privacy-first**: No PII in analytics events, only event names and non-identifying properties
- **User state**: Subscribe to useStore user changes for login/logout detection
- **App lifecycle**: Tauri window events (focus/blur/close) tracked in src/main.tsx
- **Anonymous users**: Use system UUID for user journeys without creating identified profiles
- **Events tracked**: app.start/focus/blur/close, user.login/logout/register, runbook operations
- **Opt-out**: Respects usage_tracking setting in KVStore

## Runbook Editor System

### Block Architecture
- Blocks are created using BlockNote React editor
- Each block type needs:
  1. A React component for the UI
  2. Registration with createReactBlockSpec
  3. Addition to the schema in src/components/runbooks/editor/create_editor.ts
  4. Addition to the slash menu in src/components/runbooks/editor/Editor.tsx

### Template Variables
- Variables can be created with Var blocks
- Backend commands:
  - set_template_var: Store variables in Rust backend
  - get_template_var: Retrieve variables from backend
- Implementation in backend/src/commands/template.rs
- Variables scoped to specific runbook IDs

### Adding New Blocks
1. Create component in src/components/runbooks/editor/blocks/[BlockName]/index.tsx
2. Export with createReactBlockSpec({ type, propSchema, content }, { render })
3. Import & add to schema in create_editor.ts
4. Create slash menu item in Editor.tsx with appropriate icon and description