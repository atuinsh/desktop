# Atuin Desktop Memory

## Build & Test Commands
- Build: `bun run build`
- Development: `bun run dev` or `./script/dev` (recommended)
- Test all: `bun test`
- Test single file: `bun test src/path/to/test.ts`
- Test once: `bun test-once`
- Tauri: `bun run tauri`

## Code Style Guidelines
- TypeScript: Strict mode, no unused locals/parameters
- Format: Prettier with 2-space indentation, 100 char line limit
- Imports: Use path aliases (@/* resolves to ./src/*)
- Naming: PascalCase for components, camelCase for functions/variables
- Error handling: Use Option/Result pattern from @binarymuse/ts-stdlib
- Tests: Use Vitest with describe/test/expect
- React: Use functional components with hooks
- State: Zustand for global state management
- Tracking: Use unified event names with type parameters (e.g., `track_event("runbooks.block.execute", {type: "terminal"})`)
- Workflow: Commit changes with conventional commit messages after implementation

## Project Structure
- Frontend: React + Vite + TypeScript + Tailwind
- Backend: Tauri + Rust
- Tests: Vitest for frontend, cargo test for backend

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